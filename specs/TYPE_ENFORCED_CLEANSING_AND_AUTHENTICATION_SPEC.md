# TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_SPEC.md

**Date:** 2026-05-21
**Application:** Foretias v0.3+
**Source:** `specs/COMBINED_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md` §1.3.2, §1.5.C, §2.3.2-§2.3.6
**Paired Plan:** `specs/TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_PLAN.md`
**Merge Order:** This plan merges **AFTER** `HOW_SECRET_IS_SECURED_BY_SOFTWARE_PLAN.md` (secret-compliance-93283 branch).

---

## 1. Purpose

Enforce the **validate-before-trust** discipline at the type level so that the compiler refuses to compile any code path that consumes untrusted inbound data as if it were safe.

### 1.1 The Problem

Currently, inbound data from the network, DHT, gossip, and disk is parsed directly into trusted domain types. Verification happens *after* the type is already consumed downstream, or not at all. A handler receives a `ChrononRecord` and has no way to know — from the type alone — whether that record has been authenticated, sanitized, or is just raw bytes from an untrusted peer.

### 1.2 The Solution

This spec introduces a three-type progression: `Unprocessed<X>` → `CleanAuthenticated<X>` → `Externalized<X>`. The compiler enforces that data flows through this progression. You cannot skip the middle step.

### 1.3 What This Gives You

**For the code writer:** When your function signature accepts `CleanAuthenticated<ChrononRecord>`, you know — with compiler-enforced certainty — that:

- The record is **authenticated** to the TBID it claims to be from. The cryptographic signature was verified against the claimed signer's public key. It really came from that TBID.
- The record is **clean**. Structural invariants hold: field ranges are valid, formats are correct, bitwidths match expectations. Malformed data was rejected.
- The record is **sanitized**. No injection vectors, no trailing data, no ambiguous encodings slipped through.
- The record is **normalized**. Multiple encodings of the same logical value are collapsed to a single canonical form.

You don't need to check. You don't need to hope. The type system guarantees it.

**For the code reviewer:** The entire trust discipline collapses to five transition points (documented in §6.2). Review those five locations, and you've reviewed the entire trust boundary. You don't need to trace record flow through hundreds of functions.

**For formal analysis:** The type system provides a machine-checkable proof that untrusted data cannot reach trusted code paths. A static analyzer can verify the discipline by checking that `Unprocessed<X>` only appears at inbound boundaries and `CleanAuthenticated<X>` only appears after the due diligence gate.

**For the auditor:** Each transition point is a single, well-defined location in the codebase. An auditor can inspect the five gates and verify that due diligence is complete before any record enters the trusted domain.

### 1.4 What `CleanAuthenticated` Does NOT Mean

The name `CleanAuthenticated` is deliberately chosen to **downscope** from "verified" and avoid ambiguity:

- `CleanAuthenticated<X>` does **NOT** mean the record has been verified against the entire chronochain back to genesis. That would be "chain-verified" or "provenance-verified."
- `CleanAuthenticated<X>` does **NOT** mean the record is semantically correct or logically consistent with the broader protocol state.
- `CleanAuthenticated<X>` **DOES** mean: (a) the cryptographic signature matches the claimed TBID, and (b) the data is structurally clean (valid ranges, formats, bitwidths, normalized representation).

If you need chain-of-trust verification to genesis, that's a separate concern that builds on top of `CleanAuthenticated<X>`.

## 2. Design Principles

### 2.1 Three-Type Model

The spec enforces a **three-stage type progression** for inbound data:

| Type | Meaning | Contains |
|------|---------|----------|
| `Unprocessed<X>` | Structurally parsed, not yet verified | Raw domain data only |
| `CleanAuthenticated<X>` | TimeFamily due diligence complete — authenticated to claimed TBID, sanitized, validated, normalized; safe to consume in-process | Domain data + runtime context (crypto refs, clock, verification metadata) |
| `Externalized<X>` | Wire/disk format; stripped of all runtime information | Minimal persistent fields only |

The progression is unidirectional:

```
Wire bytes → Unprocessed<X> → CleanAuthenticated<X> → Externalized<X> → Disk/Wire
                              ↑
                              └── (re-verification on load)
```

**Why three types?**

1. `CleanAuthenticated<X>` may hold runtime context — references to `CryptoServer`, injected `Clock`, verification timestamps, or other in-process metadata. This context is essential for in-process reasoning but must never serialize.

2. `Externalized<X>` is the compiler-enforced "I've cleaned this for persistence." It contains only the fields necessary for reconstruction and re-verification on the receiving side. No runtime artifacts leak into storage or wire format.

3. The separation prevents two classes of bugs:
   - **Serialization of runtime state** (e.g., a `CleanAuthenticatedChrononRecord` accidentally serializing an internal verification timestamp that wasn't part of the original record)
   - **Storage bloat** (e.g., keeping a full `CleanAuthenticated<X>` in memory when only the minimal `Externalized<X>` is needed for disk persistence)

### 2.2 Single Concern per Type

- `Unprocessed<X>` — "I parsed this, but don't trust it yet."
- `CleanAuthenticated<X>` — "The TimeFamily has done due diligence: authenticated to the claimed TBID, sanitized, validated, normalized. Safe for in-process use."
- `Externalized<X>` — "I stripped this to its minimal persistent form."

Provenance metadata (`verifier_tbid`, `verified_at`, `source_tbid`) is **NOT** part of any of these three types. Provenance belongs at the storage layer (`MirrorEntry`, `CalendarStore`), not the verification or serialization layer.

### 2.3 New Module, Not Bindings

All Unprocessed/CleanAuthenticated/Externalized types are defined in a **new Rust module** (`p2p/core-engine/src/foretias/clean_auth.rs`). We never modify `bindings.rs` (auto-generated by bindgen; the secret-handling plan regenerates it in Phases 1-5).

### 2.4 Private Constructors — Single Gate to `CleanAuthenticated<X>`

The `CleanAuthenticated<X>` types have **private fields**. No code outside `clean_auth.rs` can construct them. The only paths to a `CleanAuthenticated<X>` are:

1. **`Unprocessed<X>::into_clean_authenticated()`** — the due diligence gate. Performs authentication, sanitization, validation, normalization. Returns `Result<CleanAuthenticated<X>, CleanAuthError>`. This is the only path for inbound data.

2. **`CleanAuthenticated<X>::from_trusted()`** — for locally-produced data (trusted by construction). Chronomatter-produced records use this path. The caller asserts the record was signed with our own key.

There is no `as` cast, no `unsafe` transmute, no `Deref` that bypasses the gate. There is no `impl Default`, no `new()` constructor, no public fields.

```rust
// Private fields — zero external construction
pub struct CleanAuthenticatedChrononRecord {
    inner: ChrononRecord,  // private
}

// Gate 1: inbound verification (only path for external data)
impl UnprocessedChrononRecord {
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        prev: &CleanAuthenticatedChrononRecord,
    ) -> Result<CleanAuthenticatedChrononRecord, CleanAuthError>;
}

// Gate 2: trusted construction (only path for local data)
impl CleanAuthenticatedChrononRecord {
    pub fn from_trusted(record: ChrononRecord) -> Self {
        Self { inner: record }
    }
}

// Read-only accessor
impl CleanAuthenticatedChrononRecord {
    pub fn inner(&self) -> &ChrononRecord {
        &self.inner
    }
}
```

The complete transition map:
- `Unprocessed<X>::into_clean_authenticated()` → `CleanAuthenticated<X>` (inbound gate)
- `CleanAuthenticated<X>::from_trusted()` → `CleanAuthenticated<X>` (local gate)
- `CleanAuthenticated<X>::externalize()` → `Externalized<X>` (serialization)
- `Externalized<X>::into_unprocessed()` → `Unprocessed<X>` (re-verification on load)

### 2.5 Verification Uses Existing Crypto Primitives

The `into_clean_authenticated()` methods call `CryptoServer` trait methods (`verify_ed25519`, `verify_with`, etc.). This spec does not add new cryptographic algorithms.

## 3. Type Triples

Each domain object has three wrapper types forming a complete lifecycle.

### 3.1 `UnprocessedChrononRecord` / `CleanAuthenticatedChrononRecord` / `ExternalizedChrononRecord`

**Inbound sources:** `handle_ship_ack`, `handle_stream_tick`, `get_calendar_slice` (communerd), DHT calendar replication, `.tmp` file recovery.

```rust
pub struct UnprocessedChrononRecord(pub ChrononRecord);

pub struct CleanAuthenticatedChrononRecord {
    pub inner: ChrononRecord,
    // Runtime context — never serialized:
    // (future: verification clock reference, crypto backend reference)
}

/// Minimal persistent form for disk/wire. Contains only fields needed
/// for reconstruction and re-verification on the receiving side.
#[derive(Serialize, Deserialize)]
pub struct ExternalizedChrononRecord {
    pub chronon_number: u64,
    pub public_key: [u8; 32],
    pub forward_foretis: Vec<u8>,
    pub backward_foretis: Vec<u8>,
    pub aa_nonce: [u8; 16],
    pub external_attestations: Vec<ExternalizedAttestation>,
    pub tb_version: u8,
    pub tbid: Vec<u8>,
    pub stamps_per_tick: u64,
}
```

**Construction:**
```rust
impl UnprocessedChrononRecord {
    /// Parse from wire bytes. No verification performed.
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError>;

    /// Parse from JSON-RPC params. No verification performed.
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError>;
}
```

**Verification:**
```rust
impl UnprocessedChrononRecord {
    /// Verify a non-genesis record against its predecessor.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        prev: &CleanAuthenticatedChrononRecord,
    ) -> Result<CleanAuthenticatedChrononRecord, VerifyError>;

    /// Verify a genesis record (tick 1). No predecessor required.
    pub fn into_clean_authenticated_genesis(
        self,
        crypto: &dyn CryptoServer,
    ) -> Result<CleanAuthenticatedChrononRecord, VerifyError>;
}
```

**Verification steps (non-genesis):**
1. Rebuild auto-attestation blob from `self` and `prev`.
2. Verify forward foretis signature against `prev`'s public key.
3. Verify backward foretis signature against `self`'s public key.
4. Verify hash chain continuity (forward hash matches backward hash).

**Verification steps (genesis):**
1. Verify forward foretis length >= `EXPECTED_GENESIS_FORETIS_MIN_LEN` (64 bytes for Ed25519; 49,920+ for SLH-DSA).
2. Split into Ed25519 component + genesis component (if `tb_version >= 1`).
3. Verify Ed25519 signature against the genesis public key.
4. If `tb_version >= 1`, verify SLH-DSA genesis signature.

### 3.2 `UnprocessedForetis` / `CleanAuthenticatedForetis` / `ExternalizedForetis`

**Inbound sources:** `handle_verify`, `cross_node_verify`, `stamp_peer` (communerd), `route_stamp` (communerd).

```rust
pub struct UnprocessedForetis(pub Foretis);

pub struct CleanAuthenticatedForetis {
    pub inner: Foretis,
    // Runtime context — never serialized:
    // (future: reference to the CleanAuthenticatedChrononRecord used for verification)
}

/// Minimal wire form for a Foretis.
#[derive(Serialize, Deserialize)]
pub struct ExternalizedForetis {
    pub chronon_number: u64,
    pub content_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub signature_algorithm: String,
    pub tbid: Vec<u8>,
    pub echo: Option<String>,
    pub tbn: u64,
}
```

**Construction:**
```rust
impl UnprocessedForetis {
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError>;
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError>;
}
```

**Verification:**
```rust
impl UnprocessedForetis {
    /// Verify against a calendar record.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        record: &CleanAuthenticatedChrononRecord,
    ) -> Result<CleanAuthenticatedForetis, VerifyError>;
}
```

**Verification steps:**
1. Rebuild `sig_input` from `tbid || chronon_number || content_hash`.
2. Check `self.signature_algorithm == record.signature_algorithm` (algorithm mismatch detection).
3. Verify signature against `record.public_key` using the indicated algorithm.
4. Check `self.chronon_number == record.chronon_number`.

**Externalization:**
```rust
impl CleanAuthenticatedForetis {
    /// Strip to minimal wire form. No runtime context leaks.
    pub fn externalize(self) -> ExternalizedForetis;
}

impl ExternalizedForetis {
    /// Reconstruct as Unprocessed for re-verification on the receiving side.
    pub fn into_unprocessed(self) -> Result<UnprocessedForetis, ParseError>;
}
```

### 3.3 `UnprocessedProbityReport` / `CleanAuthenticatedProbityReport` / `ExternalizedProbityReport`

**Inbound sources:** GossipSub messages (`gossip_handler.rs`).

```rust
pub struct UnprocessedProbityReport(pub ProbityReport);

pub struct CleanAuthenticatedProbityReport {
    pub inner: ProbityReport,
    // Runtime context — never serialized:
    // (future: reference to the resolved reporter identity)
}

/// Minimal gossip payload for probity reports.
#[derive(Serialize, Deserialize)]
pub struct ExternalizedProbityReport {
    pub reporter: Vec<u8>,    // reporter TBID
    pub subject: Vec<u8>,     // subject TBID
    pub target: Vec<u8>,      // target TBID
    pub score: i32,
    pub timestamp_ns: u64,
    pub signature: Vec<u8>,   // Ed25519 signature over canonical bytes
}
```

**Construction:**
```rust
impl UnprocessedProbityReport {
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError>;
}
```

**Verification:**
```rust
impl UnprocessedProbityReport {
    /// Verify Ed25519 signature against the reporter's public key.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        reporter_pub_key: &[u8; 32],
    ) -> Result<CleanAuthenticatedProbityReport, VerifyError>;
}
```

**Verification steps:**
1. Rebuild canonical signing bytes (length-prefixed: `len(subject) || subject || len(target) || target || score`).
2. Verify Ed25519 signature (`self.signature`) against `reporter_pub_key`.
3. Reject if `signature.len() != 64`.

**Reporter key resolution:** The `gossip_handler` must resolve the reporter's public key from the `reporter` TBID field. This requires a TBID-to-public-key lookup (from the local calendar or DHT). If the reporter is unknown, reject the report.

**Externalization:**
```rust
impl CleanAuthenticatedProbityReport {
    pub fn externalize(self) -> ExternalizedProbityReport;
}

impl ExternalizedProbityReport {
    pub fn into_unprocessed(self) -> Result<UnprocessedProbityReport, ParseError>;
}
```

### 3.4 `UnprocessedPeerRegistrationRecord` / `CleanAuthenticatedPeerRegistrationRecord` / `ExternalizedPeerRegistrationRecord`

**Inbound sources:** DHT `RecordRetrieved` events (`communerd/mod.rs`).

```rust
pub struct UnprocessedPeerRegistrationRecord(pub PeerRegistrationRecord);

pub struct CleanAuthenticatedPeerRegistrationRecord {
    pub inner: PeerRegistrationRecord,
    // Runtime context — never serialized:
    // (future: TTL, expiration timestamp, verification source)
}

/// Minimal DHT registration record.
#[derive(Serialize, Deserialize)]
pub struct ExternalizedPeerRegistrationRecord {
    pub peer_id: Vec<u8>,
    pub tbid: Vec<u8>,
    pub multiaddr: String,
    pub json_rpc: String,
    pub chronon_ns: u64,
    pub registered_at_ns: u64,
    pub capabilities: Vec<String>,
    pub signature: Vec<u8>,  // Ed25519 over canonical bytes
}
```

**Construction:**
```rust
impl UnprocessedPeerRegistrationRecord {
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError>;
}
```

**Verification:**
```rust
impl UnprocessedPeerRegistrationRecord {
    /// Verify that the TBID signing key produced the registration.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
    ) -> Result<CleanAuthenticatedPeerRegistrationRecord, VerifyError>;
}
```

**Verification steps:**
1. Rebuild canonical signing bytes: `peer_id_bytes || tbid_bytes || multiaddr_bytes || json_rpc_bytes`.
2. The `PeerRegistrationRecord` must include a `signature` field (Ed25519) over the canonical bytes.
3. Verify the signature against the Ed25519 public key embedded in the TBID (`tbid_ed25519_pub` from the TBID bytes).
4. Verify that `peer_id` matches the libp2p peer identity derivable from the signing public key.

**Schema change:** `PeerRegistrationRecord` gains a `signature: Vec<u8>` field. This is a wire-format addition, not a breaking change for existing entries (new entries carry signatures; old entries are rejected as unverifiable).

**Externalization:**
```rust
impl CleanAuthenticatedPeerRegistrationRecord {
    pub fn externalize(self) -> ExternalizedPeerRegistrationRecord;
}

impl ExternalizedPeerRegistrationRecord {
    pub fn into_unprocessed(self) -> Result<UnprocessedPeerRegistrationRecord, ParseError>;
}
```

### 3.5 `UnprocessedEpochSnapshot` / `CleanAuthenticatedEpochSnapshot` / `ExternalizedEpochSnapshot`

**Inbound sources:** `handle_verify_epoch_snapshot` (JSON-RPC).

```rust
pub struct UnprocessedEpochSnapshot(pub EpochSnapshot);

pub struct CleanAuthenticatedEpochSnapshot {
    pub inner: EpochSnapshot,
    // Runtime context — never serialized:
    // (future: reference to the committee used for verification)
}

/// Minimal epoch snapshot for wire transmission.
#[derive(Serialize, Deserialize)]
pub struct ExternalizedEpochSnapshot {
    pub epoch_number: u64,
    pub peer_scores: Vec<(Vec<u8>, i32)>,  // (TBID, score)
    pub committee: Vec<Vec<u8>>,            // TBIDs
    pub threshold: usize,
    pub frost_signature: Vec<u8>,
    pub committee_pubkey: Vec<u8>,
}
```

**Verification:**
```rust
impl UnprocessedEpochSnapshot {
    /// Verify FROST threshold signature.
    /// Returns VerifyError::NotYetImplemented until FROST ships.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        committee_pubkeys: &[SignatureBytes],
        threshold: usize,
    ) -> Result<CleanAuthenticatedEpochSnapshot, VerifyError>;
}
```

**Verification steps (when FROST is implemented):**
1. Verify FROST aggregate signature against committee public keys.
2. Verify threshold is met (at least `threshold` partial signatures contributed).

**Current behavior (FROST stubbed):** Return `VerifyError::NotYetImplemented`. This is honest about the current state and prevents any code from acting on a false positive.

**Externalization:**
```rust
impl CleanAuthenticatedEpochSnapshot {
    pub fn externalize(self) -> ExternalizedEpochSnapshot;
}

impl ExternalizedEpochSnapshot {
    pub fn into_unprocessed(self) -> Result<UnprocessedEpochSnapshot, ParseError>;
}
```

### 3.6 `ExternalizedChrononRecord` — Externalization for ChrononRecord

(Added to §3.1 above.)

```rust
impl CleanAuthenticatedChrononRecord {
    /// Strip to minimal persistent form. No runtime context leaks.
    pub fn externalize(self) -> ExternalizedChrononRecord;
}

impl ExternalizedChrononRecord {
    /// Reconstruct as Unprocessed for re-verification on load.
    pub fn into_unprocessed(self) -> Result<UnprocessedChrononRecord, ParseError>;
}
```

## 4. Error Type

A dedicated error enum for verification failures:

```rust
pub enum VerifyError {
    InvalidSignature,
    AlgorithmMismatch,
    ChainBreak,
    ReplayDetected,
    UnknownPeer,
    NotYetImplemented,
    ParseError(ParseError),
}

pub enum ParseError {
    InvalidJson(serde_json::Error),
    TruncatedBytes,
    InvalidLength,
}
```

`VerifyError` implements `From<ParseError>` so parse failures propagate naturally.

## 5. Handler Refactoring Rules

### 5.1 General Pattern

Every handler that accepts inbound data follows this pattern:

```rust
// 1. Parse to Unprocessed (fails on malformed input)
let unprocessed = UnprocessedFoo::from_bytes(&raw)?;

// 2. Clean & authenticate (fails on cryptographic failure)
let ca = unprocessed.into_clean_authenticated(&self.crypto, ...)?;

// 3. Consume clean-authenticated data (compiler guarantees this is safe)
self.store.insert(ca.inner())?;
```

### 5.2 Specific Handlers

| Handler | Current Type | New Type | Verification Context |
|---------|-------------|----------|---------------------|
| `handle_ship_ack` | `Vec<ChrononRecord>` | `Vec<UnprocessedChrononRecord>` | Batch verify: chain each record against its predecessor |
| `handle_stream_tick` | `ChrononRecord` | `UnprocessedChrononRecord` | Verify against latest verified record in mirror |
| `handle_verify` (local) | `Foretis` | `UnprocessedForetis` | Verify against local calendar record |
| `cross_node_verify` | `ChrononRecord` (from DHT) | `UnprocessedChrononRecord` | Verify chain-of-trust back to trust root |
| `handle_verify_epoch_snapshot` | `EpochSnapshot` | `UnprocessedEpochSnapshot` | FROST verify (or `NotYetImplemented`) |
| `gossip_handler::handle_gossip_message` | `ProbityReport` | `UnprocessedProbityReport` | Ed25519 verify against reporter's public key |
| `communerd::gossip_event_loop` (DHT) | `PeerRegistrationRecord` | `UnprocessedPeerRegistrationRecord` | TBID signature verify |
| `stamp_peer` / `route_stamp` | `Foretis` | `UnprocessedForetis` | Verify against local calendar |
| `get_calendar_slice` (remote) | `Vec<ChrononRecord>` | `Vec<UnprocessedChrononRecord>` | Chain verify returned slice |

### 5.3 Batch Verification Optimization

For `handle_ship_ack` with a batch of records, verify sequentially (each record depends on its predecessor):

```rust
let mut prev: Option<CleanAuthenticatedChrononRecord> = None;
for up in unprocessed_batch {
    let ca = match &prev {
        Some(p) => up.into_clean_authenticated(&self.crypto, p)?,
        None => up.into_clean_authenticated_genesis(&self.crypto)?,
    };
    mirror_store.insert_mirrored(ca.inner())?;
    prev = Some(ca);
}
```

If any record fails verification, the entire batch is rejected (atomic insert semantics).

## 6. Trust Boundaries and Type Transitions

This section documents where each type phase exists and how records transition between phases. The transitions are centralized at component boundaries, making the code easy to inspect, review, and audit.

### 6.1 Component Trust Boundaries

Each component operates exclusively within one type phase:

| Component | Type Phase | Role |
|-----------|------------|------|
| **Communerd** (inbound) | `Unprocessed<X>` → `CleanAuthenticated<X>` | Receives `Externalized<X>` from wire, parses to `Unprocessed<X>`, verifies to `CleanAuthenticated<X>`. Everything coming **out** of Communerd (inbound direction) is a `CleanAuthenticated<Record>`. Communerd performs its own signature verification on top of the P2P/PtP transport layer — above transport identification and authentication, but below full chain-of-trust to genesis. |
| **Communerd** (outbound) | `CleanAuthenticated<X>` → `Externalized<X>` | Receives `CleanAuthenticated<X>` from intra-family, externalizes to `Externalized<X>` for wire transmission. |
| **Intra-family** (Chronomatter ↔ Calendar ↔ Server) | `CleanAuthenticated<X>` only | Time beings within the same TimeFamilyServer communicate exclusively in `CleanAuthenticated<Record>`. No `Unprocessed<X>` crosses these boundaries. |
| **Calendar** (storage + load verification) | `Externalized<X>` on disk, `CleanAuthenticated<X>` in memory | Calendar stores `Externalized<X>` on disk. On load, reads `Externalized<X>`, converts to `Unprocessed<X>`, runs chain verification (`integrity_check`), and produces `CleanAuthenticated<X>`. Calendar is a verification gate — unverified data from disk never reaches intra-family callers. |
| **Chronomatter** (local production) | `CleanAuthenticated<X>` (trusted by construction) | Local stamps and ticks are trusted by construction — see §7. |

### 6.2 The Five Transitions

Each transition has a single, well-defined location in the codebase:

| # | Transition | Location | Direction |
|---|-----------|----------|-----------|
| T1 | `Externalized<X>` → `Unprocessed<X>` | Communerd inbound handlers, Calendar load | Wire/disk → parsed |
| T2 | `Unprocessed<X>` → `CleanAuthenticated<X>` | Communerd verification gate, Calendar load verification | Parsed → due diligence complete (authenticated, sanitized, validated, normalized) |
| T3 | `CleanAuthenticated<X>` → `Externalized<X>` | Calendar store, Communerd outbound | In-process → wire/disk |
| T4 | `CleanAuthenticated<X>` → intra-family call | Server → Chronomatter/Calendar method calls | Direct Rust calls |
| T5 | Local production → `CleanAuthenticated<X>` | Chronomatter `stamp()` / `daemon_tick()` | Trusted construction |

### 6.3 Complete Record Lifecycle

**Inbound record** (from a remote peer):
```
Wire bytes
  → (deserialize) Externalized<X>           [Communerd receives]
  → (into_unprocessed) Unprocessed<X>        [T1: parse, no trust]
  → (into_clean_authenticated) CleanAuthenticated<X>            [T2: signature verify — Communerd gate]
  → (intra-family call)                    [T4: to Calendar/Server]
  → (externalize) Externalized<X>           [T3: strip runtime context]
  → (persist) Disk                         [Calendar stores]
```

**Outbound record** (to a remote peer):
```
Local CleanAuthenticated<X> (from Chronomatter or mirror)
  → (externalize) Externalized<X>           [T3: strip runtime context]
  → (serialize) Wire bytes                 [Communerd sends]
```

**Calendar reload** (after restart):
```
Disk
  → (deserialize) Externalized<X>           [Calendar loads]
  → (into_unprocessed) Unprocessed<X>        [T1: parse, no trust]
  → (integrity_check) CleanAuthenticated<X>          [T2: chain verify]
  → (intra-family call)                    [T4: to Chronomatter/Server]
```

### 6.4 Audit Implications

Since each transition occurs at a single, well-defined location:

1. **T1** (parse): Audit the `into_unprocessed()` constructors in `clean_auth.rs`. Verify they reject malformed input.
2. **T2** (due diligence): Audit the `into_clean_authenticated()` methods in `clean_auth.rs` and the Communerd/Calendar handlers that invoke them. Verify: (a) authentication to claimed TBID, (b) sanitization of inbound data, (c) validation of structural invariants, (d) normalization of representations. Verify no `Unprocessed<X>` escapes without completing all due diligence steps.
3. **T3** (externalize): Audit the `externalize()` methods in `clean_auth.rs`. Verify no runtime context leaks.
4. **T4** (intra-family): Audit the method signatures between Server, Chronomatter, and Calendar. Verify they accept `CleanAuthenticated<X>`, never `Unprocessed<X>`.
5. **T5** (local production): Audit `Chronomatter::stamp()` and `daemon_tick()`. Verify the produced records are genuinely trusted by construction.

A code reviewer can verify the entire trust discipline by inspecting these five transition points, rather than tracing record flow through the entire codebase.

## 7. Storage Layer Rules

The storage pipeline is: `CleanAuthenticated<X>` → `Externalized<X>` → Disk/Wire.

**Calendar** stores only `Externalized<X>`. **Communerd** transmits only `Externalized<X>` over wire. Neither component touches `CleanAuthenticated<X>` directly for persistence or transmission — the externalization step (T3 above) strips all runtime context before the record leaves the in-process domain.

```rust
impl MirrorStore {
    /// Accepts an already-verified record, externalizes it for storage.
    pub fn insert_mirrored(&self, verified: CleanAuthenticatedChrononRecord) -> Result<(), StoreError> {
        let ext = verified.externalize();
        self.persist(ext)  // only Externalized<X> touches disk
    }

    /// Loads from disk, returns Unprocessed for re-verification.
    pub fn load(&self) -> Result<Vec<UnprocessedChrononRecord>, StoreError>;
}
```

Provenance tracking (`verified_by`, `verified_at`, `source_tbid`) belongs in a wrapper struct at the storage layer:

```rust
pub struct MirrorEntry {
    pub record: ExternalizedChrononRecord,  // stored form, not CleanAuthenticated
    pub verified_by: Tbid,
    pub verified_at: Timestamp,
    pub source_tbid: Tbid,
}
```

**Key invariant:** `CleanAuthenticated<X>` never touches disk or wire. Only `Externalized<X>` serializes.

## 8. Local Data (Trusted by Construction)

Local calendar records created by `stamp()` are trusted by construction — the server signs them with its own key. These do NOT need the Unprocessed/CleanAuthenticated distinction:

- `Chronomatter::stamp()` → produces `Foretis` (trusted)
- `Chronomatter::daemon_tick()` → produces `ChrononRecord` (trusted)
- `Calendar::append()` → accepts trusted records

The distinction applies only to **inbound** data from external sources.

## 9. Out of Scope

- **Secret material handling** — covered by `SECRET_TYPE_DISCIPLINE_SPEC.md` / `HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md`
- **FFI length validation** — covered by `FFI_LENGTH_VALIDATION_SPEC.md`
- **Clock injection** — covered by `CLOCK_INJECTION_SPEC.md`
- **FROST implementation** — covered by `FROST_IMPLEMENTATION_SPEC.md`
- **Canonical encoding** (length-prefix signing payloads) — covered by `CANONICAL_ENCODING_SPEC.md`
- **Provenance metadata** — belongs at the storage layer, not the type layer

## 9. Verification Checklist

Before this spec is considered implemented:

- [ ] All five type triples (Unprocessed/CleanAuthenticated/Externalized) defined in `clean_auth.rs`
- [ ] All nine handlers refactored to use Unprocessed types
- [ ] `VerifyError` enum defined and used
- [ ] No `unwrap()` in verification paths
- [ ] Batch verification is atomic (all-or-nothing insert)
- [ ] `bindings.rs` is NOT modified
- [ ] `cargo test --workspace` passes
- [ ] No regression in existing tests
