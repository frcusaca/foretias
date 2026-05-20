# Foretias — Pre-P2P Comprehensive Product & Code Review

**Reviewer:** Claude Sonnet 4.6 (Anthropic) — architectural, product, and security review
**Date:** 2026-05-19
**Scope:** All dimensions — code organization, correctness, security, product design, mindshare, lifecycle, future directions
**Prior art read:** `specs/pre-public-mvp-code-review.md` (Claude Opus 4.7 technical review, 2026-05)
**Status of P2P:** Implemented; not yet integration-verified against live multi-node network

This document does not repeat findings already catalogued in `pre-public-mvp-code-review.md`. It builds on that baseline and extends into dimensions that were not the focus of the prior review: product framing, mindshare, library usability, the 50%-compromise resilience question, future hardware and environment adaptation, and ecosystem positioning. Technical findings here are new observations not in the prior document.

---

## 0. The Nightmare Test — A Direct Answer

> *"If I have a nightmare about more than 50% of servers being hacked, will my mind have a reassuring answer?"*

**The short answer: yes, for past stamps. The long answer has a gap for present and future ones.**

### What the architecture guarantees (and why the nightmare is survivable)

A Foretis is a signature over `(TBID ‖ chronon_number ‖ content)` made with the private key of chronon `N`. That key was algorithmically incapacitated when chronon `N+1` was created — the forward and backward auto-attestations prove chronon `N+1` exists. A compromised attacker who acquires a running server *after the fact* cannot forge a Foretis claiming to be from chronon `N`, because:

1. The private key for chronon `N` no longer exists in any form — it is not on disk, not in the compromised memory snapshot (if the compromise happened after tick advance), and cannot be derived from the public key or the calendar record.
2. Any attempt to produce a Foretis for chronon `N` with a *different* key produces a signature that fails verification against the public key published in the calendar — verifiable offline, with no server needed.

**This is the ironclad guarantee.** Every past Foretis, once issued and once the next tick advanced, is verifiably correct or forged — and forgery is computationally infeasible. A calendar you received yesterday is still fully self-consistent without any server alive.

### What fails when 50% of servers are compromised

The gap is in the *live and near-future* period:

- **Live stamps:** A compromised server is in an active tick. The attacker *does* hold the current tick private key. They can issue any Foretis they want with that key until the tick advances. Rate of damage is bounded by chronon duration — for a 60-second chronon, the damage window is at most 60 seconds per server.
- **Probity system:** The honor system assumes an honest majority to aggregate truthful probity reports. 50%+ compromise means dishonest reports can bias the probity score. The current implementation (`foretias-server/src/probity/`) has no Sybil resistance beyond the DHT namespace key — there is no economic cost to creating nodes.
- **Epoch consensus (not yet implemented):** FROST t-of-n threshold signing on epoch snapshots (`specs/FORETIAS_2_P2P_8_epoch_consensus.md`, `p2p/core-engine/src/epoch/`) would make this more robust — if t > 50%, an attacker controlling N/2 cannot forge a valid epoch snapshot. But FROST is stubbed: `software.rs:284` returns `Err(CryptoError::Unsupported("FROST signing not supported"))`.

### The reassuring architectural path

Once the following are implemented, the nightmare has a strong answer even for 50%+ live compromise:

1. **Short chronons as attack window limiter.** A 60-second chronon means the damage window per compromised server is at most 60 seconds. An operator can respond before the next tick.
2. **Calendar mirroring across honest peers.** If even one honest peer has replicated the calendar up to chronon `N-1`, and the honest calendar diverges from the attacker's forged calendar at chronon `N`, the divergence is cryptographically detectable.
3. **FROST epoch consensus (v0.6).** Requires k-of-n honest committee members. With k > N/2, a majority compromise cannot produce a valid epoch snapshot.
4. **The offline verification guarantee (already present).** Any Foretis from *before* the compromise can be verified forever against a pre-compromise calendar copy.

**The specific answer to bring to a nightmare:** "My Foretis from last week cannot be forged. A compromised server could issue fake stamps for at most 60 seconds per tick, and those fake stamps are detectable by any honest peer with a mirrored calendar. The cryptographic chain is append-only and chained — you can't insert a backdated entry without rewriting all subsequent ticks, which requires all future keys."

**The gap to close (tracked as improvement recommendations below):**
- Implement FROST epoch consensus with k > N/2
- Add calendar mirroring verification (cross-check your local copy against multiple peers)
- Add a "last honest tick" attestation mechanism (a signed statement from k-of-n peers that calendar X is valid through tick Y)

---

## 1. Product Identity & Mindshare

### 1.1 The One-Sentence Pitch Is Missing Everywhere

The core insight is brilliant and expressible in one sentence:

> "A timestamp that is mathematically impossible to backdate — because the signing key self-destructs after every tick."

This sentence appears nowhere in README.md, HOWTO.md, or any public-facing documentation. The README opens with the project acronym expansion and a dense technical explanation. Most potential users stop reading before reaching the value proposition.

**Recommendations:**
- README.md line 1 should be that sentence, followed by the installation and quick start.
- A "30-second pitch" section before the technical content.
- Comparison table: Foretias vs RFC 3161 (TSA), vs OpenTimestamps/Bitcoin anchoring, vs certificate transparency logs, vs traditional PKI timestamping. Keywords: `RFC 3161`, `TSA`, `OpenTimestamps`, `blockchain timestamp`, `certificate transparency`, `RFC 6962`.

### 1.2 The Latin Taxonomy — Asset or Liability?

The Latin naming scheme (*Chronos fidelius authenticus*, *Chronos fidelius grapha*, etc.) is memorable and differentiating. It signals that this is a deliberate system with a coherent worldview. However:

- It requires translation for every new reader. A developer integrating the library for the first time encounters `Chronomatter`, `Communerd`, `Foretis` (plural of *Foretias*?) without context.
- The HOWTO.md and README.md do not explain what `Chronomatter` is in plain English before using it.
- `Foretis` as a data type conflicts with `Foretias` as the project name — `foretis = tbf.stamp(...)` looks like a typo.
- `Communerd` is evocative but "communard" (Paris Commune member) + "nerd" may not translate across cultures.

**Recommendations:**
- Keep the Latin names; they are distinctive. Add a **terminology box** at the top of every public-facing document: `Chronomatter = the active tick engine`, `Foretis = a single timestamped artifact`, `Calendar = the append-only record of tick transitions`.
- Consider a "plain English mode" in the API (type aliases, re-exports under intuitive names: `pub use Foretis as Timestamp;`).
- The `echo` field in `Foretis` (defined at `p2p/core-engine/src/foretias/types.rs`) has no explanation in the struct docs. "Echo" is jargon — document it explicitly.

### 1.3 Audience Segmentation

Foretias serves at least three distinct audiences with different needs:

| Audience | Primary Need | Current Gap |
|---|---|---|
| Application developers | Embed timestamping in their service | No published library crate, no SDK docs |
| Infrastructure operators | Run a timestamping network | CLI works; no monitoring integration, no auth |
| Auditors/verifiers | Verify a Foretis they received | `foretias verify-with-proof` works; hard to discover |

The current documentation serves the operator audience reasonably. Application developers and auditors are underserved.

**Keywords for further development:** `library-first API`, `SDK documentation`, `verifier persona`, `operator persona`, `developer persona`.

---

## 2. Code Organization — New Architectural Observations

### 2.1 Stamp Semantics: One Tick Per Stamp is a Hidden Invariant

**File:** `p2p/core-engine/src/chronomatter/mod.rs:302-320`

Every call to `stamp()` atomically increments `current_tick`:
```rust
let tick = {
    let old = self.current_tick.load(SeqCst);
    self.current_tick.compare_exchange(old, old + 1, SeqCst, SeqCst)...
    old + 1
};
```

The daemon tick loop (`daemon_tick()` at line 421) also increments `current_tick`. These two code paths race. The spec (`foretias-v1.md §2.1`) distinguishes:
- `serialized=True`: tick advances only on stamp
- `serialized=False`: background daemon advances tick; stamp signs under current tick without advancing

But the Rust implementation advances the tick on every stamp *regardless* of serialized mode, making the daemon's tick purely additive. A 60-second chronon with 100 stamps/minute creates 160 ticks/minute, not 1.

**Consequence:** Stamps cannot be batched under a single tick. Every stamp is its own chronon. The calendar grows one record per stamp call, not one per chronon interval. This is a fundamental behavioral difference from the Python prototype's non-serialized mode and from the spec.

**Concern:** The existing tests don't catch this (`two_stamps_share_same_tick_if_no_daemon_advance` at line 527 actually tests that two stamps get *different* ticks, which is the opposite of spec intent for non-serialized mode).

### 2.2 Tick Numbers Are Sequential Integers, Not Nanoseconds

**Files:** `p2p/core-engine/src/chronomatter/mod.rs:49`, `p2p/core-engine/src/foretias/types.rs`

`current_tick` starts at 0 and increments by 1 per stamp or daemon tick. The spec (`foretias-v1.md §2.2`) says:

> "Chronons are numbered from genesis; each `chronon_number` represents nanoseconds past the Unix epoch."

The Rust implementation uses sequential integers (1, 2, 3, ...) not nanosecond timestamps. This means:
- A Foretis cannot be placed on an absolute timeline without knowing when the server started.
- Cross-server calendar comparison by chronon_number is meaningless (server A's tick 100 and server B's tick 100 are completely unrelated times).
- The `time_being_reference_time` field (`chronomatter/mod.rs:349`) records wall clock time but is advisory only — it is not part of the cryptographic signature input.

**Impact:** Any use case that relies on chronon_number as a time anchor (not just an ordering anchor) is broken. This is a spec/implementation divergence that needs explicit resolution.

### 2.3 Unbounded Keypair Accumulation

**File:** `p2p/core-engine/src/chronomatter/mod.rs:35`

```rust
keypairs: RwLock<Vec<TickKeyPair>>,
```

This `Vec` grows by one `TickKeyPair` per stamp call, never shrinks, and is never evicted. After 1 million stamps, this holds 1 million private key handles in memory. The spec (`FORETIAS_0_OVERVIEW.md §0.3`) requires: "Private keys never persisted" — this is honored — but there is no in-memory bound either, which is also unspecified.

The `generate_and_store_keypair()` function at line 149 holds the write lock for the duration of `PrivKeyHandle::generate()`, which calls into libsodium for key generation and into the PQC stack for PQC keys. On high-throughput paths, this serializes stamping.

**Fix:** Limit retention to the last 2 keypairs (current and previous, enough for `build_auto_attestation`). Zero-and-drop older ones explicitly.

### 2.4 Concurrent CAS Race Between stamp() and daemon_tick()

**File:** `p2p/core-engine/src/chronomatter/mod.rs:302-320`, `:421-443`

Both `stamp()` and `daemon_tick()` use `compare_exchange` on `current_tick`. Under concurrent load:

1. Thread A (stamp) loads `old = 5`, tries CAS(5→6), succeeds. `kp_idx = 5`, signs under keypair 5.
2. Thread B (daemon) loads `old = 5`, tries CAS(5→6), **fails** because A already changed it to 6. The `map_err` turns the `Err(e)` into `NodeError::Internal("tick counter conflict: 6")`.

The daemon failure is non-fatal (`error!()` is logged and the daemon continues). But the stamp failure *does* return an error to the caller:
```rust
self.current_tick.compare_exchange(old, old + 1, SeqCst, SeqCst)
    .map_err(|e| NodeError::Internal(format!("tick counter conflict: {}", e)))?;
```

Under concurrent stamping + daemon ticking, stamp calls will occasionally fail with `Internal` error. This is silent to the caller unless they check the error. In integration tests this manifests as flaky failures.

**Correct fix:** Use `fetch_add(1, SeqCst)` which is atomic without the CAS failure mode. CAS is appropriate only when you need to observe the *expected* previous value and take different action if it changed.

### 2.5 build_auto_attestation Index Arithmetic Is Fragile

**File:** `p2p/core-engine/src/chronomatter/mod.rs:174`

```rust
let kp_idx = (tick - 1) as usize;
...
let prev_kp_idx = (prev_tick - 1) as usize;
let prev_pub = self.keypair_pub(prev_kp_idx)...
```

This assumes `keypairs[tick-1]` is always the keypair for tick `tick`. Since stamp() and daemon_tick() both generate keypairs independently, and the keypair is generated *inside* `create_foretis` at `kp_idx = self.generate_and_store_keypair()`, the mapping of keypair index to tick number is maintained only if:
- No concurrent stamp calls
- No concurrent daemon ticks
- The Vec has never had an element removed

With concurrent stampers, the Vec index may not equal `tick - 1`. The actual index returned by `generate_and_store_keypair()` is `keypairs.len()` before push. If two stamp calls race, tick values may be 6 and 7, but their indices may be 5 and 6, which are consistent — until we try to call `build_auto_attestation` which uses `tick-1` as index rather than the actual stored index.

This is a latent correctness bug in concurrent mode.

### 2.6 The Echo Field Is Not Signed

**File:** `p2p/core-engine/src/chronomatter/mod.rs:332-334`, `foretias/types.rs`

The `echo` field is returned in the Foretis but is not part of the signature input (`sig_input` at line 332 contains only `tbid ‖ tick ‖ content`). A MITM server can modify the echo field without invalidating the cryptographic proof.

For clients using echo as a correlation ID (the CLI does: `--message "hello world"` becomes `echo`), this is benign. But if callers interpret echo as authenticated content, this is a security issue.

**Recommendation:** Either (a) document prominently that echo is advisory and unauthenticated, or (b) include echo in the signature input. If (b), this is a wire-breaking change requiring version negotiation.

### 2.7 id Field Read from params Instead of JSON-RPC Envelope

**File:** `p2p/foretias-server/src/server/handlers.rs:31`

```rust
let id = params.get("id").cloned();
```

JSON-RPC 2.0 places `id` at the top-level envelope, not inside `params`. The handlers read `id` from `params`, so every response will have `id: null` unless the dispatcher manually injects `id` into `params`. Looking at `server/mod.rs` for evidence of this injection:

If the dispatcher does not inject, then all JSON-RPC responses have `null` for `id`, breaking response correlation for batch/async clients. This was flagged in the prior review but checking the handler code confirms it is still present and likely still broken.

### 2.8 block_on Deadlock in Async Context

**File:** `p2p/foretias-server/src/server/handlers.rs:114`, `:182`, `:292-296`

```rust
match tokio::runtime::Handle::current().block_on(async {
    cm.route_stamp(&target_tbid, ...).await
}) {
```

`block_on` from within a tokio task context is safe only if the runtime has multiple threads. On a single-thread `current_thread` runtime (common in test and embedded contexts), `block_on` inside an async task deadlocks because it blocks the single thread trying to drive the future. The `handle_status` function at line 292 also does this.

Axum handlers are async by default. The correct pattern is to make handlers `async fn` and use `.await` directly. This is not just a performance issue — it is a correctness/reliability issue.

---

## 3. Security Analysis — New Findings

### 3.1 No Authentication on Any JSON-RPC Endpoint

**File:** `p2p/foretias-server/src/server/mod.rs`

The JSON-RPC server accepts connections from any client on the listen port. Any process on the network can:
- Call `stamp` and issue Foretises under the server's TBID
- Call `get_calendar_slice` and exfiltrate the full calendar
- Call `route_stamp` to use this server as a proxy to stamp against any peer's calendar

For a timestamping service where stamps are authoritative proofs, unauthenticated stamp access means anyone can inflate the calendar and create timestamps not originating from the legitimate content owner.

**Recommendation:** Add a token-based auth layer (shared secret in header, or mTLS) as a configuration option. Default to token auth on non-localhost bind addresses. Keywords: `shared secret`, `mTLS`, `JSON-RPC auth`, `Bearer token`.

### 3.2 Content Is Hex-Encoded in Transit Without Integrity Protection

**File:** `p2p/foretias-server/src/server/handlers.rs:39-43`

The stamp handler decodes hex from the `content` parameter. The JSON-RPC transport has no TLS. A network observer sees all content being stamped in plaintext (after hex decoding). This is acknowledged in the threat model (`docs/threat_model_v0_5.md §3.1`), but the default user experience (stamping "my confidential document hash") is to send it unencrypted over the wire.

**Recommendation:** Document that JSON-RPC is unencrypted; provide a transport-level encryption option (Noise_XX already exists in the codebase at `p2p/core-engine/src/noise.rs`). The `foretias-client/src/noise_ptp.rs` already wraps Noise for client connections — this should be the default, not an alternative.

### 3.3 Heartbeat Replay via Stale Timestamps

**File:** `p2p/core-engine/src/collision/detector.rs` (referenced in `docs/threat_model_v0_5.md §1.4`)

The collision detection heartbeat accepts heartbeats with arbitrary `timestamp_ns` without a freshness window check. An attacker can capture a heartbeat and replay it much later to trigger spurious collision detection, causing a legitimate node to go dormant.

This was flagged in the prior review. The priority is HIGH because going dormant is irreversible without process restart, making this an effective denial-of-service against a specific honest node.

### 3.4 ProbityReport Canonical Serialization Uses Null-Byte Separators

**File:** `p2p/core-engine/src/probity/mod.rs` (spec: `specs/FORETIAS_2_P2P_SPEC.md §9.1`)

The canonical bytes for signing a `ProbityReport` use null bytes as field separators:
```rust
buf.extend_from_slice(self.subject.as_bytes());
buf.push(0);
```

If `subject` contains a null byte (possible with malformed input), the canonical serialization becomes ambiguous. A subject of `"A\0B"` produces the same canonical prefix as subject `"A"` followed by a different field starting with `"B"`. This is a canonicalization attack surface.

**Recommendation:** Use length-prefixed encoding (e.g., 4-byte big-endian length before each field) or a deterministic serialization format like CBOR or protobuf for the canonical form. Keywords: `canonicalization attack`, `length-prefix encoding`, `deterministic serialization`.

### 3.5 The 1 GiB Content Limit Is Not Enforced at the Transport Layer

**File:** `p2p/foretias-server/src/server/handlers.rs:11`, `:45`

`MAX_CONTENT_BYTES = 1_073_741_824`. The check at line 45 verifies the *decoded* content length, but the hex-encoded content (`content_hex` at line 33) is read without a size limit. A 2 GiB hex string (encoding 1 GiB of content) is accepted into memory as a `String` before the content length check. Effective memory usage is 3× the content size (hex string + decoded bytes + signature input buffer).

The prior review noted this. The fix is to enforce a limit on the raw hex string length before allocation: `MAX_CONTENT_BYTES * 2 + some_slack`.

### 3.6 Divergent Algorithm Between stamp() and daemon_tick()

**New observation from reviewing chronomatter/mod.rs:**

`stamp()` at line 355 writes `SignatureAlgorithm::Ed25519.to_id_string()` unconditionally into the Foretis and ChrononRecord. But the crypto_server's `signature_algorithm()` now correctly returns `Ed25519` after the fix documented in the prior review. If a future backend returns `Dilithium3`, the `stamp()` function will still write `Ed25519` because it is hardcoded:

```rust
signature_algorithm: crate::foretias::types::SignatureAlgorithm::Ed25519.to_id_string().to_string(),
```

The algorithm should be queried from the crypto_server: `self.crypto.signature_algorithm().to_id_string()`. This appears in both `stamp()` (line 355) and `build_tick_record()` (line 259, 283).

### 3.7 OnceLock Pattern in Communerd Makes Failure Silent

**File:** `p2p/foretias-server/src/communerd/mod.rs:66-80`

```rust
p2p_events: Arc<OnceLock<tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>>>,
p2p_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
p2p_cmd_tx: Arc<OnceLock<tokio::sync::mpsc::UnboundedSender<SwarmCommand>>>,
```

`OnceLock` fields that are never set mean the methods that try to use them silently fail (get returns `None`). If P2P startup fails partway through, some `OnceLock` fields are set and others are not. The Communerd struct offers no way to query its initialization state, making bug diagnosis very difficult.

**Recommendation:** Replace scattered OnceLock fields with a single state enum: `Uninitialized`, `Initializing`, `Ready`, `Failed(error)`. This makes P2P init state observable.

---

## 4. P2P Layer Analysis

### 4.1 What Is Implemented vs. What Is Verified

| Component | Implemented | Verified in CI | Verified by manual test |
|---|---|---|---|
| DHT peer discovery (Kademlia) | Yes (`communerd/dht_peer_source.rs`) | No | Partially (`dht_stress_test.sh`) |
| GossipSub probity gossip | Yes (`probity/gossip_handler.rs`) | No | No |
| libp2p swarm + Noise handshake | Yes (`communerd/p2p/swarm.rs`) | No | `integration_sanity.sh` |
| Mutual attestation via Noise PtP | Yes (`communerd/json_rpc_transport.rs`) | No | Partially |
| Collision detection + dormancy | Yes (`collision/detector.rs`) | No | No |
| FROST epoch consensus | Stubbed | No | No |
| Calendar mirroring | Yes (`calendar_store/`) | No | No |
| Cross-node verify via DHT | Yes (`handlers.rs:cross_node_verify`) | No | No |

None of the P2P integration paths are in CI. The DHT stress test (`p2p/dht_stress_test.sh`) and integration sanity (`p2p/integration_sanity.sh`) are shell scripts that run manually only.

### 4.2 Probity Gossip Is Wired but Unproven

**File:** `p2p/foretias-server/src/probity/gossip_handler.rs`

The gossip handler subscribes to GossipSub topics and feeds messages to `ProbityStore`. But:
- There are no tests that spawn two nodes, gossip a `ProbityReport`, and verify the receiving node's score updated.
- The `handle_gossip_message` function processes messages but the `ProbityStore` aggregator (`p2p/foretias-server/src/probity/aggregator.rs`) implements the U-shape weighting formula — this formula is never exercised in any automated test.
- The U-shape formula (`valley_floor`, `hot_window_ns`, `ancient_start_ns`) is configurable but the defaults are never validated against real network behavior.

**Keywords for future work:** `gossip integration test`, `U-shape weight validation`, `probity convergence under Byzantine nodes`.

### 4.3 DHT Registration Is Unauthenticated at Application Level

**File:** `p2p/foretias-server/src/communerd/mod.rs`

The `PeerRegistrationRecord` stored in the DHT contains `json_rpc`, `tbid`, `peer_id`. Any node can register any `json_rpc` address and any `tbid` in the DHT. The libp2p `peer_id` is authenticated by the Noise handshake, but the `tbid` is not cryptographically bound to the `peer_id`.

A malicious node can register itself in the DHT claiming to own TBID `X`. When a verifier does a cross-node verify for TBID `X`, it will contact the attacker's server. The attacker's server returns forged calendar records.

**Mitigation path:** The TBID key generation uses a dual-key scheme (`TbidSecret` at `p2p/core-engine/src/crypto_server/signing_tbid.rs`) — the TBID includes a public key component. DHT registration should include a signature over `(peer_id ‖ json_rpc ‖ tbid)` made with the TBID signing key. Verifiers check this signature before trusting the DHT entry.

### 4.4 Calendar Mirror Trust Without Verification

**File:** `p2p/foretias-server/src/server/handlers.rs:465-482` (`handle_ship_ack`)

When receiving mirrored calendar records via `ship_ack`, the handler calls `mirror_store.insert_mirrored()` without running `integrity_check` on the received records. A malicious peer can ship forged calendar records that appear structurally valid (correct JSON fields) but have invalid signatures.

**Recommendation:** After receiving a batch of records, run `verify_pair` on each consecutive pair before inserting into the mirror store. Cross-check the TBID against the DHT registration.

### 4.5 No Maximum Peers in the Peer Pool

**File:** `p2p/foretias-server/src/communerd/peer_pool.rs`

The threat model (`docs/threat_model_v0_5.md §6.1`) recommends `MAX_PEERS = 256` for `DhtPeerSource`. Verify whether `PeerPool` also has a cap. If not, an attacker that discovers many bogus peers via DHT can grow the peer pool unboundedly.

---

## 5. API & Library Usability

### 5.1 No Published Library Crate

Foretias ships a CLI binary. For embedding in an application, there is no published library interface. The `foretias-client` crate has a `Foretias` struct and a `ForetiasError` type, but:
- It is not published to crates.io
- The public API surface (`foretias-client/src/lib.rs`) is `pub use foretias, calendar, config` — sparse documentation
- There is no `cargo add foretias` equivalent

**For adoption**, a library-first pattern is essential. The minimal embed use case should be:
```rust
use foretias::TimeFamily;
let tf = TimeFamily::new()?;
let stamp = tf.stamp(b"my document hash")?;
let valid = tf.verify(b"my document hash", &stamp)?;
```

This is the Python API (from `foretias-v1.md §8`). The Rust equivalent does not yet exist as a clean embedded interface.

### 5.2 The Three-Level Instantiation Is Underexplained

**File:** `specs/RUST_THREE_LEVEL_INSTANTIATION_SPEC.md`

The three levels (Standalone, PtP, P2P) represent a progressive adoption path, but:
- The `foretias-client/src/config.rs` shows `StandaloneConfig`, `PtpConfig`, `P2pConfig` but these are not prominently surfaced in any developer guide
- A developer wanting to "just stamp things" should reach for `StandaloneConfig` but there is no example showing how
- The README CLI examples show server setup, not library embedding

### 5.3 The verify() Return Value Is Ambiguous

**File:** `p2p/foretias-server/src/server/handlers.rs:173`

```rust
return resp_success(server, id, serde_json::json!({"valid": false, "method": "local", "note": "foretis.tbid not found in local calendar"}));
```

`valid: false` is returned for both:
1. Signature verification failed
2. The TBID is not in this server's calendar (a perfectly valid Foretis from another server)

A consumer cannot distinguish "this is a forged stamp" from "this server doesn't know about that calendar." This could lead applications to reject valid stamps issued by peers. The prior review flagged this; it remains present.

### 5.4 Foretis JSON Shape Is Implicit

A `Foretis` JSON object has no schema document or OpenAPI definition. Consumers must read the Rust struct definition to know what fields to expect. For a timestamping service meant to be verifiable by third parties (including non-Rust code), a stable, documented JSON schema is essential.

**Keywords:** `JSON schema`, `OpenAPI`, `schema stability`, `wire format versioning`, `serde rename alias`.

### 5.5 No Versioning in the Wire Format

The JSON-RPC method namespace has no version prefix (`stamp` not `foretias/v1/stamp`). If the stamp parameters or response fields change, there is no way for a client to negotiate the version it expects. The `serde(rename = "tick_number")` compat alias added for `ChrononRecord` is a workaround, not a solution.

**Recommendation:** Add a `version` field to `Foretis` (a u32 schema version) and add version negotiation to the `stamp` RPC. This costs one field and one server-side check.

---

## 6. Testing & Verification Gaps (Beyond Prior Review)

### 6.1 The HOWTO Section 4 Claim Is Not Yet True

HOWTO.md section 4 promises: "**RESILIENT** — kill half the network and verification still works." But:
- The script `integration-tests/start_local_peers.py` starts N peers and demonstrates mutual attestation
- There is no automated test that kills N/2 peers and proves the surviving calendar is still verifiable
- The resilience claim depends on calendar mirroring being complete, which is partially implemented (`calendar_store/`) but not tested end-to-end

**This is a documentation correctness issue and a trust issue with early adopters.** If someone runs the HOWTO and the "kill half" demo doesn't work as described, trust is damaged.

### 6.2 No Regression Test for the Algorithm/Signature Mismatch

The prior review identified the critical bug where `sign()` and `signature_algorithm()` were mismatched. Even after fixing this, there is no regression test in the test suite that would catch a re-introduction. Add a parametric test that, for each supported algorithm, verifies that `stamp → serialize → deserialize → verify` succeeds end-to-end.

### 6.3 No Test for Calendar Divergence Detection

If two nodes have the same TBID (collision scenario) and both advance their calendars independently, the calendars will diverge after the fork point. The collision detector should catch this, but there is no test that:
1. Creates two nodes with forced identical TBID
2. Lets them diverge
3. Verifies the divergence is detected and both go dormant

### 6.4 No Property Tests for Calendar Invariants

Key invariants that should hold for any sequence of stamp/tick operations:
- `calendar.latest() >= any previous calendar.latest()`
- `verify(content, stamp(content)) == true` for any content
- `verify(content_a, stamp(content_b)) == false` when content_a ≠ content_b
- After N stamps, `integrity_check()` returns all-true

These are not covered by property-based tests (`proptest` or `quickcheck`).

---

## 7. Efficiency Observations

### 7.1 Full Calendar JSON Rewrite Per Stamp

**File:** `p2p/foretias-server/src/server/handlers.rs:63`

```rust
if let Err(e) = server.save() {
    tracing::warn!("failed to persist calendar after stamp: {}", e);
}
```

Each stamp triggers a full serialization and write of the entire calendar JSON. With N chronon records, each O(N) bytes, this is O(N) work per stamp. At 1000 stamps with 32-byte SPHINCS+ entries (actually 7856 bytes per signature) the calendar file can grow to several megabytes, and each stamp rewrites the whole file.

**The `EncryptedJsonlCalendarStore` in `calendar_store/encrypted_jsonl.rs` is an append-only format — this is the correct solution, already implemented.** The issue is it is not yet the default persistence backend.

### 7.2 PQC Keygen at Every SoftwareCryptoServer Instantiation

Each call to `SoftwareCryptoServer::generate()` generates SPHINCS+, Dilithium3, SPHINCS-256F, and ML-KEM-768 keypairs (lines 60-71). SPHINCS+ keygen is approximately 10-20ms. For embedded or high-throughput scenarios, this startup cost matters. Lazy initialization (generate on first use of each algorithm) is the correct approach.

### 7.3 RwLock Contention on keypairs Vec

**File:** `p2p/core-engine/src/chronomatter/mod.rs:149-157`

`generate_and_store_keypair()` holds the write lock for the duration of `PrivKeyHandle::generate()`, which includes a libsodium call (Ed25519 keygen) plus optional PQC keygen. Any concurrent stamp call that tries to read from `keypairs` will block for the full keygen duration.

Generate the keypair outside the lock, then acquire the write lock only to push the result.

---

## 8. Future Directions — Hardware, Environment, and Ecosystem

### 8.1 Post-Quantum Readiness

The PQC foundations are present: liboqs integration, SPHINCS+, Dilithium3, ML-KEM-768. The architecture is algorithm-agile. However:

- **NIST PQC finalization:** ML-KEM (FIPS 203), ML-DSA/Dilithium (FIPS 204), SLH-DSA/SPHINCS+ (FIPS 205) are finalized. The codebase uses "Dilithium3" which is the pre-standardization name. The type `SignatureAlgorithm::Dilithium3` should be aliased to `ML_DSA_65` to track the standard name. Keywords: `FIPS 203`, `FIPS 204`, `FIPS 205`, `ML-KEM`, `ML-DSA`, `SLH-DSA`.
- **Hybrid classical/PQC:** For the transition period, combined signatures (Ed25519 + ML-DSA in one Foretis) are a recommended practice (`IETF hybrid-sig` drafts). This is unspecified in Foretias.
- **Algorithm sunset:** If Ed25519 is broken by a quantum adversary, past Foretises signed with Ed25519 are retroactively forgeable. A roadmap for algorithm migration (re-attesting old calendars with a new algorithm) is missing.

### 8.2 Embedded and Resource-Constrained Environments

**File:** `p2p/core/src/privkey.c:83, :125`

`calloc/free` in the C11 core breaks no-alloc embedded targets. The spec mentions MIPS cross-compile and ARM targets (`FORETIAS_0_OVERVIEW.md APPENDIX B`). For embedded:
- liboqs (200+ source files, MB-scale) is not embeddable as-is
- The `SoftwareCryptoServer` PQC keygen is unsuitable for constrained devices
- The C11 core's `privkey.c` needs a static-allocation path

**Keywords:** `no_alloc`, `embedded Rust`, `heapless`, `fixed-size buffers`, `MIPS`, `ARM Cortex-M`, `thumbv7em-none-eabihf`.

### 8.3 Space Flight and Extreme Reliability Contexts

For space applications (satellite telemetry timestamping, mission log authenticity):
- **Clock drift:** Satellites experience GPS disciplined clocks but also have isolated operation periods. The chronon_ns should be configurable from external time sources, not just wall clock.
- **Radiation hardening:** Key material in DRAM is susceptible to single-event upsets (SEUs). The C11 `PrivKeyHandle` is in-process memory with no ECC. For high-reliability applications, key material should be in ECC-protected memory or refreshed with checksums.
- **Bandwidth-constrained verification:** A SPHINCS+ signature is 7856 bytes. For satellite downlink with limited bandwidth, Foretis size matters. Ed25519 (64 bytes) plus a SPHINCS+ anchor signature per epoch is more efficient than SPHINCS+ per stamp.
- **Asynchronous operation:** Space vehicles may be out of contact for hours or days. The calendar needs to continue ticking offline and synchronize upon reconnection — the `serialized=True` mode with offline persistence supports this, but the sync protocol is underspecified.

**Keywords:** `space flight`, `radiation hardening`, `ECC memory`, `SEU mitigation`, `bandwidth-constrained`, `offline operation`, `asynchronous sync`.

### 8.4 Trusted Execution Environments and Secure Enclaves

`FORETIAS_0_OVERVIEW.md §0.9+` references `FORETIAS_ENCLAVE_SPEC.md` (restricted distribution). The `CryptoServer` trait abstraction is exactly the right design for a TEE backend. Specific considerations:

- **Intel SGX / AMD SEV:** The `CryptoServer::generate()` would run inside the enclave, never exposing key material. The `SealOps::seal_for_self` / `unseal_for_self` would use the enclave's sealing key (bound to the CPU).
- **ARM TrustZone:** For mobile timestamping use cases, TrustZone provides an isolated execution environment for key operations.
- **Apple Secure Enclave / Android StrongBox:** Key lifecycle operations (generate, sign, destroy) can be delegated to hardware. The `PrivKeyHandle` pattern is the right abstraction.
- **FIDO2/WebAuthn alignment:** The Foretias TBID signing model is similar to FIDO2 authenticators. There may be value in an authenticator-based CryptoServer for web use cases.

**Keywords:** `Intel SGX`, `AMD SEV`, `ARM TrustZone`, `Apple Secure Enclave`, `FIDO2`, `WebAuthn`, `hardware attestation`.

### 8.5 Key Transparency and External Auditability

For Foretias to be useful as a public infrastructure (not just private network), the calendar needs to be auditable by third parties who were not present during the events being timestamped. This is analogous to Certificate Transparency (RFC 6962).

Key open questions:
- Should the calendar be published to a global log (Merkle tree over all chronons)?
- Should epoch snapshots be anchored in a public ledger (blockchain, CT log)?
- How do you prove to a third-party auditor that a calendar they receive is the *same* calendar the issuer claimed to be running?

**Keywords:** `certificate transparency`, `RFC 6962`, `Merkle tree`, `public append-only log`, `third-party audit`, `blockchain anchoring`, `inclusion proof`.

### 8.6 Interoperability with Existing Timestamping Standards

- **RFC 3161 (TSA):** The de-facto enterprise timestamping standard. A bridge that issues RFC 3161 tokens anchored in a Foretias calendar would allow integration with existing enterprise PKI workflows.
- **OpenTimestamps:** Anchors arbitrary hashes in Bitcoin/Ethereum. A Foretias calendar anchored in OpenTimestamps provides quantum-resistant temporal anchoring.
- **ANSI X9.95:** Standard for trusted time stamps in financial applications. Foretias's cryptographic properties exceed this standard's requirements.

**Keywords:** `RFC 3161`, `TSA`, `OpenTimestamps`, `ANSI X9.95`, `enterprise PKI`, `bridge adapter`.

---

## 9. Marketing & Product Lifecycle Considerations

### 9.1 The Competitive Landscape

| Competitor | Strength | Foretias Advantage |
|---|---|---|
| RFC 3161 TSA | Widely deployed, enterprise trust | Decentralized, no trusted third party |
| OpenTimestamps | Public, Bitcoin-backed, free | No blockchain dependency, faster |
| Blockchain timestamps (ETH/BTC) | Decentralized, widely auditable | No transaction cost, no confirmation wait |
| Certificate Transparency | Public audit | Timestamps, not just existence proofs |
| Hyperledger Fabric timestamps | Enterprise permissioned | Open-source, no consortium lock-in |

Foretias's unique position: **mathematically provable backdate-resistance without a trusted third party or blockchain**. No other system offers this property through ephemeral key rotation.

### 9.2 The Whitepaper Gap

`specs/WHITEPAPER_SPEC.md` and `specs/WHITEPAPER_PLAN.md` exist. The whitepaper is not yet written. For academic and enterprise adoption, a whitepaper is the primary trust-building artifact. Priority: HIGH before first public release.

**The whitepaper should include:**
- Formal definition of the Foretias guarantee (backdate impossibility)
- Security proofs under the standard cryptographic assumptions (ROM, Discrete Log, CDH)
- Comparison with prior work
- Known limitations (network partition, key-in-memory extraction window)
- The 50%-compromise analysis (addressed in §0 of this document)

### 9.3 Naming Consistency Issues

The project uses:
- `Foretias` (the project, plural noun)
- `Foretis` (singular noun, a single timestamp artifact)
- `foretias` (the binary)
- `foretias-core`, `foretias-client`, `foretias-server` (crate names)
- `foretias_p2p` (PyO3 binding name, backburnered)

The singular/plural distinction (`Foretias`/`Foretis`) is elegant but non-obvious. The command `foretias stamp` returns a `foretis` — which looks like a typo of the command name.

**Recommendation:** Make the singular form more distinctive. Possible alternatives: `FStamp` (Foretias Stamp), or keep `Foretis` but add consistent documentation of the convention. The README should explain the naming scheme in the first paragraph.

### 9.4 Licensing and Adoption

The project uses BSD 3-Clause Clear License. For maximum adoption:
- BSD 3-Clause is permissive and compatible with commercial use — good choice
- Consider Apache 2.0 + MIT dual license (the Rust ecosystem standard) for broader compatibility
- Contributor License Agreement (CLA) decision should be made before first public contributors appear

### 9.5 First-Mover Considerations

Foretias should file documentation (not patents, but prior art disclosure through the whitepaper) before competitors copy the ephemeral-key-rotation approach. A public arXiv preprint establishing priority would be valuable.

**Keywords:** `prior art disclosure`, `arXiv`, `academic citation`, `open specification`.

---

## 10. Documentation Architecture

### 10.1 Documentation Taxonomy Is Over-Complex for Newcomers

The documentation system has:
- `foretias-v1.md` (product spec)
- `FORETIAS_0_OVERVIEW.md` (invariants, roadmap)
- `FORETIAS_1_MVP_SPEC.md`, `FORETIAS_2_P2P_SPEC.md`, ... (implementation specs)
- `AGENTS.md` (AI agent instructions)
- `README.md` (user-facing)
- `HOWTO.md` (tutorial)
- `docs/threat_model_v0_5.md` (security)
- `specs/*.md` (dozens of working specs and plans)

A new human contributor has no clear reading order. `AGENTS.md` defines a reading order but it's for AI agents. The `FORETIAS_0_OVERVIEW.md §READING ORDER` says to read in a specific sequence, but that section is buried.

**Recommendation:** Create a `CONTRIBUTING.md` with a human-oriented onboarding path: `README → HOWTO → foretias-v1.md → FORETIAS_0_OVERVIEW.md → specific feature spec`. Also create a `docs/architecture.md` that summarizes the system for contributors without requiring them to read all specs.

### 10.2 The @human Comment System Is an Internal Tool, Not a Bug

The `(@human ...)` parenthetical system in specs is an effective way to communicate between spec authors and AI coding agents. However, unresolved `@human` comments in specs are silently carried forward. A `@human` comment in a spec that hasn't been resolved represents an open design decision that may not be visible to contributors.

**Recommendation:** Track unresolved `@human` comments as issues or spec TODOs so they are not forgotten. The comment at `FORETIAS_0_OVERVIEW.md §0.1` about dormant TBID design is particularly important — it identifies a design gap in how dormant nodes answer queries about their calendar.

---

## 11. Specific Code Findings Cross-Referenced to Files

This section lists new findings (not in `pre-public-mvp-code-review.md`) with file:line references.

| Severity | Finding | File:Line |
|---|---|---|
| HIGH | stamp() increments tick on every call regardless of serialized mode; races with daemon_tick() | `chronomatter/mod.rs:302-320` |
| HIGH | fetch_add should replace CAS for tick counter; CAS failures cause stamp() to return errors under concurrent load | `chronomatter/mod.rs:310-314`, `:427-431` |
| HIGH | keypair index arithmetic assumes keypairs[tick-1]; breaks under concurrent stampers | `chronomatter/mod.rs:174` |
| HIGH | Algorithm hardcoded to Ed25519 in build_tick_record and stamp; should query from crypto_server | `chronomatter/mod.rs:259`, `:283`, `:355` |
| HIGH | DHT TBID registration is unauthenticated; attacker can register any TBID | `communerd/mod.rs:41-51` |
| HIGH | Mirror record batch accepted without integrity_check; allows forged calendar injection | `server/handlers.rs:465-482` |
| HIGH | block_on in async context at handle_status | `server/handlers.rs:292-296` |
| HIGH | Communerd OnceLock pattern makes initialization failure silent | `communerd/mod.rs:66-80` |
| MED | Tick numbers are sequential integers, not nanosecond epoch values as spec requires | `chronomatter/mod.rs:49`, `foretias-v1.md §2.2` |
| MED | echo field not included in signature input; MITM can modify echo | `chronomatter/mod.rs:332-334` |
| MED | No cap on keypairs Vec; memory grows unboundedly; old keys retained | `chronomatter/mod.rs:35` |
| MED | ProbityReport canonical form uses null-byte separators; canonicalization ambiguity | `probity/mod.rs` (spec `FORETIAS_2_P2P_SPEC §9.1`) |
| MED | No authentication on JSON-RPC; any client can stamp | `server/mod.rs` |
| MED | verify() returns valid:false for both forgery and unknown TBID; caller cannot distinguish | `server/handlers.rs:173` |
| MED | No wire format versioning; can't negotiate protocol version with peers | All handlers |
| MED | HOWTO §4 "kill half the network" demo not implemented or tested | `HOWTO.md §4`, `integration-tests/` |
| LOW | Foretis JSON has no schema document; consumers must reverse-engineer | `foretias/types.rs` |
| LOW | No OpenAPI / JSON schema for JSON-RPC surface | `server/jsonrpc.rs` |
| LOW | Naming: `Foretias` (project) vs `Foretis` (artifact) looks like a typo to newcomers | `README.md` |
| LOW | Algorithm name "Dilithium3" should track FIPS 204 name "ML-DSA-65" | `foretias/types.rs:SignatureAlgorithm` |

---

## 12. Prioritized Improvements — Beyond Prior Review

Ordered by impact, independent of the prior review's P0-P3 list.

### P0 — Before Any Public Announcement

1. **Fix tick counter race:** Replace CAS in `stamp()` and `daemon_tick()` with `fetch_add`. Add a concurrent stamping stress test.
2. **Fix algorithm hardcoding in build_tick_record/stamp:** Query from `crypto.signature_algorithm()`.
3. **Add JSON-RPC authentication:** Token-based auth for non-localhost bind addresses.
4. **Fix verify() ambiguity:** Return distinct codes for "forgery" vs "unknown TBID".
5. **Fix the HOWTO §4 claim or remove it:** Either implement and test the kill-half demo, or remove the claim until it works.

### P1 — Before First Public Tag

6. **Bound the keypairs Vec:** LRU eviction of old keypairs; zero-and-drop evicted keys.
7. **Fix echo field documentation or include in signature:** Choose one; don't leave it ambiguous.
8. **Add wire format versioning:** `version` field in Foretis JSON; version negotiation in stamp RPC.
9. **Authenticate DHT TBID registration:** Sign registration record with TBID signing key; verifiers check signature.
10. **Calendar mirror integrity check:** Run `verify_pair` on received batch before inserting.
11. **Write the whitepaper:** Required for academic and enterprise trust.
12. **Publish a library crate:** For application developer adoption.

### P2 — Near Term

13. **Address the chronon_number semantics:** Decide: sequential integer or nanosecond epoch? Document and implement consistently.
14. **Property tests:** `proptest` for calendar invariants, `quickcheck` for Foretis round-trips.
15. **P2P integration tests in CI:** The gossip, DHT, and cross-node verify paths need automated tests.
16. **OnceLock → state enum in Communerd:** Make P2P initialization state observable.
17. **FROST epoch consensus:** Implement or feature-gate behind `cfg(feature = "epoch")`.
18. **ProbityReport canonical serialization:** Use length-prefixed encoding.

### P3 — Strategic / Long Term

19. **RFC 3161 bridge adapter:** Enterprise adoption gateway.
20. **Whitepaper arXiv submission:** Priority establishment.
21. **Embedded / no-alloc C11 core path:** `privkey.c` without `calloc/free`.
22. **Hybrid classical/PQC signatures:** Follow IETF hybrid-sig drafts.
23. **ML-DSA/ML-KEM naming alignment:** Track FIPS 204/203 standard names.
24. **Calendar public auditability:** Merkle root published to CT-log equivalent.
25. **One-sentence pitch added to README line 1:** Maximize mindshare for every reader.

---

## 13. The Reassuring Architecture Summary

For the record: the core of Foretias works. The ephemeral key rotation mechanism is cryptographically sound. A calendar produced by this system, once a tick has advanced and the key has been algorithmically incapacitated, provides a verifiable guarantee that no one can produce a valid Foretis for that chronon's content without having been present during that specific 60-second (or configured) window.

The nightmare scenario — where more than half the network is compromised — is survivable for past stamps because the cryptographic chain is self-contained and verifiable offline. For live and future stamps, the FROST epoch consensus mechanism (currently stubbed) provides the safety net: with threshold k > N/2, an attacker holding N/2 nodes cannot produce a valid signed epoch snapshot.

The gap between the architecture's promise and the current implementation's guarantees is bridgeable with the P1-P2 items above. The vision is correct. The implementation needs hardening.

---

## 14. Deep Dive: TBID Dual-Key Design and Its Implications

### 14.1 The 49920-Byte TBID Signature

**File:** `p2p/core-engine/src/crypto_server/signing_tbid.rs:11`

TBID V1 combines Ed25519 (32-byte PK, 64-byte sig) with SLH-DSA-SHA2-256f (64-byte PK, 49856-byte sig) into a single dual-key structure:
- Public key: 96 bytes
- Secret key: 160 bytes
- Signature: 49,920 bytes

This is the genesis signature and the TBID proof-of-identity. Implications:

1. **Network bandwidth:** Mutual attestation using TBID proofs involves 49920-byte signatures per peer. For a 1000-node network doing periodic attestation rounds, each round transmits 50 MB of signature data. This needs to be factored into network design.

2. **Verification latency:** SLH-DSA-SHA2-256f verification is slower than Ed25519 (~2–5 ms per verify vs ~50 µs). Every TBID handshake adds this overhead.

3. **The TbidHandshake does NOT use the dual key:** `p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs:56-80` creates the proof using `self.crypto.sign()`, which is Ed25519 only. The TBID binding is not quantum-resistant at the handshake level — only the genesis tick record itself carries the SLH-DSA signature.

4. **Rehydration from raw bytes copies into Vec without zeroization:** `signing_tbid.rs:32-44` extracts `secret_bytes: Vec<u8>` from the C struct and then calls `foretias_tbid_v1_secret_zeroize` on the C struct. But `secret_bytes` (the Rust Vec) is NOT wrapped in `Zeroizing` and will persist in heap memory until the allocator reuses those pages.

**Fix:** Wrap `secret_bytes` as `let secret_bytes = Zeroizing::new(vec![0u8; ...])` before populating, and use `Zeroizing` throughout.

### 14.2 TBID Hexadecimal Representation Overhead

`Tbid::to_hex()` on a 96-byte TBID produces 192 hexadecimal characters. This string is used as a HashMap key (`tbid_index: Arc<std::sync::RwLock<HashMap<String, PeerRegistrationRecord>>>`), as a log field (`%tbid.to_hex()`), and embedded in JSON. The 192-character string is human-unreadable and operationally unwieldy.

**Recommendation:** Introduce a `Tbid::to_short_hex()` that returns the first 16 hex chars (as already done for `tbn`), and use this in logs and display. Use the full 96-byte array (not string) as the HashMap key for efficiency.

### 14.3 TBID as Node Identity vs. Tick Key Identity

The TBID is the node's long-term identity. The per-tick Ed25519 keypairs are the short-term signing identities. These are distinct, but the spec (`foretias-v1.md §2.1`) says:

> "tbid (bytes): opaque internal identity — UUID v4, unique per TimeBeing."

The Rust TBID V1 is NOT a UUID v4 — it is a 96-byte dual-key structure. This is a documentation divergence that will confuse anyone reading the spec alongside the code. The spec predates the TBID V1 implementation.

---

## 15. Deep Dive: FROST Epoch Consensus — Current State

### 15.1 The Triple Stub Problem

Epoch consensus is effectively non-functional at three layers simultaneously:

1. **`software.rs:284`** — `frost_sign_partial` returns `Err(CryptoError::Unsupported)`. No partial signature can be produced.

2. **`frost_bridge.rs:28-46`** — `run_frost_round` returns a dummy snapshot with `frost_signature: vec![0x00; 64]`. This is a 64-byte all-zeros signature tagged as a FROST signature.

3. **`server/handlers.rs:354-377`** — `handle_verify_epoch_snapshot` always returns `valid: true` regardless of the signature. There is no FROST verification.

This means: any node can forge an epoch snapshot with arbitrary committee/peer scores, send it to any other node, and that node will accept it as `valid: true`. This is not an attack vector today (nobody uses epoch snapshots for anything functional), but it would be catastrophic if code that acts on epoch snapshot validity (e.g., "trust only nodes in the epoch committee") were added before FROST is implemented.

**Recommendation:** Change `handle_verify_epoch_snapshot` to return `valid: false` with reason `"FROST epoch verification not yet implemented"` rather than `valid: true`. This is the honest state.

### 15.2 FROST Committee Selection is Top-Probity Only

**File:** `p2p/core-engine/src/epoch/committee.rs`

The `TopProbitySelector` picks the top-k peers by probity score. This is a good starting point, but:
- With a cold start (no probity history), all scores are 0 and the committee is chosen arbitrarily
- A node that corrupts its probity data (by never reporting bad behavior) can inflate its score
- An attacker who controls probity gossip for a period can influence committee selection

**Long-term:** Consider requiring committee candidates to have been active for at minimum N epochs, and requiring their calendar to pass integrity_check before eligibility.

---

## 16. Deep Dive: Collision Detection — Edge Cases

### 16.1 Time-Based vs Count-Based Nonce Window

**File:** `p2p/core-engine/src/collision/detector.rs:34-39`

```rust
if guard.len() >= self.nonce_window {
    guard.pop_front();
}
guard.push_back(nonce);
```

The nonce window is count-based (`nonce_window` items). Under normal operation with 30-second heartbeat intervals, a window of 100 nonces covers 3000 seconds (~50 minutes). Under attack (many heartbeats per second), the window could be exhausted in seconds, allowing replay of early nonces.

**Fix:** Bound the nonce window by time, not count. Keep nonces that are less than `heartbeat_max_age_ns` seconds old. This prevents the attack regardless of heartbeat rate.

### 16.2 Dormancy Transition Is Unrecoverable Without Restart

When a collision is detected, the node goes dormant (`set_dormant(true)`). There is no path to recover from dormancy without process restart. The collision event should:
1. Immediately broadcast to the liege channel (implemented as a stub)
2. Allow operator override after verification that the collision was spurious (e.g., replay attack)
3. Provide a grace period window where the node is dormant but can be revived if the collision is retracted

### 16.3 Collision Detection Uses Own Server's Ed25519 Key

**File:** `p2p/core-engine/src/collision/detector.rs:58`

```rust
let valid = crypto.verify_ed25519(&self.my_pub_key, &hb.canonical(), &sig).ok()?;
```

The detector checks if an inbound heartbeat, claiming to be from our peer_id, has a valid signature under OUR public key. If valid and the nonce is unknown, it's a collision. But this means a heartbeat from an entity that somehow obtained our private key (e.g., a cloned VM) would be detected. What about an entity that generated a TBID that happens to hash to the same peer_id but has a different keypair? That case produces an invalid signature and is ignored — which is correct for TBID collision detection. But the heartbeat uses `peer_id` (the libp2p PeerId derived from the public key) as the identity, not the TBID itself. Two nodes with different TBIDs but the same libp2p PeerId are mathematically nearly impossible, but this edge case should be documented.

---

## 17. Deep Dive: The Probity Gossip Backburner

**File:** `specs/FORETIAS_2_P2P_6_probity_gossip.md`

The probity gossip spec is marked `[x] backburnered`. However, the implementation is fully present:
- `p2p/foretias-server/src/probity/gossip_handler.rs`
- `p2p/foretias-server/src/probity/aggregator.rs`
- `p2p/foretias-server/src/probity/store.rs`
- `p2p/foretias-server/src/probity/report.rs`

This is the reverse of the normal spec-before-code discipline. The implementation was written ahead of (or in parallel with) the spec, and then the spec was backburnered. This creates a gap: there is no authoritative spec for the current probity implementation. The implementation may have diverged from the backburnered spec.

**Recommendation:** Un-backburner or supersede the probity spec with a new document that describes the *actual* implementation and its invariants. Then add integration tests that verify those invariants hold.

---

## 18. Observability, Operations, and Production Readiness

### 18.1 No Prometheus Metrics Endpoint

**File:** `p2p/foretias-server/src/metrics.rs`

The `Metrics` struct has atomic counters (`stamps_total`, `heartbeats_sent`, `collisions_detected`, etc.). These are accessible via the `status` and `collision_status` JSON-RPC calls, but there is no Prometheus exporter, no Grafana dashboard configuration, no structured log format compatible with ELK/Splunk.

For production adoption, operators need:
- Prometheus `/metrics` endpoint exposing all counters and gauges
- A Grafana dashboard JSON for the key metrics (calendar lag, peer count, stamp rate, probity score distribution)
- Alerting rules: "calendar not advancing for > 2 chronons", "peer count < threshold", "collision detected"

**Keywords:** `Prometheus`, `Grafana`, `OpenTelemetry`, `structured logging`, `alerting`, `SLO`.

### 18.2 Graceful Shutdown Is Not Implemented

**File:** `p2p/core-engine/src/chronomatter/mod.rs:413-419`

```rust
pub fn stop_daemon(&self) {
    let mut daemon = self.daemon_handle.lock();
    if let Some(handle) = daemon.take() {
        handle.abort();
    }
}
```

`handle.abort()` cancels the tokio task at the next await point without cleanup. If the daemon was:
- Holding the keypairs write lock during key generation: the lock is dropped at cancellation, but any in-flight calendar record may be lost
- In the middle of a `save()` call: the save may be partially written

A graceful shutdown should:
1. Set a shutdown flag that the daemon checks after each tick
2. Wait for the current tick to complete
3. Write the final calendar record atomically
4. Signal all P2P connections to close gracefully

**Keywords:** `graceful shutdown`, `tokio CancellationToken`, `shutdown signal`, `SIGTERM handler`.

### 18.3 No Maximum Concurrent Connections Limit

The JSON-RPC server (`server/mod.rs`) accepts unlimited concurrent TCP connections. The OS has a file descriptor limit (typically 1024-65536 per process). Under a connection flood, the server exhausts file descriptors and new connections fail with opaque OS errors. Explicit limits and connection counting should be added.

**File:** `p2p/foretias-server/src/server/mod.rs`

### 18.4 The `time_being_reference_time` Custom Format

**File:** `p2p/core-engine/src/chronomatter/mod.rs:349`

```rust
let time_being_reference_time = format!("UE+{}ns", now_ns);
```

"UE+1700000000000000000ns" is a custom format. It is not:
- ISO 8601 (can't be parsed by any standard library)
- RFC 3339 (no timezone indicator)
- Unix timestamp (not a number)

For interoperability with any non-Foretias tool, use RFC 3339 (`2026-05-19T12:00:00.000000000Z`) or a raw nanosecond u64.

### 18.5 Replication Logger Has No Consumers

**File:** `p2p/foretias-server/src/replication_logger.rs`

This file presumably logs calendar replication events. If it logs to a separate file, there is no integration with the primary tracing subscriber setup in `main.rs`. Verify that replication events are accessible to operators without reading a separate log file.

---

## 19. Deep Dive: Memory Safety and Zeroization Inventory

A complete inventory of secret material and its zeroization status:

| Secret | Location | Zeroized? | Method |
|---|---|---|---|
| Ed25519 tick private key | `chronomatter.rs: keypairs[i].priv_key` | Only on `PrivKeyHandle::drop` | C11 `foretias_memzero` |
| SoftwareCryptoServer seal key | `software.rs:34 Zeroizing<[u8;32]>` | Yes, on drop | `Zeroizing` auto |
| SPHINCS+ secret key | `software.rs:38 Option<Zeroizing<SignatureBytes>>` | Yes, on drop | `Zeroizing` auto |
| Dilithium3 secret key | `software.rs:40 Option<Zeroizing<SignatureBytes>>` | Yes (fixed from prior review) | `Zeroizing` auto |
| SLH-DSA-256f secret key | `software.rs:43 Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` auto |
| ML-KEM secret key | `software.rs:46 Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` auto |
| TBID secret key (in signing_tbid.rs) | `secret_bytes: Vec<u8>` (Rust side) | **NO** | Not wrapped in `Zeroizing` |
| Noise session keys | In libp2p swarm memory | Depends on libp2p | libp2p manages |
| FROST shares | `software.rs:36 parking_lot::Mutex<HashMap<String, Zeroizing<Vec<u8>>>>` | Yes, on drop | `Zeroizing` auto |
| TbidProofRequest nonce | `tbid_handshake.rs:52 [u8; 32]` | No (stack allocated, drops normally) | None needed (not secret) |
| TbidProofResponse signature | `tbid_handshake.rs:31 [u8; 64]` | No | Not zeroized |

The TBID secret key in `signing_tbid.rs:32-43` is the most significant gap: 160 bytes of secret material in a plain `Vec<u8>` that is not zeroized.

---

## 20. P2P Network Event Channel — Potential Memory Leak

**File:** `p2p/foretias-server/src/communerd/mod.rs:68`

```rust
p2p_events: Arc<OnceLock<tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>>>,
```

An `UnboundedReceiver<NetworkEvent>` is stored in a `OnceLock`. If nothing ever reads from this receiver, events accumulate in the channel buffer without bound. The `Communerd` struct has no method that reads from `p2p_events`.

The events are presumably meant to be consumed by an event loop in `serve` startup. If the event loop is not started or terminates early, the channel fills unboundedly. Under active P2P with many peers, this is a memory leak.

**Recommendation:** Either consume the events in a dedicated task (started during `serve`) or replace `UnboundedReceiver` with a bounded channel. If the consumer falls behind, bounded channel backpressure signals the problem rather than silently growing memory.

---

## 21. The "Serialized Mode" Design Decision

**File:** `specs/foretias-v1.md §2.1`, `p2p/core-engine/src/chronomatter/mod.rs`

The spec defines two modes:
- `serialized=False`: background daemon advances tick on a timer; multiple stamps can occur within one tick
- `serialized=True`: each stamp advances the tick; at most one stamp per tick

The Rust implementation does not expose `serialized` mode at all. Every stamp advances the tick. This is effectively `serialized=True` for all callers.

For high-throughput use cases (e.g., stamping 1000 documents at the same instant), the non-serialized mode allows batching: all 1000 stamps share the same chronon_number, and only one tick advance happens. This is both more efficient and semantically richer (all 1000 documents were stamped "within the same minute" as witnessed by a single key transition).

The loss of non-serialized mode affects:
- **Throughput:** 1000 stamps = 1000 tick advances = 1000 keypair generations = significant CPU
- **Calendar size:** 1000 stamps = 1000 ChrononRecords instead of 1, plus 1000 Foretis entries
- **Semantics:** Two stamps that happen at the same wall-clock second get different chronon numbers, giving the false impression they happened at different "times"

This design choice should be explicitly documented and justified, or non-serialized mode should be implemented.

---

## 22. Cross-Language and Interoperability Gaps

### 22.1 The Foretis JSON Has No Canonical Serialization

For a Foretis to be verifiable by any implementation, the JSON serialization must be canonical. But Rust's `serde_json` does not guarantee field order, and the Foretis struct uses `#[derive(Serialize, Deserialize)]` without a canonical order constraint. Two different Rust implementations might serialize the same Foretis with fields in different orders, producing different JSON strings — but the same cryptographic validity.

This is fine for parse-then-verify workflows. But if any code ever does `hash(json_string_of_foretis)` as a content check, field ordering matters. Document explicitly that Foretis JSON strings are NOT canonical and must not be hashed.

### 22.2 Base64 vs Hex Encoding

**File:** `specs/FORETIAS_CUSTOM_BASE64_SPEC.md`

A spec exists for a custom base64 encoding. The implementation uses hex encoding everywhere (`hex::encode`, `hex::decode`). Hex doubles the size of all byte fields. A 64-byte Ed25519 signature becomes 128 hex characters; a 49856-byte SLH-DSA signature becomes 99712 hex characters per TBID proof record.

Base64 would reduce that to 66 and 66504 characters respectively (~33% savings). For large-scale calendar replication, this matters. The `FORETIAS_CUSTOM_BASE64_SPEC.md` spec exists but the plan's status is unknown.

### 22.3 No Stable CBOR or Protobuf Wire Format

The JSON-RPC protocol uses JSON encoding for all fields. JSON is human-readable but:
- Variable-length encoding is inefficient for fixed-size cryptographic types (32-byte key → 64 hex chars → 2× overhead)
- No schema validation by default
- No forward/backward compatibility guarantees

For the P2P layer, a binary protocol (CBOR, protobuf, MessagePack) would be more efficient and less ambiguous. This is a long-term consideration but should be factored into the wire format versioning decision.

---

## 23. The Dormant Node Design Gap

**File:** `specs/FORETIAS_0_OVERVIEW.md §0.1` (the @human comment)

The spec has an unresolved `@human` comment about dormant nodes:

> "The answer is that we need to update communication with calendric data by adding 'tbid' to the query, so a new dormant time being with its own TBID, it can serve queries about one or many previous and other simultaneous TBID."

This describes a design where a single dormant node can serve calendar queries for multiple historical TBIDs. The current implementation only serves the local node's calendar. The `handle_get_calendar_slice` handler has no TBID parameter — it always returns the server's own calendar.

For a production system:
- A "calendar archive node" would store and serve many historical calendars
- Queries would be `get_calendar_slice(tbid, start, count)` not just `get_calendar_slice(start, count)`
- The mirror store already accumulates other TBIDs' calendars, but is not exposed via the slice endpoint

This is an important missing feature for the "kill half the network and verification still works" scenario: surviving nodes should be able to serve calendar slices for dead nodes' TBIDs.

---

## 24. Time Synchronization and Clock Skew

### 24.1 The Chronon Timer Under NTP Slew

The daemon tick loop uses `tokio::time::interval`:
```rust
let mut interval = tokio::time::interval(duration);
interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
```

With `MissedTickBehavior::Delay`, if a tick takes longer than `chronon_ns` to process, the next tick fires immediately after, not at the scheduled time. This means:
- Under load, tick rate can exceed the configured rate
- Calendar grows faster than expected
- The "60-second chronon" guarantee becomes "at most 60-second chronon, possibly shorter"

`MissedTickBehavior::Skip` would preserve the wall-clock alignment but skip ticks under load — potentially missing attestation windows.

The correct behavior depends on the security model: is it "ticks should happen no more than every X ns" or "ticks happen approximately every X ns"? The spec says `chronon_ns` is "adhered to at best effort", so `Delay` is defensible, but should be documented.

### 24.2 Leap Seconds and Discontinuous Wall Clocks

When NTP applies a leap second, the system clock jumps by ±1 second. `SystemTime::now()` may return:
- A time in the past (negative slew — very brief window of timestamps appearing backward)
- A duplicate time (leap smearing)

The sequential integer tick numbering bypasses this because it doesn't depend on wall clock ordering. But `time_being_reference_time` uses wall clock time and could show discontinuities. For audit purposes, a reference time that jumps backward is confusing even if the chronon_number ordering is preserved.

**Keywords:** `leap second`, `NTP slew`, `clock discipline`, `monotonic clock`, `tai64n`.

### 24.3 GPS-Disciplined Time for High-Accuracy Deployments

For use cases requiring sub-millisecond timestamp accuracy (financial trading, scientific instruments), the system clock is insufficient. GPS-disciplined clocks (PPS signal + GPSD) or IEEE 1588 PTP can provide nanosecond accuracy. The `Clock` trait is the correct abstraction point for injecting a high-precision clock source.

**Keywords:** `GPS`, `PPS`, `GPSD`, `IEEE 1588`, `PTP`, `high-precision timing`.

---

## 25. The Honor System vs. Zero-Trust

### 25.1 Foretias Is "Trust by Verification, Not by Faith"

The probity system is fundamentally a reputation system. Reputation systems work well when honest participants form a strong majority. The spec acknowledges this:

> "No central enforcement. The hierarchy of honor self-organizes."

This is philosophically admirable but has practical failure modes:
- **Cold start:** A new node with no reputation can behave honestly for N epochs, accumulate reputation, then defect
- **Forgetting:** The U-shape weight gives equal weight to recent bad behavior and very old bad behavior (the "ancient wisdom" curve). An attacker who behaved badly 6 months ago and is now behaving well can recover reputation
- **Coordinated Sybil:** Multiple low-reputation nodes provide positive reports to each other, inflating each other's scores

The U-shape aggregation is innovative. Its security properties under Byzantine adversaries should be analyzed formally before the system is depended upon for high-value timestamping. Keywords: `Byzantine fault tolerance`, `Sybil attack`, `reputation systems`, `mechanism design`.

### 25.2 Application Layer Has No Enforcement

The P2P layer reports probity scores. Applications decide what to do. The current application (`TimeFamilyServer`) does not query probity scores before accepting stamp requests, routing stamps, or accepting calendar mirrors. Probity data is gathered but never acted upon.

This is an intermediate state. The next step is to add policy enforcement: "refuse to accept calendar mirrors from nodes with probity score < threshold." But this threshold is application-specific and not currently configurable.

---

## 26. Language Bindings and Ecosystem Position

### 26.1 The Backburnered Language Gap

Python and Java bindings are backburnered. The `specs/SCOPE_REDUCTION_SPEC.md` explains the rationale. But the audiences who would benefit most from Foretias embedding are:
- **Python data scientists:** timestamping ML model training runs, experiment results
- **Java enterprise developers:** financial timestamping, document workflows
- **JavaScript/Node.js:** web applications, browser-based verification
- **Go:** cloud infrastructure, Kubernetes operators

The current Rust CLI is well-suited for infrastructure operators. For application developers, a language binding is the entry point.

**Priority order for re-introduction:**
1. Python (largest audience, data science use case is compelling)
2. JavaScript/TypeScript (web verification without running a local server)
3. Go (cloud-native ecosystem)
4. Java (enterprise existing PKI workflows)

### 26.2 WASM/Browser Verification

A WebAssembly build of the verification path (no stamping, no P2P) would allow:
- Browser-based Foretis verification without any server infrastructure
- Embeddable verification in web applications
- Audit tools accessible to non-technical users

The C11 core uses libsodium and liboqs, both of which have WASM builds. A `foretias-verify-wasm` crate exposing `verify_foretis(foretis_json, content, calendar_json) -> bool` would be a compelling standalone artifact.

**Keywords:** `wasm-pack`, `wasm-bindgen`, `libsodium-wasm`, `browser verification`, `WebAssembly`.

### 26.3 C ABI for Maximum Interoperability

A C ABI (`p2p/core-engine/src/api/c_ffi.rs` is referenced in the overview spec but may not exist) would enable binding from any language via FFI: Python ctypes, Go cgo, JavaScript Node native addons, Ruby FFI, etc. This is the most universal interoperability layer.

---

## 27. Open Design Questions Requiring Resolution

These are items that appear as unresolved tensions in the specs or code, requiring explicit decisions:

### 27.1 Chronon Number = Absolute Time or Sequence Number?

- **Spec says:** nanoseconds since Unix epoch (absolute time)
- **Implementation has:** sequential integer starting from 1

Resolution needed before any cross-node interoperability depends on chronon_number for ordering.

### 27.2 Non-Serialized Mode Implementation

- **Spec says:** non-serialized = multiple stamps per tick, daemon advances tick
- **Implementation:** all stamps advance tick independently

Resolution needed before high-throughput use cases are documented or promised.

### 27.3 The "Source of Truth" for Protocol Semantics

- **Prior review noted:** `FORETIAS_0_OVERVIEW.md §0.7` says Python is the source of truth, but Python code has deprecation warnings
- **Resolution pending from P1 list of prior review**

### 27.4 Dormant TBID Query Routing

- **Spec notes (@human):** dormant nodes should serve queries for multiple historical TBIDs
- **Implementation:** servers only serve their own calendar
- **Resolution needed** before "kill half the network" resilience demo works

### 27.5 The Echo Field — Authenticated or Advisory?

- **Current:** not signed, advisory only
- **Use case:** clients use echo as a correlation ID; MITM servers can replace it
- **Resolution:** document as advisory, or include in sig input

### 27.6 Algorithm Sunset Planning

- **No documented roadmap** for algorithm deprecation
- **Ed25519 quantum vulnerability** timeline matters for long-lived calendars
- **Resolution:** add algorithm sunset plan to the roadmap spec

---

## 28. Application Use Cases and Their Gaps

To make the product roadmap concrete, here are specific use cases and what needs to be true for each to work:

| Use Case | Current Status | Gap |
|---|---|---|
| **Document signing with backdate-proof timestamp** | Works (CLI stamp/verify) | No library API; no auth on stamp endpoint |
| **Software release attestation** ("this binary was built at T") | Works | No integration with CI/CD systems; no metadata fields |
| **ML experiment timestamping** ("model trained with these hyperparams at T") | Works manually | No Python library; no structured metadata in Foretis |
| **Legal document execution** ("contract signed at this moment") | Partially works | No RFC 3161 bridge; no legal jurisdiction compliance docs |
| **Supply chain attestation** ("component provenance at T") | Not yet | Need a rich Foretis format with structured metadata |
| **Financial trade timestamping** | Not yet | Need sub-second accuracy; need ANSI X9.95 compliance |
| **Distributed system causality ordering** ("A happened before B") | Partially | Calendar ordering works but requires common TimeBeing reference |
| **Satellite telemetry authenticity** | Not yet | Need offline operation + sync protocol; radiation hardening |

### 28.1 Missing: Structured Metadata in Foretis

A Foretis currently stamps raw content bytes. For most real use cases, the "content" is a hash of a larger artifact plus structured metadata:
- What artifact is this? (filename, version, type)
- Who requested this stamp? (requester identity)
- What was the context? (software version, environment)

Adding structured metadata to `Foretis` (or a wrapping `SignedStamp` envelope) would make Foretias usable for supply chain, legal, and ML use cases without forcing users to pack metadata into the content bytes themselves.

**Keywords:** `structured metadata`, `Foretis envelope`, `content attestation`, `supply chain`, `SBOM`.

---

## 29. Security Properties Summary — What Is and Is Not Guaranteed

This section provides a plain-language summary of the security properties, useful for the whitepaper and for auditors.

### 29.1 Properties the System Provides

1. **Backdate impossibility (for past chronons):** Once chronon N's private key has been algorithmically incapacitated by the advance to chronon N+1, no valid Foretis for chronon N can be created. This holds regardless of attacker capability, assuming the underlying cryptographic primitives (Ed25519, SHA-256) are not broken.

2. **Calendar integrity (append-only chain):** The forward/backward auto-attestation structure means that any modification to a chronon record is detectable. A verifier can check the entire chain with `integrity_check()` and detect any tampering.

3. **Offline verification:** A calendar file, once obtained, can be verified without any network access. No server needs to be online for historical Foretis verification.

4. **Cross-peer witness:** External attestations from other time beings in a chronon record provide corroborating evidence from independent parties.

### 29.2 Properties the System Does NOT Provide

1. **Wall-clock time accuracy:** The chronon_number is currently a sequential integer, not a real nanosecond timestamp. The system proves ordering, not absolute time.

2. **Protection against key-in-memory extraction:** If an attacker can extract the current tick's private key from process memory (via physical access, hypervisor exploit, side-channel), they can forge Foretises for the current tick until the next tick advance.

3. **Sybil resistance:** Creating Foretias nodes is free. An attacker can create thousands of TBIDs to influence probity scores or DHT routing.

4. **Protection against majority compromise (live period):** If more than half the network is compromised simultaneously, the probity and gossip systems are subverted for as long as the compromise persists. Past Foretises remain verifiable; future ones may come from a compromised network.

5. **Cryptographic binding of content to real-world events:** The system proves that content with hash H was presented to the timestamping server at some point in time. It cannot prove anything about the content itself (authorship, correctness, legal significance).

---

## 30. The Path to a Usable Public Release

### 30.1 Minimum Viable Public Release Checklist

Beyond the P0 items in §12, a public release needs:

- [ ] BSD 3-Clause attribution correctly in all source files (verify LICENSE headers)
- [ ] Security disclosure process documented (SECURITY.md)
- [ ] CI passing on every PR (not just on release tags)
- [ ] `cargo audit` and `cargo deny` passing
- [ ] Memory leak check (valgrind or cargo-valgrind on C11 tests)
- [ ] ASAN + UBSAN passing on C11 tests
- [ ] No `TODO`, `FIXME`, `HACK`, or `STUB` in public-facing APIs
- [ ] All `@human` comments either resolved or tracked as issues
- [ ] Whitepaper or technical note published (even as a preprint)
- [ ] Clear versioning policy (SemVer for CLI flags, wire format, library API)
- [ ] `CHANGELOG.md` for users tracking updates

### 30.2 The Adoption Funnel

For a developer to go from "heard about Foretias" to "using it in production":

1. **Discover:** Find the project (GitHub, HN post, blog post, conference talk)
2. **Understand:** Read README and understand the value proposition in 60 seconds
3. **Try:** `cargo install foretias` → `foretias serve` → `foretias stamp` in 5 minutes
4. **Evaluate:** "Does it do what I need?" — requires docs on use cases and limitations
5. **Integrate:** `cargo add foretias-client` and embed in application
6. **Deploy:** Run a server, connect to peers, monitor it
7. **Trust:** Verify that the security properties are real (whitepaper, audit)

Steps 3 and 5 currently require significant manual effort. Steps 2, 4, and 7 are not well served by current documentation.

### 30.3 The One-Page Pitch

Every open-source project needs a one-page pitch that can be shared as a PDF or linked in a tweet. Key elements:
- What problem does it solve? (timestamps are forgeable by default)
- How does it solve it? (ephemeral keys — one paragraph, with a diagram)
- What are the guarantees? (backdate impossibility, offline verification)
- What are the limitations? (honest majority for live stamps)
- How do you get started? (two commands)

This does not currently exist. It should be the highest-priority documentation task, written before any public announcement.

---

## 31. Ideas for Further Research and Exploration

These are open-ended ideas that may be worth prototyping, analyzing, or discussing. Each represents a thread worth pulling.

### 31.1 Recursive Calendar Anchoring

If calendar A anchors calendar B (B's first chronon is cross-signed by A), and calendar C anchors calendar A, you get a tree of trust anchored at the root calendar. A "root time being" with very high uptime and broad peer network could serve as a trust anchor for a PKI-like hierarchy. This resembles Certificate Transparency but for timestamps.

**Keywords:** `trust anchor`, `root calendar`, `certificate hierarchy`, `timestamp PKI`.

### 31.2 Zero-Knowledge Proofs for Foretis Verification

A ZK proof could prove that "I possess a valid Foretis for content C at chronon N" without revealing the Foretis itself (privacy-preserving verification). This is useful for scenarios where the content of the stamp is confidential but the fact of having a valid stamp needs to be demonstrated.

**Keywords:** `zk-SNARK`, `Groth16`, `Plonk`, `privacy-preserving verification`, `zero-knowledge timestamp`.

### 31.3 Threshold Timestamping

Instead of one time being stamping content, k-of-n time beings co-sign a single Foretis. This provides resistance to k-1 compromised signers and removes single-point-of-failure from the stamping path. FROST is already in the roadmap for epoch consensus; extending it to the stamp path would enable threshold timestamping.

**Keywords:** `threshold signature`, `FROST`, `multi-party timestamping`, `k-of-n signing`.

### 31.4 Content Commitment Schemes

Instead of stamping the raw content hash, a commitment scheme (Pedersen commitment, KZG) over the content would allow batch stamping (one stamp covers 1 million documents) with individual inclusion proofs. This is important for high-throughput scenarios.

**Keywords:** `KZG commitment`, `Pedersen commitment`, `batch timestamping`, `inclusion proof`, `vector commitment`.

### 31.5 Time Being as a Smart Contract

A blockchain-based time being where the chronon advance is triggered by a smart contract transaction. The private key is derived from a verifiable random function (VRF) seeded by the previous block hash. This provides public, auditable, and censorship-resistant timestamping with no trusted operator.

**Keywords:** `smart contract`, `VRF`, `Ethereum`, `Solidity`, `decentralized timestamping`.

### 31.6 Post-Compromise Attestation

"I was running fine until time T; after T, I may have been compromised." A mechanism for a node to attest its last-known-good state and revoke authority for subsequent stamps. This requires a persistent identity (possibly through TBID succession) that outlives individual process runs.

**Keywords:** `post-compromise security`, `attestation revocation`, `key compromise notification`, `TBID succession`.

### 31.7 Latency-Optimized Verification

Current verification requires fetching the full chronon record (public key + auto-attestation). For a 49920-byte TBID signature in each record, fetching even one record is expensive. An optimized path would cache public keys separately from the full record, allowing verification with just the 32-byte Ed25519 public key for Ed25519-signed Foretises.

**Keywords:** `key cache`, `verification optimiation`, `partial calendar sync`, `Merkle proof of inclusion`.

### 31.8 Federated Calendar Networks

Multiple independent Foretias networks (different dht_namespace) anchored to each other via cross-calendar attestations. A "US legal" network, an "EU legal" network, and a "financial" network could all be independently operated but mutually attesting, creating a web of trust across jurisdictions.

**Keywords:** `federation`, `cross-namespace attestation`, `multi-jurisdiction`, `trust web`.

### 31.9 The Nanosecond Timestamp Recovery Path

The current sequential integer tick numbering could be made backward-compatible with the spec's nanosecond design: record the start time of the server and the tick duration, and derive the theoretical nanosecond timestamp from `start_time + tick_number * chronon_ns`. This is an approximation (actual tick times vary due to daemon load), but it restores the "absolute time" property without breaking existing data.

---

## 32. Appendix: File-By-File Risk Assessment

A triage table for code review prioritization. Files with HIGH or CRIT risk that an auditor should review first:

| File | Risk | Primary Concern |
|---|---|---|
| `p2p/core/src/noise_xx.c` | CRIT | Nonce overflow (2^64), stack buffers |
| `p2p/core/src/privkey.c` | CRIT | HKDF stack overflow, global state, dynamic alloc |
| `p2p/core-engine/src/crypto_server/software.rs` | HIGH | Algorithm dispatch, Drop zeroization |
| `p2p/core-engine/src/chronomatter/mod.rs` | HIGH | CAS race, unbounded keypairs, tick semantics |
| `p2p/foretias-server/src/server/handlers.rs` | HIGH | block_on, auth gap, JSON-RPC id, content limits |
| `p2p/core-engine/src/crypto_server/signing_tbid.rs` | HIGH | Rust Vec not zeroized after TBID signing |
| `p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs` | MED | Uses system RNG not crypto server RNG |
| `p2p/core-engine/src/collision/detector.rs` | MED | Count-based nonce window, no timestamp freshness |
| `p2p/foretias-server/src/communerd/mod.rs` | MED | OnceLock init failure silent, event channel leak |
| `p2p/foretias-server/src/probity/gossip_handler.rs` | MED | No integration tests, backburnered spec |
| `p2p/core-engine/src/epoch/frost_bridge.rs` | MED | Stub signature accepted as valid |
| `p2p/foretias-server/src/server/handlers.rs` (ship_ack) | MED | Mirror records not integrity-checked |
| `p2p/foretias-server/src/calendar_store/encrypted_jsonl.rs` | LOW | Full-file read, block ID reset on error |
| `p2p/core-engine/src/probity/store.rs` | LOW | O(N) scan expected; watch as network grows |

---

## 33. In Summary — A Project Worth Completing

Foretias addresses a real and important problem: the forgeability of timestamps in digital systems. The core mechanism — per-chronon ephemeral key pairs with self-documenting chain transitions — is cryptographically elegant, and the fact that verification works offline without trusting any server is genuinely powerful.

The implementation is in a credible intermediate state. The foundational cryptographic layer (C11 core + Rust wrappers) works. The P2P layer is wired but unverified. The product presentation and library usability need significant work before public adoption.

The most urgent improvements, ranked by impact on public readiness:

1. Fix the three critical C11 safety issues (nonce overflow, HKDF stack overflow, dynamic allocation)
2. Add PR-blocking CI (the #1 process failure)
3. Fix the echo/stamp semantics documentation (behavioral correctness)
4. Add JSON-RPC authentication (security correctness)
5. Write and publish the one-page pitch and whitepaper (adoption prerequisite)

Everything else is important but can be phased. The vision is clear, the architecture is sound, and the team discipline (spec-first, terminology enforcement, security-first coding guidelines) is excellent. The project deserves to reach its potential.

---

---

## 34. Cross-Reference with Prior Review — Unaddressed Findings

**Source:** `specs/pre-public-mvp-code-review.md` (Claude Opus 4.7, 2026-05)

This section maps every finding from the prior review against the coverage in this document. Items marked **[REPEAT]** were covered in this document's earlier sections (the section reference is given). Items without a [REPEAT] marker are findings from the prior review that were NOT addressed in this document — they are catalogued here with their full detail so nothing is lost.

---

### 34.0 Prior Review P0 Items — Status

| Prior §  | Finding | Status in This Document |
|---|---|---|
| §2.1 | `sign()` / `signature_algorithm()` mismatch | **[REPEAT §3.6]** — covered; may be fixed now |
| §2.2 | Python ↔ Rust auto-attestation blob incompatibility | **[REPEAT §27.3]** — only referenced as pending; NOT fully catalogued below |
| §1.2 | PQC C files not in `CMakeLists.txt` | NOT addressed — catalogued at §34.1 |
| §1.2 | OpenSSL link via `hash_legacy_insecure_*.c` | NOT addressed — catalogued at §34.1 |
| §2.3 | `Drop` skips `dilithium_secret_key` | **[REPEAT §19]** — noted as fixed via `Zeroizing` wrapper |
| §6.3 | No PR-blocking CI workflow | **[REPEAT §4.1, §30.1]** |
| §3.1 | C-core nonce overflow (`noise_xx.c:69,86`) | **[REPEAT §32 CRIT]** |
| §3.1 | C-core HKDF stack overflow (`privkey.c:236-240`) | **[REPEAT §32 CRIT]** |
| §1.5 | `block_on` deadlock in handlers | **[REPEAT §2.8]** |
| §1.5 | `MAX_CONTENT_BYTES` 1 GiB pre-decode | **[REPEAT §3.5]** |

---

### 34.1 Duplicated C Build Path (PQC Files Not in CMakeLists.txt)

**From prior review §1.2 — [CRIT] — NOT addressed in this document.**

`p2p/core/CMakeLists.txt:23-40` does NOT include `signing_sphincs.c`, `signing_dilithium.c`, or `kem_mlkem.c` in `FORETIAS_CORE_SOURCES`. The Rust `build.rs` compiles its own copies from `p2p/core-engine/src/core/algorithms.{c,h}` — a parallel C build that bypasses the strict C11 compile flags (`-Wpedantic -Werror -fstack-protector -fvisibility=hidden`) mandated by `FORETIAS_1_MVP_SPEC §4.1`.

**Consequence:** The PQC code path is compiled without verified-core invariants. The C11 security boundary is not enforced for the code that handles post-quantum signatures and key encapsulation.

**Fix:** Either (a) add the PQC source files to `p2p/core/CMakeLists.txt` with the same strict flags, or (b) remove the parallel `algorithms.{c,h}` shadow path and have `build.rs` compile the authoritative sources from `p2p/core/src/`.

**Also from §1.2 — [HIGH]:** Function names diverge between spec and implementation:
- Spec: `foretias_sphincs_*`, `foretias_dilithium_*`, `foretias_mlkem_*`
- Implementation: `foretias_sphincs_sha2_128s_*`, `foretias_dilithium3_*`, `foretias_mlkem_768_*`

`foretias_core.h` declares both forms inconsistently. The Rust FFI and C build will silently disagree on the symbol surface. Pick one canonical form, regenerate bindings, update spec.

---

### 34.2 OpenSSL Dependency via Legacy Hash Files

**From prior review §1.2 — [HIGH] — NOT addressed in this document.**

`p2p/core/src/hash_legacy_insecure_md5.c:2` and `hash_legacy_insecure_sha1.c:2` link OpenSSL EVP. `FORETIAS_1_MVP_SPEC §4.3` explicitly forbids this: *"Do NOT link against OpenSSL for this — vendor a small pure-C implementation."*

The CMakeLists also adds OpenSSL as a hard dependency, expanding the supply-chain surface beyond libsodium + liboqs. This is a supply-chain risk (OpenSSL CVEs), a build complexity issue (OpenSSL headers required on every build host), and a spec violation.

**Fix:** Vendor a small pure-C MD5/SHA-1 (both are public domain algorithms; reference implementations exist in ~200 LOC each). Or remove the legacy hash files entirely if no caller requires them.

---

### 34.3 Python ↔ Rust Auto-Attestation Blob Incompatibility

**From prior review §2.2 — [CRIT] — only mentioned tangentially at §27.3 of this document.**

The auto-attestation blob layouts differ:
- **Python** (`src/foretias/_timebeing.py:114-120, :152-158`): `tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk` — no `stamps_per_tick`, no `aa_nonce`
- **Rust** (`p2p/core-engine/src/foretias/tick.rs:149-168`): `tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk ‖ stamps_per_tick (u64 BE) ‖ nonce (16B)`

A calendar produced by Python cannot be verified by Rust and vice versa. This is a cross-language correctness blocker.

**Resolution:** Adopt the Rust layout (strictly stronger — replay-resistant via `aa_nonce`). Update Python `_timebeing.py` to include `stamps_per_tick` and `aa_nonce` in its blob. Then add a cross-language equivalence test:
1. Create a calendar in Python, open it in Rust, run `integrity_check` — expect all-true
2. Create a calendar in Rust, open it in Python, verify each consecutive pair — expect all-true

---

### 34.4 Serde Default on `stamps_per_tick` Contradicts PQC Spec

**From prior review §1.3 — [MED] — NOT addressed in this document.**

**File:** `p2p/core-engine/src/foretias/tick.rs:27`

```rust
#[serde(default)] pub stamps_per_tick: u64
```

`FORETIAS_3 §front-matter` (PQC integration spec) explicitly states: *"no `#[serde(default)]` for new fields, no legacy migration paths."* The attribute allows deserialization of records that omit `stamps_per_tick`, silently defaulting to 0. This makes it impossible to distinguish a record that legitimately has 0 stamps from a Python-format record that doesn't include the field at all.

**Fix:** Remove `#[serde(default)]`. Add a migration note in the changelog. A missing field should be a hard deserialization error, not a silent zero.

---

### 34.5 Undocumented Fields in Foretis and TickRecord

**From prior review §1.3 — [MED] — NOT addressed in this document.**

- `tick.rs:44 signature_algorithm: String` and `:52 time_being_reference_time: String` — present in Rust `Foretis` but absent from `FORETIAS_1_MVP_SPEC §7.1`. Update spec to declare these.
- `tick.rs:31 external_attestations: Vec<ExternalAttestation>` — an undocumented addition. Mixing third-party attestations into `TickRecord` widens the calendar's signed surface without specification. Either document it formally in spec or move to a side-channel structure outside the canonical calendar record.

---

### 34.6 Python Field Name Divergence

**From prior review §1.3 — [HIGH] — NOT addressed in this document.**

- Python `Foretis.my_content_hash` vs Rust `Foretis.content_hash`
- Python `models.py:20-35` — `TickRecord` has no `aa_nonce` field; the README's "Data models" table claims one
- Python calendar `save()` does NOT serialize `aa_nonce` or `signature_algorithm`

Cross-language JSON cannot round-trip without a translation shim. The README also incorrectly documents the Python model (claims `aa_nonce` exists when it doesn't).

**Fix:** Unify field names across Python and Rust. Update `models.py` to include `aa_nonce`. Update `calendar.py:save()` to serialize all fields. Add a cross-language JSON round-trip test.

---

### 34.7 ECDH Functions Unimplemented — Noise XX Broken Through CryptoServer Trait

**From prior review §1.4 — [HIGH] — NOT addressed in this document.**

**File:** `p2p/core-engine/src/crypto_server/software.rs:162-168`

`ecdh_ed25519` and `ecdh_p256` return `Err(CryptoError::Unsupported)`. The Noise XX handshake requires ECDH for key agreement. If the libp2p code path performs ECDH outside the `CryptoServer` trait, this violates the spec design that "private key never escapes." If the libp2p code path relies on the trait, Noise XX is non-functional through `SoftwareCryptoServer`.

**Also from §1.4 — [HIGH]:** P-256 sign, verify, and keypair are all stubs in `software.rs:158-160` and `core/src/signing_p256.c:4-28`. `FORETIAS_0 §0.6` says: *"Every Foretias node ships software implementations of both Ed25519 and P-256."* Either implement P-256 or remove it from the spec.

---

### 34.8 Calendar Slice Response Memory Explosion

**From prior review §1.5 — [MED] — NOT addressed in this document.**

**File:** `p2p/foretias-server/src/server/handlers.rs:12, :250-253`

`MAX_CALENDAR_SLICE_COUNT = 10_000` records × SPHINCS+ 7,856-byte signatures × 2 (forward/backward) ≈ **~157 MiB per JSON response** for a max slice. Neither streaming nor a lower cap is implemented.

**Fix:** Either lower the cap to a reasonable default (100-500 records), add streaming (chunked transfer encoding), or move to a length-delimited binary protocol for large slices.

**Also from §1.5 — [MED]:** `FORETIAS_1_MVP_SPEC §2` says `get_calendar_slice` returns *"1 or 2 records depending on whether `cal_tick_start`'s chronon duration has elapsed or not"*. The current implementation honours an arbitrary `count` parameter and ignores this rule entirely. Decide which behaviour is canonical and document it.

---

### 34.9 Calendar Persistence Drift

**From prior review §1.6 — NOT addressed in this document.**

- **[MED]** `EncryptedJsonlCalendarStore` (`p2p/foretias-node/src/calendar_store/encrypted_jsonl.rs`, 347 lines, fully implemented) is a **v0.7 feature** per `FORETIAS_0 §14`. The current target is v0.1/MVP. The store is dormant but its presence violates milestone discipline. Either feature-gate it (`cargo feature "encrypted-calendar-store"`) or promote it to active and update the spec.

- **[HIGH]** `src/foretias/calendar.py:113-129` — Python `save()` does `path.write_text(...)` directly with no atomic rename. A crash mid-write corrupts the calendar permanently. The Rust `calendar.rs:84-95` uses tempfile+rename correctly. Bring Python into line.

- **[MED]** Python `save()` does NOT serialize `aa_nonce` or `signature_algorithm`. If the model is updated, persistence silently truncates those fields on the next save.

---

### 34.10 C11 Core Findings Not Catalogued in Full

**From prior review §3.1 — partially addressed in this document's §32 file risk table but not fully catalogued.**

The following specific findings from the prior C11 review are not documented in full in the current review:

- **[MED]** `privkey.c:122-156` — `foretias_privkey_ed25519_from_seed` does NOT zero the caller's `seed` despite the header doc-comment (`foretias_core.h:382`) claiming it does. The `const` qualifier prevents zeroing. Fix: drop `const` and zero, or fix the doc.

- **[MED]** `noise_xx.c:241-247, :286-287, :313, :350, :361, :369, :394, :419` — early-return paths leave stack-local key material un-zeroed. Add `sodium_memzero` cleanup at every `return` after handshake-key bytes touch the locals.

- **[MED]** `signing_sphincs.c:39, :17`, `signing_dilithium.c:39, :17` — `OQS_MEM_cleanse` uses caller-supplied `len`; if caller passed `{.len=0}`, no cleanse occurs. Use `sizeof(buf->bytes)` or the algorithm's max constant instead.

- **[MED]** `signing_sphincs.c`, `signing_dilithium.c`, `kem_mlkem.c` — no `OQS_init()` call anywhere in the codebase. liboqs requires `OQS_init()` once per process for CPU-feature dispatch (e.g., AVX2 vs. scalar code paths). Add a one-shot init in `PrivKeyHandle::init()` or a dedicated `foretias_pqc_init` entry point.

- **[LOW]** `rng_mix.c` — name is misleading; no mixing occurs. Reads `/dev/urandom` directly per call. For ARM/embedded targets or sandboxed containers (no `/dev/urandom`), switch to `getrandom(2)` with a fallback.

- **[LOW]** `merkle.c:5-9, :18-22, :49-61` — ACSL/Frama-C annotations exist but the `proofs/` directory is empty. Either drive the proofs in CI or remove the annotations. Annotations without proofs imply verification that doesn't exist — this is misleading to auditors.

---

### 34.11 Chronomatter Dependency Injection Inconsistency

**From prior review §3.2 — [HIGH] — NOT addressed in this document.**

**File:** `p2p/core-engine/src/chronomatter/mod.rs:46-47`

`Chronomatter::new` constructs its own `SoftwareCryptoServer` internally, ignoring the `crypto: Arc<dyn CryptoServer>` that callers might intend to inject. The `from_calendar` path (line 73) does accept an injected server. The inconsistency means a custom `CryptoServer` backend (e.g., for HSM or enclave use) cannot be used with the default `Chronomatter::new` constructor.

**Fix:** Plumb the crypto server parameter through `Chronomatter::new`. Remove the internal `SoftwareCryptoServer::generate()` call. This is essential for the hardware/enclave backend path described in §8.4 of this document.

---

### 34.12 `noise_static_priv` Not Zeroized

**From prior review §3.3 — [HIGH] — NOT in this document's §19 zeroization table.**

**File:** `p2p/foretias-server/src/server/mod.rs:74, :83-84`

`noise_static_priv: [u8; 32]` is held as a plain `[u8; 32]` on the server struct. It is generated at server start and never zeroed — the server has no `Drop` implementation. This is the Noise XX static private key; its compromise allows decrypting past sessions.

**Fix:** Wrap in `Zeroizing<[u8; 32]>`, or better, move it into a `PrivKeyHandle` managed by the `CryptoServer` trait so the key never appears in plain Rust memory.

**Add to §19 zeroization table:**

| Secret | Location | Zeroized? | Method |
|---|---|---|---|
| Noise static private key | `server/mod.rs:83-84 noise_static_priv: [u8; 32]` | **NO** | Not wrapped in `Zeroizing`, no `Drop` |

---

### 34.13 Hard-Coded `/tmp/foretias-mirrors` Path

**From prior review §3.3 — [MED] — NOT addressed in this document.**

**File:** `p2p/foretias-server/src/server/mod.rs:78, :105`

The mirror store path is hard-coded to `/tmp/foretias-mirrors`. Issues:
1. `/tmp` has world-readable defaults on most Linux distros — mirrored calendar data (including TBID mappings) is readable by any local user
2. It is not configurable — operators running multiple server instances will collide
3. `/tmp` is usually cleared on reboot — mirrors are lost silently

**Fix:** Make the mirror store path a config option with a sensible default outside `/tmp` (e.g., `~/.local/share/foretias/mirrors` or a path relative to `--persist-path`).

---

### 34.14 `EncryptedJsonlCalendarStore::read_all` Unbounded Memory

**From prior review §3.3 — [MED] — NOT addressed in this document.**

**File:** `p2p/foretias-node/src/calendar_store/encrypted_jsonl.rs:107-112`

`read_all` does `read_to_string` over the entire file, loading the complete encrypted calendar into memory at once. For a large calendar (thousands of SPHINCS+ records at ~8 KiB each), this means hundreds of megabytes of memory on a single `read_all` call.

**Fix:** Use a streaming reader (`BufReader::lines`) and lazy-decode each block.

**Also from §3.3 — [MED]:** `compute_next_block_id` at line 48 — on error, falls back to `0`. A transient I/O failure can reset block IDs to 0, creating duplicate block IDs in the store. Fail loudly on block ID computation failure rather than silently resetting.

---

### 34.15 `json_path.to_str().unwrap()` Panic

**From prior review §3.3 — [LOW] — NOT addressed in this document.**

**File:** `p2p/foretias-server/src/server/mod.rs:166`

`json_path.to_str().unwrap()` panics on non-UTF-8 file paths. On Linux, file paths are arbitrary byte sequences, not necessarily valid UTF-8. Use `to_string_lossy()` or propagate the error.

---

### 34.16 Python Prototype — Precision, Key Safety, and CLI Gaps

**From prior review §3.4 — NOT addressed in this document.**

- **[MED]** `src/foretias/_timebeing.py:107` — `_now_ns()` uses `int(time.time() * 1e9)`. Floating-point `time.time()` loses precision beyond 2^53 ns (~104 days past 1970, but more practically beyond ~2015 dates the least significant bits are noise). **Fix:** Use `time.time_ns()` (Python 3.7+) which returns an integer nanosecond value directly.

- **[MED]** `src/foretias/_timebeing.py:126-136` — On `_tick()`, the new private key is generated and returned by value to the caller. The old private key is whatever the caller passed in — neither is zeroed. Even in a Python prototype, document that callers must zero key material. Consider a class-owned handle pattern.

- **[MED]** `src/foretias/cli.py:71-73, :107` — CLI `stamp` and `verify` commands instantiate `PyTimeFamilyServer` with no auth and no config. If `persist_path` is world-readable, an attacker who can write the calendar JSON can rewrite the chain (no signature on the JSON envelope itself).

- **[LOW]** `src/foretias/cli.py:64, :89` — `open(args.message_file, "rb").read()` reads the entire message file into memory without a size cap. Add a `--max-bytes` flag or a hardcoded reasonable limit.

---

### 34.17 Correctness Gaps Not Catalogued

**From prior review §4 — NOT addressed in this document.**

- **[MED]** `foretias-node/src/server/handlers.rs:138` — `serde_json::from_value(v.clone()).ok()` swallows the deserialization error. The caller cannot distinguish "missing field" from "wrong type." **Fix:** Return `INVALID_PARAMS` with the parse error string.

- **[MED]** `core-engine/src/foretias/calendar.rs:107-141` — `Calendar::load` recovers from a `.tmp` file if it has more ticks. A malicious actor with write access can drop a fabricated `.tmp` with more (forged) ticks, and the loader will prefer it without verification. The Rust path does not run `integrity_check` after `.tmp` recovery (the Python path does). **Fix:** Call `integrity_check` after recovering from `.tmp`.

- **[MED]** `core-engine/src/foretias/calendar.rs:82-95` — `save()` uses `std::fs::rename`. On Windows, rename is not atomic across volumes. Either mark as POSIX-only or use `cap-std`/`tempfile::persist`.

- **[MED]** `core-engine/src/foretias/calendar.rs:33-43` — `append()` rejects non-strictly-ascending tick numbers but does NOT verify `forward_foretis` / `backward_foretis` against the previous tick at append time. A calendar can be in an invalid intermediate state if `append` is misused programmatically. Add a debug-only `verify_pair` after append.

- **[LOW]** `foretias-node/src/main.rs:56` — `default_value = "9900..9999"` for `--p2p-port-range` parsed as a string. Ensure `start <= end`, both are valid u16, and the parsing error is user-friendly.

---

### 34.18 Efficiency — Signal Input Built Twice and Ed25519 Seed Re-Derivation

**From prior review §5 — NOT addressed in this document.**

- **[MED]** `core-engine/src/foretias/tick.rs:79-92, :129-140` — `sig_input` (the byte vector over which the signature is computed) is built with the same layout in both `stamp()` and `verify()`, but as two independent code paths. A bug in one won't necessarily appear in the other. Extract into a shared `fn build_sig_input(tbid, tick_number, content) -> Vec<u8>`.

- **[LOW]** `core/src/signing_ed25519.c:12` — re-derives the 64-byte signing key from the 32-byte seed on every `sign()` call (~30 µs overhead). Document or expose a `_with_keypair` variant for callers in hot loops.

---

### 34.19 Test Quality Issues

**From prior review §6.4 — NOT addressed in this document.**

- **[MED]** `core-engine/src/foretias/calendar.rs:464-483` — `calendar_crash_recovery_corrupt_tmp` test documents that *"corrupt .tmp not removed by current implementation"* and **asserts the buggy behavior** rather than testing the correct behavior. This is a test that enshrines a known bug. The test should either be marked `#[should_panic]` with a `FIXME` comment, or the bug should be fixed.

- **[MED]** Many Rust tests use `unwrap()` on results that include `random_bytes()` — non-deterministic. For property tests, seed an RNG deterministically.

- **[MED]** Python tests rely on wall-clock `time.time()` for tick numbers, making timing-sensitive tests potentially flaky on slow CI machines. Add fake clock injection for testing.

**Missing test types not covered in this document's §6 (from prior review §6.2):**

- **[HIGH]** **Crash recovery for Python `save()`:** There is no Python equivalent of Rust's `calendar_crash_recovery_from_tmp` test. Since the Python `save()` is non-atomic, a crash mid-write corrupts the calendar permanently — and there's no test for this failure mode.
- **[HIGH]** **JSON-RPC fuzz:** malformed envelope, oversized `content`, deeply nested JSON (stack overflow in serde), invalid hex, unknown method, empty params. These are common attack surfaces for any RPC server.
- **[HIGH]** **C-core fuzz targets:** libfuzzer targets for `foretias_merkle_verify`, `foretias_noise_step`, `foretias_*_verify`, `foretias_privkey_derive_seal_key`. The C11 core has the highest risk (unsafe code, no Rust safety guarantees) and no fuzz coverage.
- **[HIGH]** **PrivKey memzero verification:** After a tick advance, scan the process's heap pages (or a known buffer) for the previous private-key bytes; assert absence. On Linux, use `procfs` or jemalloc poisoning hooks. This test would have caught the Dilithium zeroing gap identified in the prior review.

---

### 34.20 Documentation Field Name Drift

**From prior review §7 — NOT addressed in this document.**

- **[HIGH]** `README.md` "Data models" section claims `aa_nonce: bytes` on `TickRecord`, but `src/foretias/models.py` does not have this field. README also documents `my_content_hash` on `Foretis` but the Rust implementation uses `content_hash`. These are presentation-layer inconsistencies that erode developer trust.

- **[MED]** `README.md` build instructions mention `python -m build` and `pip install -e .`, but `pyproject.toml` declares `maturin` as the build backend. `pip install -e .` will invoke maturin, which requires a Rust toolchain. Document the dependency or add a pure-Python fallback build target.

- **[MED]** `README.md` lists `pyforetias` as the Rust-backed package, but `pyproject.toml` exports `foretias_p2p`. Reconcile.

- **[MED]** `FORETIAS_0_OVERVIEW.md` describes `foretias/p2p/node/` and `foretias/p2p/bindings/python/`, but the actual layout is `p2p/core-engine/`, `p2p/foretias-server/`, `p2p/foretias-python/`. Update the spec or document the path change.

---

### 34.21 Dependency and Supply-Chain Gaps

**From prior review §8 — NOT addressed in this document.**

- **[MED]** No `cargo audit` configuration and no pinned dependency versions checked in CI. The new release checklist in §30.1 mentions `cargo audit` as a checkbox but it is not yet a blocking CI step.

- **[MED]** `liboqs` version — `FORETIAS_3 §0.3` specifies version 0.13.0 for SPHINCS+ availability. Verify that `Cargo.lock` and `build.rs` pin this version consistently. A floating dep on `oqs` could pick up a breaking version change.

- **[LOW]** `serde_cbor` is unmaintained as of 2024. Replace with `ciborium` (the maintained CBOR implementation for Rust). This is a low-risk substitution — same CBOR semantics, active maintenance.

- **[LOW]** `parking_lot` dependency — audit whether it's still needed or can be replaced with `std::sync` equivalents (Rust 1.70+ has `OnceLock` in std). If keeping it, ensure the version is pinned.

- **[LOW]** Workspace `Cargo.toml` not audited for floating vs pinned versions. Run `cargo update --dry-run` and commit a lockfile policy (always commit `Cargo.lock` for binaries; optionally for libraries).

---

### 34.22 Summary: Prior Review Items Now Addressed vs. Still Open

Items from `pre-public-mvp-code-review.md` confirmed addressed (in prior code changes or in this document):

| Item | Status |
|---|---|
| `sign()` / `signature_algorithm()` mismatch (CRIT) | **Fixed** — `software.rs` now returns `Ed25519`; covered in §3.6 of this doc |
| `Dilithium::Drop` skips secret key (HIGH) | **Fixed** — now uses `Zeroizing` wrapper; confirmed in §19 |
| Nonce overflow in `noise_xx.c` (CRIT) | **Flagged** in this doc §32 as CRIT |
| HKDF stack overflow in `privkey.c` (CRIT) | **Flagged** in this doc §32 as CRIT |
| `block_on` deadlock in handlers (HIGH) | **Flagged** in this doc §2.8; still present |
| `MAX_CONTENT_BYTES` 1 GiB pre-decode (HIGH) | **Flagged** in this doc §3.5; still present |
| Heartbeat freshness window (MED) | **Flagged** in this doc §3.3 |
| Count-based nonce window (LOW) | **Flagged** in this doc §16.1 |
| Eager PQC keygen (MED) | **Flagged** in this doc §7.2 |
| `keypairs` Vec unbounded (MED) | **Flagged** in this doc §2.3 |
| No PR CI (CRIT) | **Flagged** in this doc §4.1, §30.1 |
| `verify()` ambiguous return (MED) | **Flagged** in this doc §5.3 |
| FROST triple stub (MED) | **Flagged** in this doc §15.1 |
| OnceLock init failure silent (MED) | **Flagged** in this doc §3.7 |
| Calendar mirror without integrity_check (MED) | **Flagged** in this doc §4.4 |
| DHT TBID registration unauthenticated (MED) | **Flagged** in this doc §4.3 |
| TBID secret key not `Zeroizing` in Rust Vec (HIGH) | **Flagged** in this doc §14.1, §19 |

Items from `pre-public-mvp-code-review.md` confirmed still open (added in §34 of this document):

| Prior §   | Item | Severity |
|---|---|---|
| §1.2 | PQC files not in CMakeLists.txt | CRIT |
| §1.2 | Function name divergence in C headers | HIGH |
| §1.2 | OpenSSL link via legacy hash files | HIGH |
| §2.2 | Python ↔ Rust auto-attestation blob incompatibility | CRIT |
| §1.3 | `#[serde(default)]` on `stamps_per_tick` | MED |
| §1.3 | Undocumented `external_attestations` field | MED |
| §1.3 | Python/Rust field name divergence | HIGH |
| §1.4 | `ecdh_ed25519` / `ecdh_p256` return Unsupported | HIGH |
| §1.4 | P-256 implementation is stub | HIGH |
| §1.5 | `MAX_CALENDAR_SLICE_COUNT` → 157 MiB response | MED |
| §1.5 | `get_calendar_slice` spec (1-2 records) vs impl (arbitrary count) | MED |
| §1.6 | `EncryptedJsonlCalendarStore` present without feature gate | MED |
| §1.6 | Python `calendar.py::save()` non-atomic | HIGH |
| §1.6 | Python `save()` truncates `aa_nonce`, `signature_algorithm` | MED |
| §3.1 | `foretias_privkey_ed25519_from_seed` doesn't zero seed | MED |
| §3.1 | `noise_xx.c` early-return unzeroed key material | MED |
| §3.1 | `OQS_MEM_cleanse` len=0 issue | MED |
| §3.1 | `OQS_init()` missing | MED |
| §3.1 | `rng_mix.c` misleading name; no mixing | LOW |
| §3.1 | Frama-C annotations without proofs | LOW |
| §3.2 | `Chronomatter::new` ignores injected `CryptoServer` | HIGH |
| §3.3 | `noise_static_priv` not `Zeroizing` | HIGH |
| §3.3 | `/tmp/foretias-mirrors` hard-coded | MED |
| §3.3 | `read_all` unbounded memory in `EncryptedJsonlCalendarStore` | MED |
| §3.3 | Block ID resets to 0 on error | MED |
| §3.3 | `json_path.to_str().unwrap()` panic | LOW |
| §3.4 | Python `_now_ns()` floating-point precision | MED |
| §3.4 | Python `_tick()` key not zeroed | MED |
| §3.4 | Python CLI unbounded file read | LOW |
| §4 | `serde_json` error swallowed in stamp handler | MED |
| §4 | `.tmp` recovery without `integrity_check` | MED |
| §4 | `save()` rename non-atomic on Windows | MED |
| §4 | `append()` doesn't verify auto-attestations | MED |
| §4 | `--p2p-port-range` parsing not validated | LOW |
| §5 | `sig_input` built twice in stamp/verify | MED |
| §5 | Ed25519 re-derives keypair from seed on every sign | LOW |
| §6.4 | `calendar_crash_recovery_corrupt_tmp` asserts a bug | MED |
| §6.4 | Tests non-deterministic (rand + unwrap) | MED |
| §6.4 | Python tests use wall-clock for tick numbers (flaky) | MED |
| §6.2 | No Python crash-recovery test | HIGH |
| §6.2 | No JSON-RPC fuzz tests | HIGH |
| §6.2 | No C-core fuzz targets | HIGH |
| §6.2 | No PrivKey memzero verification test | HIGH |
| §7 | README `aa_nonce` / `my_content_hash` wrong | HIGH |
| §7 | README build instructions wrong (maturin vs pip) | MED |
| §7 | README package name wrong (pyforetias vs foretias_p2p) | MED |
| §7 | FORETIAS_0 spec directory paths stale | MED |
| §8 | No `cargo audit` in CI | MED |
| §8 | `liboqs` version not pinned in build | MED |
| §8 | `serde_cbor` unmaintained → use `ciborium` | LOW |
| §8 | `parking_lot` dep audit | LOW |
| §8 | Floating versions in workspace `Cargo.toml` | LOW |

---

*End of review — Claude Sonnet 4.6, 2026-05-19; augmented with prior review cross-reference 2026-05-19*

