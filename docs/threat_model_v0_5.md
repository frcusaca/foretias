# Fortias v0.5 Threat Model

**Version:** v0.5-hardening
**Scope:** v0.2 through v0.4 implementation (auto-attestation, libp2p transport, DHT discovery)
**Date:** 2026-04-30
**Status:** Baseline — living artifact, updated with each milestone

---

## 1. Scope

**In scope:** The complete attack surface introduced across v0.2 (direct P2P auto-attestation), v0.3 (libp2p transport with two-layer identity), and v0.4 (Kademlia DHT discovery with `DhtPeerSource` and hybrid transport selection).

**Out of scope:** v0.6+ GossipSub layer (not yet shipped), v0.9+ secure enclave backend. Those will produce their own threat model addenda.

---

## 2. Asset Inventory

The following assets are worth protecting. Each has a trust boundary, current protection status, and the component responsible.

| Asset | Where it lives | Protection | Responsible component |
|---|---|---|---|
| **Tick private keys** | In-memory only, `Vec<TickKeyPair>` inside `Chronomatter` (`p2p/core-engine/src/chronomatter/mod.rs`) | Never written to disk. `PrivKeyHandle` from the C11 core (`p2p/core/src/`) provides `zeroize` on drop. Key rotated every tick. | `CryptoServer` trait (`p2p/core-engine/src/crypto_server/mod.rs`), `SoftwareCryptoServer` (`p2p/core-engine/src/crypto_server/software.rs`) |
| **Calendar integrity** | `Calendar` struct backed by `Chronomatter` tick records. Persisted to `NodeConfig::calendar_path` | Forward/backward `Fortis` signatures in each `TickRecord` form a chained attestation. `integrity_check()` in `Chronomatter` verifies the chain. | `Chronomatter` (`p2p/core-engine/src/chronomatter/mod.rs`), `fortias::tick::verify_pair()` |
| **External attestation authenticity** | `TickRecord.external_attestations` vector | Verified before storage per the mutual-attestation flow (v0.2). Unverified attestations are never appended to the calendar. | `Chronomatter::verify()`, stamp handler in `p2p/fortias-node/src/server/handlers.rs` |
| **Peer identity binding** | Two-layer identity: libp2p `PeerId` (transport layer, Noise protocol) + application TBID (Fortias identity) | libp2p's Noise handshake authenticates the transport `PeerId`. The application layer verifies Fortias signatures against the peer's TBID public key chain. | `Communerd` (`p2p/fortias-node/src/communerd/mod.rs`), `JsonRpcTransport` (`p2p/fortias-node/src/communerd/json_rpc_transport.rs`) |
| **Liveness of mutual-attest loop** | `PeerPool` liveness pings (`start_liveness_pings()`) + daemon tick loop in `Chronomatter` | Periodic RPC pings detect dead peers. Daemon tick advances on a timer, not on peer responses, so a slow peer cannot stall the clock. | `PeerPool` (`p2p/fortias-node/src/communerd/peer_pool.rs`), `Chronomatter::start_daemon()` |
| **DHT peer table** | `DhtPeerSource` (`p2p/fortias-node/src/communerd/dht_peer_source.rs`), an `Arc<RwLock<HashMap<PeerId, PeerAddr>>>` | Currently unprotected against exhaustion. No cap on table size (see §6, recommendation E1). | `DhtPeerSource::upsert()` |
| **Calendar on disk** | Plaintext JSON file at `calendar_path` (v0.1-v0.4 format) | Plaintext. Migration to encrypted JSONL with per-block AEAD authentication tag is part of v0.5 (§5 of FORTIAS_2_P2P_5_hardening.md). | `EncryptedJsonlCalendarStore` (new in v0.5) |

---

## 3. Attacker Model

Three attacker levels are considered, each with increasing capability.

### 3.1 Network Observer

**Capability:** Passive read access to all TCP traffic between nodes. Cannot modify, inject, or replay packets.

**Relevant to:**
- JSON-RPC traffic (TCP, no TLS by design). An observer reads all stamp requests, calendar queries, and their responses in cleartext.
- libp2p transport is encrypted via Noise, so the P2P control plane is opaque to passive observers.

**Impact assessment:** High for JSON-RPC content confidentiality, low for identity (Noise-encrypted libp2p connections reveal only the fact of connection).

### 3.2 Active Network Attacker

**Capability:** Full MITM on TCP. Can inject, drop, modify, and replay packets. Cannot break libp2p's Noise handshake or forge Ed25519 signatures.

**Relevant to:**
- JSON-RPC replay attacks (replaying a `/stamp` response with a different content)
- Connection flood (opening many TCP connections to exhaust resources)
- libp2p address spoofing via the identify protocol

**Impact assessment:** Medium. libp2p transport is resistant. JSON-RPC handlers rely on echo fields and signature verification, which limit replay damage.

### 3.3 Compromised Peer

**Capability:** A legitimate peer with valid keys, behaving maliciously. Can send forged requests, refuse to attest, or collude with other compromised peers.

**Relevant to:**
- Sybil identity generation (if RNG is weak)
- Tick-rate exhaustion (flood the victim with stamp requests to force rapid key rotation)
- Forged external attestations (submitting a valid-looking but incorrect attestation)

**Impact assessment:** Medium-high. The protocol's cross-signature verification in the mutual-attest flow limits what a single compromised peer can achieve. A majority compromise is a different threat class.

---

## 4. Threat Catalogue

| # | Threat | Description | Affected component | Impact | Mitigation |
|---|---|---|---|---|---|
| 1 | Content replay | An attacker captures a `/stamp` JSON-RPC response and replays it with different content, claiming a timestamp they did not earn. | `/stamp` JSON-RPC handler (`p2p/fortias-node/src/server/handlers.rs`) | High — undermines trust model | **Implemented.** Echo field encodes requester identity + tick number. A replayed response fails the echo check (v0.2 §6.2, step 14). The `handle_stamp` function returns the echo in the `Fortis` response for client-side verification. |
| 2 | Stamp flood (DoS) | Attacker sends a high volume of stamp requests to exhaust the node's CPU or job queue. | `Chronomatter` job queue (`p2p/core-engine/src/chronomatter/mod.rs`) | Medium — availability | **Partially implemented.** Two-layer rate limit from v0.2 §4.1. Queue depth cap added in v0.5 (§7, `MAX_JOB_QUEUE_DEPTH = 64`). Further hardening in v0.6 with GossipSub mesh caps. |
| 3 | Oversized request | A single request carries gigabytes of data, exhausting memory in the JSON-RPC handler. | All JSON-RPC handlers (`p2p/fortias-node/src/server/handlers.rs`) | Medium — availability | **Implemented.** `MAX_CONTENT_BYTES` (1 GB) in `handlers.rs` line 9. `MAX_CALENDAR_SLICE_COUNT` (10,000) limits calendar queries. `MAX_REQUEST_BYTES` in the cross-attest path (v0.2). |
| 4 | DHT eclipse attack | Attacker controls the majority of a node's Kademlia peer table, isolating it from honest peers. | Kademlia peer table (`p2p/fortias-node/src/communerd/dht_peer_source.rs`) | High — isolation | **Partially implemented.** Private namespace key (`dht_namespace` in `NodeConfig`, `p2p/core-engine/src/config.rs`) limits cross-network pollution. Bootstrap list in config prevents relying solely on DHT for initial discovery. Kademlia's own bucket diversity provides additional protection. `MAX_PEERS` cap added in v0.5 (§7). |
| 5 | Sybil via RNG failure | Weak or predictable RNG produces colliding TBIDs, allowing an attacker to impersonate another node. | Identity generation in `Chronomatter::new()` (`p2p/core-engine/src/chronomatter/mod.rs`, line 45-46) | High — identity compromise | **Implemented.** `rng_mix.c` in the C11 core (`p2p/core/src/`) mixes multiple entropy sources. v0.7 adds explicit collision detection as a backstop. |
| 6 | Forged external attestation | A compromised peer submits an attestation with valid structure but incorrect content (wrong tick, wrong key, wrong signature). | Calendar verification (`p2p/core-engine/src/chronomatter/mod.rs`) | High — integrity | **Implemented.** Full verification in v0.2 §6.2 steps 12-14: signature check against the peer's public key, tick number validation, TBID consistency. Unverified attestations are never stored. |
| 7 | Calendar file tampering | An attacker with disk access modifies the plaintext calendar file to insert or remove tick records. | Disk storage (calendar JSON file at `calendar_path`) | High — integrity | **Implemented in v0.5.** Encrypted JSONL format (`EncryptedJsonlCalendarStore`, new in v0.5) with per-block AEAD authentication tag (ChaCha20-Poly1305) detects any tampering. Migration path renames old file to `.bak`. |
| 8 | Private-key exfiltration via disk | An attacker reads the disk to recover tick private keys, allowing them to forge signatures. | `CryptoServer` key storage (`p2p/core-engine/src/crypto_server/software.rs`) | Critical — total compromise | **Implemented.** Keys are never written to disk (§0.3 invariant). `PrivKeyHandle` from C11 core uses `zeroize` on drop. Tick keypairs are regenerated every tick, so old keys are discarded immediately. |
| 9 | Tick-rate exhaustion | A peer forces the victim to advance its tick counter rapidly by sending many stamp requests, causing excessive key rotation and resource consumption. | `Chronomatter` (`p2p/core-engine/src/chronomatter/mod.rs`) | Medium — resource exhaustion | **Implemented.** Tick advance is timer-driven via `start_daemon()` (the daemon loop on line 239-267), not stamp-driven. Inbound stamp requests are rate-limited. A stamp call does advance the tick but the daemon interval provides a floor. |
| 10 | JSON-RPC address spoofing via identify | An attacker uses libp2p's identify protocol to advertise a false JSON-RPC address, redirecting RPC traffic. | `Communerd` + libp2p identify (`p2p/fortias-node/src/communerd/mod.rs`) | Low — misdirection | **Implemented.** The address from identify is advisory only. The actual RPC call still requires a verified Fortias response with correct Ed25519 signatures. A forged address results in a connection failure, not a security breach. |

---

## 5. Residual Risks and Deferred Mitigations

The following threats have no complete mitigation in v0.5 and are tracked for future sub-specs.

| Risk | Why it remains | Deferred to |
|---|---|---|
| **GossipSub message amplification** | GossipSub is not yet implemented. No mesh size or flood-publish caps exist. An attacker could flood the gossip mesh once it ships. | v0.6 — GossipSub and ProbityReport. Mesh size and flood-publish caps will be part of the initial implementation. |
| **No transport encryption on JSON-RPC** | JSON-RPC over TCP has no Noise or TLS layer by current design. A network observer reads all stamp content and calendar queries. | Deferred. The primary threat (content replay) is mitigated by echo fields and signature verification. Adding TLS is a separate infrastructure concern. |
| **Single-key compromise recovery** | If a node's in-memory private key is extracted (e.g., via memory dump), there is no revocation mechanism. All past and future signatures from that key remain valid. | v0.7 — Collision detection and identity revocation. |
| **DHT namespace collision across networks** | Two networks using the same `dht_namespace` would interleave their Kademlia tables, potentially allowing cross-network peer injection. | Addressed by operational guidance: testnet vs mainnet use distinct namespaces. Formal namespace governance is out of scope. |
| **Plaintext `.bak` file after migration** | The migration from plaintext to encrypted JSONL leaves a `.bak` copy of the old calendar on disk. | Operational. The `.bak` file is intended for one-time inspection. Future v0.5.x may add an auto-deletion timer. |
| **No rate limit on JSON-RPC connections** | The JSON-RPC server accepts unlimited concurrent TCP connections. A connection flood could exhaust file descriptors. | v0.6 — Connection limits as part of the observability and rate-limiting pass. |
| **Sybil identity without economic cost** | Creating new nodes has no cost. An attacker can create thousands of identities to influence consensus (once gossip adds voting). | v0.7 — Collision detection. Long-term, staking or reputation mechanisms may be added. |

---

## 6. Recommendations for v0.5 Code Changes

The threat analysis motivates three immediate code hardening items, all scoped to v0.5:

### 6.1 Enforce `MAX_PEERS` cap in `DhtPeerSource`

**File:** `p2p/fortias-node/src/communerd/dht_peer_source.rs`

**Problem:** `DhtPeerSource::upsert()` currently inserts any discovered peer without limit. A DHT eclipse attacker could fill the table with thousands of bogus peers, increasing memory and CPU.

**Change:**

```rust
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

This is idempotent for known peers (updating an existing entry does not count against the cap) and rejects unknown peers once the table is full.

### 6.2 Add `max_queue_depth` to Chronomatter job queue

**File:** `p2p/core-engine/src/chronomatter/mod.rs`

**Problem:** The stamp job queue has no upper bound. A stamp flood can queue unbounded work.

**Change:** Add `MAX_JOB_QUEUE_DEPTH` constant (suggested value: 64). When the bounded channel is full, reject with `NodeError::QueueFull` rather than blocking or dropping silently.

```rust
const MAX_JOB_QUEUE_DEPTH: usize = 64;

// In the job submission path (TimeFamily inbound handler):
if job_tx.capacity() == 0 {  // bounded channel; 0 remaining = full
    return Err(NodeError::QueueFull);
}
```

### 6.3 Validate `dht_namespace` configuration

**File:** `p2p/core-engine/src/config.rs`

**Problem:** An empty or excessively long `dht_namespace` could cause protocol errors in the Kademlia layer or make the node join an unintended network.

**Change:** Add validation to `NodeConfig::load()`:

```rust
fn validate(cfg: &NodeConfig) -> Result<(), ConfigError> {
    if cfg.dht_namespace.is_empty() || cfg.dht_namespace.len() > 64 {
        return Err(ConfigError::InvalidField("dht_namespace must be 1-64 bytes"));
    }
    Ok(())
}
```

This ensures the namespace is present and bounded, preventing accidental cross-network joins or Kademlia protocol errors from oversized keys.

---

## 7. Component-Specific Threat Summary

### 7.1 Communerd (`p2p/fortias-node/src/communerd/mod.rs`)

The P2P orchestrator. All external communication flows through this component. Key threats: address spoofing via identify (mitigated by signature verification), DHT table exhaustion (mitigated by `MAX_PEERS` cap in v0.5).

### 7.2 Chronomatter (`p2p/core-engine/src/chronomatter/mod.rs`)

The tick engine. Owns keypair generation, signing, and verification. Key threats: tick-rate exhaustion (mitigated by timer-driven daemon), stamp flood (mitigated by queue depth cap), private key exfiltration (mitigated by in-memory-only storage with zeroize).

### 7.3 CryptoServer (`p2p/core-engine/src/crypto_server/mod.rs` + `software.rs`)

The cryptographic abstraction layer. Key threats: key exfiltration (mitigated by `zeroize` on drop and no disk writes), RNG weakness (mitigated by `rng_mix.c` multi-source entropy).

### 7.4 JSON-RPC Handlers (`p2p/fortias-node/src/server/handlers.rs`)

The inbound request handlers. Key threats: oversized requests (mitigated by `MAX_CONTENT_BYTES`), content replay (mitigated by echo field), stamp flood (mitigated by rate limiting and queue cap).

### 7.5 DhtPeerSource (`p2p/fortias-node/src/communerd/dht_peer_source.rs`)

The DHT-backed peer discovery. Key threats: eclipse attack (mitigated by namespace key and bootstrap list), table exhaustion (mitigated by `MAX_PEERS` cap in v0.5).

### 7.6 Config (`p2p/core-engine/src/config.rs`)

Node configuration loading. Key threats: malformed namespace (mitigated by validation in v0.5), default values that may be insecure (reviewed per release).

---

## 8. Appendix: Trust Boundaries

```
+------------------+     +------------------+     +------------------+
|  JSON-RPC Client |---->|   Fortias Node   |---->|  Peer Node      |
|  (untrusted)     |     |  (this process)  |     |  (untrusted)    |
+------------------+     +------------------+     +------------------+
                           |       |       |
                    +-------+  +----+  +---+-------+
                    | Crypto | |Chrono| | Communerd |
                    | Server | |matter| |  + DHT    |
                    +--------+ +------+ +-----------+
                          ^        ^         ^
                    zeroize on  timer-     Noise-
                    drop, no   driven     encrypted
                    disk write daemon     transport
```

- **External clients** (JSON-RPC) are untrusted. All inputs are validated.
- **Peer nodes** are untrusted. All attestations are verified before storage.
- **CryptoServer** is trusted but minimized: no disk writes, keys zeroized on drop.
- **Chronomatter** is trusted for correct tick logic: timer-driven, not request-driven.
- **Communerd** is trusted for correct routing but relies on libp2p's Noise for transport security.
