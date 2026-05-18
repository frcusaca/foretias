# Foretias — P2P Sub-Spec 4: Hardening, Observability & Encrypted Persistence (v0.5)

- [x] backburnered

**Milestone tag:** `v0.5-hardening`
**Prereq:** `v0.4-dht-discovery` must be tagged.
**Next:** `FORETIAS_2_P2P_6_probity_gossip.md` (v0.6 — GossipSub and ProbityReport).

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## READING ORDER

1. Confirm `v0.4-dht-discovery` is tagged.
2. Read the full v0.2–v0.4 implementation before starting this spec.
   Threat model (§3) and observability (§4) require understanding all
   currently exposed surfaces.
3. Read `FORETIAS_0_OVERVIEW.md` §0.3 (what persists) before touching
   the calendar persistence format.
4. Read this document end to end before writing any code.

---

## 1. GOAL

Four independent deliverables, all shipped in this milestone:

1. **Threat model document** — written analysis of the v0.2–v0.4 attack surface with mitigations, most of which are already implemented in prior sub-specs.
2. **Admin / observability endpoints** — extend the JSON-RPC server with read-only admin methods and structured tracing.
3. **Encrypted JSONL calendar persistence** — replace the plaintext JSON calendar file with an encrypted append-only format; old files are migrated on first load.
4. **Formal verification PoC** — prove absence of out-of-bounds memory access and tree-root purity for `foretias/p2p/core/src/merkle.c` using Frama-C with the WP (weakest-precondition) plugin.

(@human — these four pieces are bundled into one milestone because they
are all hardening concerns that should land before probity gossip (v0.6)
makes the network observable to external parties. Encrypted persistence
in particular matters before any gossip traffic starts: once peers can
read each other's calendars via gossip, local storage should already be
opaque to disk readers.)

---

## 2. ACCEPTANCE CRITERIA (SUMMARY)

| Deliverable | Acceptance gate |
|---|---|
| Threat model | Written doc present at `docs/threat_model_v0_5.md`; reviewed by human |
| Observability | `foretias rpc --method get_health` returns JSON with version, tick, peer count |
| Encrypted persistence | Old plaintext calendar survives a round-trip: load → re-save encrypted → reload; all ticks and attestations preserved; file is unreadable without the node's key |
| Frama-C PoC | `make frama-c-merkle` exits 0; proof artifact committed to `p2p/core/proofs/` |

---

## 3. THREAT MODEL — `docs/threat_model_v0_5.md`

The coding agent's job here is to **write a markdown document** at
`docs/threat_model_v0_5.md` covering the sections below. This document
is a deliverable in its own right, not a comment in code.

### 3.1 Scope

Attack surface in scope: the v0.2–v0.4 implementation as tagged.
Out of scope: v0.6+ gossip layer (not yet shipped), v0.9+ enclave backend.

### 3.2 Sections required in the threat model document

**A. Asset inventory**
List what is worth protecting: private keys (in-memory only, never on
disk), calendar integrity, external attestation authenticity, peer
identity binding, liveness of the mutual-attest loop.

**B. Attacker model**
Define three attacker levels the document reasons about:
- *Network observer* — can read all TCP traffic (no Noise on JSON-RPC by design).
- *Active network attacker* — can inject and replay TCP packets.
- *Compromised peer* — a legitimate peer behaving maliciously.

**C. Threat catalogue**

For each threat: name, description, affected component, impact, mitigation
(already-implemented or deferred-to-which-sub-spec).

Required entries:

| Threat | Component | Mitigation |
|---|---|---|
| Content replay | `/stamp` JSON-RPC | Echo field encodes requester identity + tick; replayed response fails echo check in v0.2 §6.2 step 14 |
| Stamp flood (DoS) | Chronomatter job queue | Two-layer rate limit (v0.2 §4.1); queue depth cap |
| Oversized request | All JSON-RPC handlers | `MAX_CONTENT_BYTES` cap in handlers (v0.1); `MAX_REQUEST_BYTES` in cross-attest path (v0.2) |
| DHT eclipse attack | Kademlia peer table | Private namespace key (v0.4); bootstrap list kept in config; Kademlia's own bucket diversity |
| Sybil via RNG failure | Identity generation | `rng_mix.c` mixes multiple entropy sources (v0.1 core); collision detection (v0.7) is backstop |
| Forged external attestation | Calendar | Verification in v0.2 §6.2 steps 12–14; unverified attestations are never stored |
| Calendar file tampering | Disk | Encrypted JSONL (this sub-spec §5); authentication tag per block detects tampering |
| Private-key exfiltration via disk | CryptoServer | Keys never written to disk (§0.3 invariant); `zeroize` on tick advance |
| Tick-rate exhaustion (peer forces rapid tick advance) | Chronomatter | Rate limit gates inbound stamp jobs; tick advance is timer-driven, not stamp-driven (v0.2) |
| JSON-RPC address spoofing via identify | Communerd | Address is advisory only; actual RPC call still requires verified Foretis response (v0.2 §6.2) |

**D. Residual risks and deferred mitigations**
Explicitly list threats with no current mitigation and note which future
sub-spec addresses them. Example: "GossipSub message amplification — no
current mitigation; addressed in v0.6 with mesh size and flood-publish
caps."

**E. Recommendations for v0.5 code changes**
List any immediate code hardening the threat analysis motivates:
- Enforce `MAX_PEERS` cap in `DhtPeerSource` (prevent table exhaustion).
- Add `max_queue_depth` to Chronomatter job queue; reject with `QueueFull` error when exceeded.
- Validate `dht_namespace` config value is non-empty and under 64 bytes.

(@human — the threat model document is a living artifact. Future sub-specs
should update it as new attack surface is introduced. The v0.5 version is
the baseline; v0.6 adds gossip-specific threats, v0.7 adds collision-
specific threats.)

---

## 4. ADMIN / OBSERVABILITY

### 4.1 New JSON-RPC admin methods

Add to `src/server/handlers.rs`. All admin methods are read-only.

```
Method: get_health
Params: {}
Result: {
  "version":        1,
  "tbn":            "tf-abc123",
  "tbid":           "abc123...",
  "current_tick":   42,
  "chronon_ns":     20000000000,
  "peer_count":     3,
  "dormant":        false,
  "uptime_secs":    3721
}

Method: get_peers
Params: {}
Result: [
  { "peer_id": "12D3KooW...", "json_rpc": "127.0.0.1:4002", "last_seen_ns": 1714000000000000000 },
  ...
]

Method: get_metrics
Params: {}
Result: {
  "stamps_total":              1024,
  "stamps_rate_limited":       3,
  "mutual_attest_sent":        88,
  "mutual_attest_ok":          85,
  "mutual_attest_failed":      3,
  "calendar_flush_count":      12,
  "calendar_size_bytes":       40960
}
```

These three methods are added to the existing `match request.method.as_str()` block in `src/server/mod.rs`.

### 4.2 Metrics counters

```rust
// src/metrics.rs  NEW
use std::sync::atomic::{AtomicU64, Ordering};

pub struct NodeMetrics {
    pub stamps_total:          AtomicU64,
    pub stamps_rate_limited:   AtomicU64,
    pub mutual_attest_sent:    AtomicU64,
    pub mutual_attest_ok:      AtomicU64,
    pub mutual_attest_failed:  AtomicU64,
    pub calendar_flush_count:  AtomicU64,
}

impl NodeMetrics {
    pub fn inc(&self, field: MetricField) {
        match field {
            MetricField::StampsTotal         => self.stamps_total.fetch_add(1, Ordering::Relaxed),
            // ... etc
        };
    }
}
```

`Arc<NodeMetrics>` is held on `TimeFamilyInner` and passed to each component at construction. Every site that already logs a WARN or INFO about stamps/attestations gets a `metrics.inc(...)` call alongside.

### 4.3 Structured tracing spans

Wrap the mutual-attest execution path in a tracing span:

```rust
let span = tracing::info_span!("mutual_attest", peer = %peer_addr.json_rpc, local_tick);
let _guard = span.enter();
```

Wrap the stamp job execution path in Chronomatter:

```rust
let span = tracing::info_span!("stamp_job", tick, echo = %job.echo);
```

No new dependencies — `tracing` is already in Cargo.toml from v0.1.

---

## 5. ENCRYPTED JSONL CALENDAR PERSISTENCE

This replaces the plaintext `Calendar::save` / `Calendar::load` used since v0.1.
The new format is an append-only file of encrypted blocks. The encryption
key is derived from the node's own `CryptoServer` so only the same process
identity can read the file.

(@human — note: the node's identity is ephemeral per §0.1 of the design
invariants. This means a calendar file written by run N cannot be decrypted
by run N+1, which has a different identity. This is intentional: the
calendar is a local audit log for the current run. Offline verification
uses the public keys recorded in each TickRecord's forward/backward foretis;
it does not require decrypting the private file. The encrypted format
protects against a disk-reader inferring the node's stamping history
or harvesting external attestation metadata.)

### 5.1 Block format

```rust
// src/calendar_store/encrypted_jsonl.rs  NEW

#[derive(Serialize, Deserialize)]
pub struct CalendarBlock {
    pub block_id:     u64,            // monotonically increasing; 0 = first block
    pub written_at_ns: u64,
    pub ticks:        Vec<TickRecord>, // one or more tick records
}
```

Each block is serialised to JSON, then sealed:

```
sealed = crypto_server.seal_for_self(json_bytes)
line   = base64_standard_encode(cbor_encode(sealed)) + "\n"
```

`seal_for_self` uses ChaCha20-Poly1305 with an ephemeral nonce and the
node's current public key as the recipient (ECDH key agreement or
symmetric derivation depending on backend; software backend uses a
deterministic key derived from the Ed25519 seed via HKDF).

The file is a sequence of such lines — one block per line, append-only.

### 5.2 `CryptoServer` additions

```rust
// src/crypto_server/mod.rs  additions
pub trait CryptoServer: Send + Sync {
    // ... existing methods ...

    /// Seal `plaintext` so only this same CryptoServer instance can unseal it.
    /// Returns an opaque blob (nonce + ciphertext + tag).
    fn seal_for_self(&self, plaintext: &[u8]) -> Result<SealedBlob, NodeError>;

    /// Unseal a blob produced by seal_for_self on this same instance.
    fn unseal_for_self(&self, blob: &SealedBlob) -> Result<Vec<u8>, NodeError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedBlob {
    pub nonce:      Vec<u8>,   // 12 bytes for ChaCha20-Poly1305
    pub ciphertext: Vec<u8>,   // plaintext length + 16 bytes AEAD tag
}
```

`SoftwareCryptoServer` derives the symmetric key via HKDF-SHA256 over
the Ed25519 seed with info string `b"foretias-calendar-seal-v1"`.

### 5.3 `EncryptedJsonlCalendarStore`

```rust
pub struct EncryptedJsonlCalendarStore {
    path:   PathBuf,
    server: Arc<dyn CryptoServer>,
    next_block_id: AtomicU64,
}

impl EncryptedJsonlCalendarStore {
    /// Append a batch of TickRecords as one block.
    pub fn append_block(&self, ticks: Vec<TickRecord>) -> Result<(), NodeError> {
        let block = CalendarBlock {
            block_id:      self.next_block_id.fetch_add(1, Ordering::Relaxed),
            written_at_ns: now_ns(),
            ticks,
        };
        let json  = serde_json::to_vec(&block)?;
        let blob  = self.server.seal_for_self(&json)?;
        let line  = base64::engine::general_purpose::STANDARD
            .encode(serde_cbor::to_vec(&blob)?);
        let mut f = std::fs::OpenOptions::new().append(true).create(true).open(&self.path)?;
        writeln!(f, "{}", line)?;
        Ok(())
    }

    /// Read all blocks; returns them in block_id order.
    pub fn read_all(&self) -> Result<Vec<CalendarBlock>, NodeError> {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        let mut blocks = Vec::new();
        for line in text.lines() {
            let blob_bytes = base64::engine::general_purpose::STANDARD.decode(line.trim())?;
            let blob: SealedBlob = serde_cbor::from_slice(&blob_bytes)?;
            let plain = self.server.unseal_for_self(&blob)?;
            let block: CalendarBlock = serde_json::from_slice(&plain)?;
            blocks.push(block);
        }
        blocks.sort_by_key(|b| b.block_id);
        Ok(blocks)
    }
}
```

### 5.4 Storage policy and bin-based LRU

For v0.5, implement `CalendarStoragePolicy` and `BinBasedLru` as defined in
`FORETIAS_2_P2P_SPEC.md` (the original Part 12.2 and 12.3). These are
straightforward; reproduce them in `src/calendar_store/lru.rs`. The LRU is
only active when `CalendarStoragePolicy::MyOwnPlusLru` is configured; the
default for v0.5 is `MyOwn` (store only this node's calendar on disk).

### 5.5 Migration from plaintext

On `Calendar` startup:

```rust
fn load_calendar(path: &Path, server: &Arc<dyn CryptoServer>) -> Result<Calendar, NodeError> {
    if looks_like_plaintext_json(path) {
        // v0.1–v0.4 format: load as before, then migrate
        let old = Calendar::load(path.to_str().unwrap())?;
        let store = EncryptedJsonlCalendarStore::new(path, Arc::clone(server));
        // Write all existing ticks in one block
        store.append_block(old.ticks.clone())?;
        // Rename old file to .bak
        std::fs::rename(path, path.with_extension("json.bak"))?;
        tracing::info!("migrated plaintext calendar to encrypted JSONL");
        Ok(old)
    } else {
        let blocks = EncryptedJsonlCalendarStore::new(path, Arc::clone(server)).read_all()?;
        let mut cal = Calendar::new(/* tbid, tbn from blocks[0] or config */);
        for block in blocks { for tick in block.ticks { cal.append(tick)?; } }
        Ok(cal)
    }
}

fn looks_like_plaintext_json(path: &Path) -> bool {
    std::fs::read_to_string(path).map(|s| s.trim_start().starts_with('{')).unwrap_or(false)
}
```

The `.bak` file is kept on disk so the human can inspect it once; it is
never loaded again.

---

## 6. FORMAL VERIFICATION POC — `merkle.c`

### 6.1 Target

`foretias/p2p/core/src/merkle.c` — the sparse Merkle tree implementation.

Properties to prove with Frama-C WP:
1. No out-of-bounds array or pointer access in any public function.
2. `foretias_merkle_root(...)` returns a value that is a pure function of its inputs (no hidden state read or written).
3. If two calls receive identical leaf arrays, they produce identical roots.

(@human — property 3 is a relational property. Frama-C WP can express this
via loop invariants over the leaf array. It does not require Coq or a
higher-order prover. The proof is bounded by the configured max-leaf-count
constant, which is acceptable for a PoC.)

### 6.2 Annotations required

Add ACSL (ANSI/ISO C Specification Language) annotations directly in
`merkle.c`. Annotations are C block comments starting with `/*@`:

```c
/*@ requires \valid(leaves + (0 .. n_leaves-1));
  @ requires \valid(root_out);
  @ requires n_leaves <= FORETIAS_MERKLE_MAX_LEAVES;
  @ assigns  root_out->bytes[0 .. 31];
  @ ensures  \valid(root_out);
  @ behavior pure:
  @   assigns root_out->bytes[0 .. 31];
  @   ensures \forall integer i; 0 <= i < 32 ==>
  @           root_out->bytes[i] == \old(computed_root(leaves, n_leaves))[i];
  @*/
ForetiasResult foretias_merkle_root(
    const ForetiasHash32* leaves,
    size_t               n_leaves,
    ForetiasHash32*       root_out
);
```

Full annotation coverage of all internal helpers is required for WP to
close the proof; partial coverage leaves open goals.

### 6.3 Build integration

```makefile
# foretias/p2p/core/Makefile  (new target)
FRAMA_C     ?= frama-c
WP_FLAGS     = -wp -wp-rte -wp-timeout 60

frama-c-merkle:
	$(FRAMA_C) $(WP_FLAGS) \
	    -cpp-extra-args="-I include" \
	    src/merkle.c src/hash_sha256.c src/memzero.c \
	    -then -report
	@echo "Frama-C WP: all goals proved"

.PHONY: frama-c-merkle
```

CI adds a job that runs `make frama-c-merkle` and fails the build if any
WP goal is not proved (exits non-zero).

### 6.4 Proof artifact

Frama-C WP produces a set of `.json` or `.why3` goal files in a `_why3`
or `wp_proofs` subdirectory. Commit these to
`foretias/p2p/core/proofs/merkle/` so the proof is reproducible without
re-running the prover.

(@human — Frama-C + WP + Why3 is installable on Ubuntu via apt. The CI
job only needs `frama-c` and the `wp` plugin (included in the standard
Frama-C distribution since 27+). The proof will take 10–60 s depending
on the complexity of the loop invariants. If Why3 fails to find a back-end
prover automatically, configure Alt-Ergo: `why3 config --add-prover alt-ergo`.)

---

## 7. CODE HARDENING (from threat model recommendations)

These are small, specific changes motivated by §3.2 section E:

```rust
// src/peer_conn/dht_peer_source.rs
const MAX_PEERS: usize = 256;

impl DhtPeerSource {
    pub fn upsert(&self, peer_id: PeerId, addr: PeerAddr) {
        let mut guard = self.peers.write();
        if guard.len() >= MAX_PEERS && !guard.contains_key(&peer_id) {
            tracing::warn!("peer table full ({MAX_PEERS}); ignoring new peer");
            return;
        }
        guard.insert(peer_id, addr);
    }
}
```

```rust
// src/chronomatter/mod.rs
const MAX_JOB_QUEUE_DEPTH: usize = 64;

// In the job submission path (TimeFamily inbound handler):
if job_tx.capacity() == 0 {  // bounded channel; 0 remaining = full
    return Err(NodeError::QueueFull);
}
```

```rust
// src/config.rs  — validation on load
fn validate(cfg: &Config) -> Result<(), ConfigError> {
    if cfg.network.dht_namespace.is_empty() || cfg.network.dht_namespace.len() > 64 {
        return Err(ConfigError::InvalidField("dht_namespace must be 1–64 bytes"));
    }
    Ok(())
}
```

---

## 8. CONFIGURATION ADDITIONS

```json
{
  "calendar": {
    "format":               "encrypted_jsonl",
    "flush_interval_secs":  30,
    "storage_policy":       "my_own"
  },
  "admin": {
    "enabled": true
  },
  "network": {
    "max_peers": 256
  }
}
```

---

## 9. TEST PLAN

- `seal_unseal_roundtrip` — `seal_for_self` then `unseal_for_self` on the software backend; output matches input.
- `seal_wrong_key_fails` — create two `SoftwareCryptoServer` instances; seal with one, attempt unseal with other; must fail.
- `encrypted_jsonl_append_read` — append 5 blocks; `read_all` returns all 5 in order.
- `plaintext_migration` — write a v0.1-format calendar JSON; run migration; reload via encrypted path; all ticks present; `.bak` file exists.
- `admin_get_health` — call `get_health` via JSON-RPC; response fields are present and typed correctly.
- `peer_table_max_peers` — insert `MAX_PEERS + 1` peers; table size stays at `MAX_PEERS`.
- `chronomatter_queue_full` — fill job queue; next submit returns `QueueFull`.
- `frama-c-merkle` — run as part of CI; exits 0.
- All v0.1–v0.4 tests pass.

---

## 10. NON-GOALS FOR v0.5

- ❌ GossipSub — v0.6.
- ❌ Signed admin responses — admin endpoints are local / trusted-network; no auth for now.
- ❌ Prometheus / OpenTelemetry export — the `get_metrics` JSON-RPC method is sufficient for v0.5; exporter integration deferred.
- ❌ Full Frama-C coverage of all C11 core files — PoC is `merkle.c` only; expand in future.

---

## 11. MILESTONE CHECKLIST

```
[ ] v0.5.1  Write docs/threat_model_v0_5.md (all sections §3.2 A–E).
[ ] v0.5.2  NodeMetrics counters; wire into stamp path and mutual-attest path.
[ ] v0.5.3  Admin JSON-RPC methods: get_health, get_peers, get_metrics.
[ ] v0.5.4  Structured tracing spans on stamp and mutual-attest paths.
[ ] v0.5.5  CryptoServer::seal_for_self / unseal_for_self on software backend.
[ ] v0.5.6  EncryptedJsonlCalendarStore: append_block, read_all.
[ ] v0.5.7  Plaintext migration path; seal_wrong_key_fails test passes.
[ ] v0.5.8  CalendarStoragePolicy + BinBasedLru in src/calendar_store/lru.rs.
[ ] v0.5.9  Code hardening: MAX_PEERS cap, MAX_JOB_QUEUE_DEPTH, config validation.
[ ] v0.5.10 ACSL annotations in merkle.c; make frama-c-merkle exits 0.
[ ] v0.5.11 Proof artifact committed to p2p/core/proofs/merkle/.
[ ] v0.5.12 All unit and integration tests pass; all prior milestone tests pass.
[ ] v0.5.13 TAG: v0.5-hardening.
```

(@human — at the end of v0.5 the node has a hardened, observed, and
encrypted-at-rest calendar. The formal proof on merkle.c is a small
but genuine correctness artifact that establishes the toolchain for
deeper verification later. v0.6 opens the network to probity gossip,
which is the first step where external parties can observe reputational
data — so it's important that the storage and rate-limit surface is solid
before that.)

---
# END OF SUB-SPEC 4 (v0.5)
