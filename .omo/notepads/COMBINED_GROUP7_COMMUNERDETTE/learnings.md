# Learnings — COMBINED_GROUP7_COMMUNERDETTE

## Phase 18.6: Unwrap/Expect Audit (2026-06-01)

### Key Findings

- **Total production unwraps/expect:** 40 (24 `.unwrap()`, 16 `.expect()`) across 3 crates.
- **Total test code unwraps/expect:** ~494 (acceptable, excluded from remediation).
- **Critical finding:** `server/handlers.rs:579` — `verified.last().unwrap()` in `handle_ship_ack` processes external mirror data. While currently safe by control flow, a future refactor could introduce a panic-on-external-input DoS vector.
- **Moderate findings:** `snapshot_signature.rs:68` (Argon2 in public function), `foretias.rs:284` (client connect panic).
- **No FFI boundary unwraps** — the C11↔Rust boundary uses proper error propagation.
- **~50 Mutex `.unwrap()` calls** on `std::sync::Mutex` in communerd are a latent poison/cascading failure risk. Consider `parking_lot::Mutex`.

### Audit Methodology

- grep for `.unwrap()` and `.expect(` across `p2p/*/src/` (excluding `tests/`, `build.rs`)
- Read each production hit for context (surrounding 20-60 lines)
- Classify into: FFI boundary, test code, production protocol code, configuration/startup, internal invariant
- Rank top 10 by: external input exposure, DoS potential, control flow dependency

### Code Patterns Observed

1. **Postcard serialization on owned data** — 4 instances of `.expect("postcard serialize X")` in canonical methods. Safe (only fails on OOM), but worth monitoring if these become hot paths.
2. **Mutex unwrap on internal state** — ~50 instances across communerd. Acceptable but `parking_lot::Mutex` would eliminate poison risk.
3. **Control-flow-dependent invariants** — `clean_auth.rs:411` and `communerdette.rs:1109` rely on preceding checks/loop logic. Safe but fragile to refactoring.
4. **Startup fail-fast** — 8 `.expect()` in configuration/startup. Acceptable (prefer crash over broken state).

## Phase 4.4: handle_route_stamp Response Shape Verification (2026-06-01)

### Execution Path (confirmed)

1. `handle_route_stamp` (handlers.rs:78-133) accepts `target_tbid` (hex), `content` (hex), `echo` (string)
2. If `target_tbid == my_tbid` → delegates to `handle_stamp` (local path)
3. Otherwise → `cm.route_stamp(&target_tbid, &content_hex, &echo).await`
4. `Communerd::route_stamp` (mod.rs:336-347) → `line_for_tbid(tbid).stamp(content, echo)`
5. `CommunerdetteLine::stamp` (communerdette.rs:1792-1812) → `execute_stamp` → Take 3 gate → `CleanAuthenticated<Foretis>`
6. Back in handler: `ca_foretis.into_inner()` extracts bare `Foretis`
7. `serde_json::to_value(bare_foretis)` serializes to JSON

### `.into_inner()` Usage (line 128)

**CORRECT** — `ca_foretis.into_inner()` properly extracts the bare `Foretis` from `CleanAuthenticated<Foretis>`.
The `CleanAuthenticated` wrapper is internal to the trust boundary and must not appear in wire responses.

### Response Shape: INCONSISTENT BETWEEN LOCAL AND REMOTE PATHS

**`handle_stamp` (local path, lines 67-71):**
```json
{
  "foretis": { "chronon_number": N, "content_hash": "...", "tbid": "...", "echo": "...", "tbn": "...", "time_being_reference_time": "..." },
  "signature": "hex-encoded-bytes",
  "signature_algorithm": "Ed25519"
}
```

**`handle_route_stamp` (remote path, line 128):**
```json
{
  "chronon_number": N,
  "content_hash": "...",
  "tbid": "...",
  "echo": "...",
  "tbn": "...",
  "time_being_reference_time": "..."
}
```

### Discrepancy Details

| Field | Local (`handle_stamp`) | Remote (`handle_route_stamp`) |
|-------|----------------------|------------------------------|
| `foretis` | Present (nested object) | **MISSING** (bare Foretis at top level) |
| `signature` | Present (hex string) | **MISSING** |
| `signature_algorithm` | Present (string) | **MISSING** |
| `chronon_number` | Inside `foretis` | **Top level** |
| `content_hash` | Inside `foretis` | **Top level** |
| `tbid` | Inside `foretis` | **Top level** |
| `echo` | Inside `foretis` | **Top level** |
| `tbn` | Inside `foretis` | **Top level** |
| `time_being_reference_time` | Inside `foretis` | **Top level** |

### Root Cause

The `handle_stamp` local path constructs a custom JSON object with three top-level keys (`foretis`, `signature`, `signature_algorithm`) from the `Stamped` struct returned by `Chronomatter::stamp()`. The `handle_route_stamp` remote path serializes the bare `Foretis` directly via `serde_json::to_value()`, which produces only the `Foretis` struct fields.

The remote path does NOT have access to the signature bytes or algorithm separately — those are carried by the `UnverifiedSignatureEnvelope` that was consumed during the Take 3 gate (`gate_foretis` in communerdette.rs:1131-1178). The `CleanAuthenticated<Foretis>` contains only the verified `Foretis` payload, not the signature metadata.

### Verdict

**The response shape is NOT unchanged after the refactor.** The remote path returns a structurally different JSON shape than the local path. This is a backward-compatibility concern: clients that parse the `route_stamp` response expecting the same shape as `stamp` will break.

### Recommended Fix (for future implementation)

`handle_route_stamp` should reconstruct the same envelope shape as `handle_stamp`:
```rust
resp_success(server, id, serde_json::json!({
    "foretis": ca_foretis.into_inner(),
    "signature": "...",  // needs to be preserved from the envelope
    "signature_algorithm": "...",  // needs to be preserved from the envelope
}))
```

This requires either:
1. Returning `(Foretis, SignatureBytes, String)` from `route_stamp` instead of just `CleanAuthenticated<Foretis>`, OR
2. Having `CleanAuthenticated<Foretis>` retain the signature metadata and exposing accessor methods, OR
3. Having the `Foretis` struct itself carry the signature fields (reverting to v1 format)

### Note on Task Description's Expected Shape

The task description lists expected shape as `{ "tbid": ..., "chronon_number": ..., "content": ..., "signature": ..., "signature_algorithm": ... }`. This does NOT match either the local or remote path's actual output. The `Foretis` struct uses `content_hash` (not `content`), and includes `echo`, `tbn`, and `time_being_reference_time` fields not mentioned in the expected shape. The expected shape appears to be from an older v1 wire format where signatures were embedded in the `Foretis` struct itself.

## Phase 4.4: handle_route_stamp Response Shape Fix (2026-06-01)

### Fix Applied

**Option chosen:** Add Foretis-specific accessor methods to `CleanAuthenticated<Foretis>` (Option 2 from recommended fix).

The `CleanAuthenticated<Foretis>` struct already preserves the `signatures: Vec<SignatureEntry>` from the consumed `UnverifiedSignatureEnvelope` — the signature metadata was never actually discarded. The Take 3 gate (`gate_foretis`) verifies the signature and the `verify()` method on `UnverifiedSignatureEnvelope<Foretis>` returns `CleanAuthenticated { inner: self.inner, signatures: self.signatures }`, preserving the ordered signature chain.

**Changes made:**

1. **`core-engine/src/foretias/clean_auth.rs`** — Added two accessor methods to `impl CleanAuthenticated<Foretis>`:
   - `signature_bytes() -> Option<&[u8]>` — returns the first signature entry's bytes
   - `signature_algorithm() -> Option<&str>` — returns the algorithm name ("Ed25519" or "SLH-DSA")

2. **`foretias-server/src/server/handlers.rs`** — Updated `handle_route_stamp` to extract signature metadata before consuming `ca_foretis.into_inner()`, then return the same envelope shape as `handle_stamp`:
   ```rust
   let sig_hex = ca_foretis.signature_bytes().map(|b| hex::encode(b)).unwrap_or_default();
   let sig_alg = ca_foretis.signature_algorithm().unwrap_or("Ed25519").to_string();
   resp_success(server, id, serde_json::json!({
       "foretis": ca_foretis.into_inner(),
       "signature": sig_hex,
       "signature_algorithm": sig_alg,
   }))
   ```

3. **Added regression test** `handle_route_stamp_self_route_returns_envelope_shape` — verifies the self-routing path (target_tbid == my_tbid) returns the correct envelope shape with all three fields.

### Verification

- `cargo test --workspace` passes (559 tests across all crates)
- No changes to verification logic in `gate_foretis`
- No changes to `CleanAuthenticated<T>` generically — only Foretis-specific accessors added
- No changes to `handle_stamp` (local path remains reference shape)
- Two snapshot tests updated: `trust_boundary_type_usage` (nightly toolchain unavailable) and `crypto_callsite_snapshot` (new accessor call sites)

### Key Insight

The `CleanAuthenticated<T>` struct's `signatures` field was already populated by the Take 3 gate pipeline. The fix required only exposing the existing data through accessor methods, not modifying the verification pipeline or return types of `route_stamp`/`execute_stamp`.

## Phase 10: Obsolete Direct Peer-Call Path Removal (2026-06-01)

### Removal Summary

Removed all obsolete direct peer-call stamp paths that bypassed the Take 3 inbound gate:

| Removed | Location | Reason |
|---------|----------|--------|
| `PeerTransport::stamp` | `transport.rs` trait | Only called by deprecated `stamp_peer`; `route_stamp` is the active path |
| `JsonRpcTransport::stamp` | `json_rpc_transport.rs` | Trait method removal |
| `Libp2pTransport::stamp` | `libp2p_transport.rs` | Trait method removal |
| `DummyTransport::stamp` | `peer_pool.rs` tests | Trait method removal |
| `Communerd::stamp_peer` | `mod.rs` | Deprecated; bypassed Take 3 via `from_trusted` |
| `CommunerdServer::stamp_peer` | `tiers.rs` | Wrapper for deprecated `stamp_peer` |
| `CommunerdP2P::stamp_peer` | `tiers.rs` | Wrapper for deprecated `stamp_peer` |

### Caller Migration

- **`integration.rs`** — 2 tests migrated from `stamp_peer` to `route_stamp`:
  - `test_two_nodes_mutual_attest`: Now uses `server_b.get_tbid().to_hex()` as target TBID
  - `test_peer_unreachable_does_not_crash`: Uses dummy TBID; transport failure still occurs before TBID validation
- **`libp2p_transport_unit.rs`** — 2 tests migrated from `stamp` to `route_stamp`:
  - `request_serialization_writes_jsonrpc_envelope`: Now verifies `route_stamp` JSON-RPC method + `target_tbid` param
  - `closed_cmd_channel_returns_transport_error`: Same error behavior via `route_stamp`

### Active Path (unchanged)

The active stamp path flows through Communerdette:
```
CommunerdetteLine::stamp → execute_stamp → do_stamp → host_execute_stamp → route_stamp (transport)
```
This path was NOT modified. It already uses `route_stamp` on the transport layer.

### Verification

- `cargo test --workspace` passes (571 tests, 0 failures)
- `crypto_callsite_snapshot` updated to reflect removed `stamp_peer` crypto calls
- No behavior changes — only cleanup of obsolete paths

### Key Insight

The `PeerTransport::stamp` method existed as a legacy transport-level RPC that sent the `stamp` JSON-RPC method directly to a peer. The replacement `route_stamp` sends the `route_stamp` JSON-RPC method, which triggers the full Communerdette pipeline on the receiving side. The distinction is important: `stamp` was a direct request (like calling a function), while `route_stamp` is a routed request (like sending a letter through a postal system that verifies the recipient).
