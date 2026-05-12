# FORETIAS-AUDIT SPECIFICATION

**Status**: Draft — Research Phase
**Prefix Pair**: `FORETIAS_5_AUDIT_SPEC.md` (will pair with `FORETIAS_5_AUDIT_PLAN.md`)
**Date**: 2026-05-12

---

## 0. Overview

The **foretis-audit** library provides tools for ordering, comparing, and analyzing **foretides** (the plural of *foretis*) stamped by different **TBIDs** (Time-Being IDs). It is designed as a **standalone analysis library** that operates independently of the core foretias runtime, suitable for use by auditors, legal investigators, compliance officers, and researchers who need to establish temporal priority across multiple independent foretis servers.

### The Core Problem

A single foretis calendar provides a **total order** within its own TBID: tick 0 < tick 1 < tick 2 < ... proven by the auto-attestation signature chain. But given two foretides from **different TBIDs**, no built-in comparison exists in the current codebase. This library fills that gap.

The complexity of cross-TBID ordering depends on:
1. **Mutual attestation topology** — whether the two TBIDs have directly or indirectly attested each other
2. **Chronon duration alignment** — whether the two TBIDs use the same tick interval
3. **Available evidence** — what calendar data, external attestations, and peer registrations are accessible

### Design Philosophy

- **Approximation over perfection** — A partial order is the best achievable result. The library provides confidence scores, not absolute guarantees, for incomparable pairs.
- **Two modes of operation** — Static analysis (offline) and Active exploration (online querying of the P2P network).
- **Language-agnostic access** — Rust core library with Python bindings (PyO3) for Jupyter notebook analysis, and potentially other language bindings following the existing foretias-p2p pattern.

---

## 1. Background: Existing Types & Invariants

*(This section documents what exists in the current codebase. foretis-audit depends on these types but does not modify them.)*

### 1.1 Foretis — The Stamped Attestation

**Location**: `p2p/core-engine/src/foretias/tick.rs`

```rust
pub struct Foretis {
    pub tick_number: u64,                       // Monotonic tick index (>0)
    pub content_hash: FTByteArray<32>,          // SHA-256 of attested content
    pub signature: FTByteVector,                // Variable-length sig (64 Ed25519, 49856 SLH-DSA)
    pub signature_algorithm: String,            // e.g. "Ed25519", "SPHINCS+-SHA2-256f-simple"
    pub tbid: Tbid,                             // 96-byte dual-key public key of signer
    pub echo: String,                           // Tick identifier string
    pub tbn: String,                            // TimeBeing name (human-readable)
    pub time_being_reference_time: String,      // "UE+<nanoseconds>ns" wall-clock format
}
```

**Current derives**: `Debug, Clone, Serialize, Deserialize` only.
**NO `PartialEq`, `PartialOrd`, or `Ord`** — this is the primary gap foretis-audit fills.

### 1.2 TickRecord — Calendar Entry

```rust
pub struct TickRecord {
    pub tick_number: u64,
    pub public_key: FTByteVector,
    pub signature_algorithm: String,
    pub forward_foretis: FTByteVector,          // Signed by PREVIOUS tick key
    pub backward_foretis: FTByteVector,         // Signed by CURRENT tick key
    pub aa_nonce: FTByteArray<16>,              // 16-byte replay nonce
    pub stamps_per_tick: u64,                   // Foretides issued during this chronon
    pub external_attestations: Vec<ExternalAttestation>,
    pub genesis_signature: FTByteVector,        // Dual-key sig on tick 1 only
    pub tb_version: u32,                        // 0=legacy, 1=dual-key TBID
}
```

**Current derives**: `Debug, Clone, Serialize, Deserialize` only.

### 1.3 ExternalAttestation — Cross-TBID Link

**Location**: `p2p/core-engine/src/foretias/external_attestation.rs`

```rust
pub struct ExternalAttestation {
    pub attester_tbid: String,                  // Hex-encoded TBID of attesting peer
    pub foretis: Foretis,                       // B's stamp of A's tick record
    pub attester_tick_record: TickRecord,       // B's tick at attestation time
    pub received_at_ns: u64,                    // Wall-clock receive time (nanoseconds)
}
```

This is the **primary data structure for cross-TBID ordering**. When node B attests node A:
- B stamps A's tick record, producing a `Foretis` (B's tick number, B's TBID)
- A stores this as `ExternalAttestation` in A's `TickRecord.external_attestations`
- The `attester_tick_record` lets us re-verify offline without contacting B

**What it provides**: "A.tick X existed when B.tick Y was active."
**What it does NOT provide**: Total ordering. Only one constraint pair per attestation.

### 1.4 TBID — Time-Being Identity

```rust
pub struct Tbid {
    pub inner: FTByteArray<96>,   // Ed25519_PK(32) || SLH-DSA_PK(64)
}
```

**Derives**: `PartialEq, Eq, Hash` — equality only, no ordering.

### 1.5 TickNumber — The Only Ord Type

```rust
pub struct TickNumber(pub u64);
```

**Derives**: `PartialEq, Eq, PartialOrd, Ord, Hash` — full ordering via numeric comparison.

**Critical resolution** (from `specs/questions.md` Q#6): `tick_number` is a **sequential counter** (0, 1, 2, ...), NOT wall-clock nanoseconds. Wall-clock reference time lives in the separate `time_being_reference_time` field.

### 1.6 Chronon Duration

`chronon_ns` is a configuration property of each `Chronomatter`, NOT stored in `Foretis` or `TickRecord`. Discoverable via:
- `PeerRegistrationRecord.chronon_ns` in the DHT
- Default: `60_000_000_000` ns (60 seconds)

Different TBIDs can have different chronon durations. This is why tick_number comparison across TBIDs is meaningless without knowing each party's chronon.

### 1.7 Intra-TBID Ordering (Already Solved)

Within a single TBID:
- **Total order** via auto-attestation chain
- Each tick n → n+1: forward_foretis (old key signs new key) + backward_foretis (new key signs old key)
- `Calendar::append` enforces strictly increasing tick_number
- `Calendar::integrity_check(start, end)` verifies `verify_pair(prev, curr)` for consecutive ticks

### 1.8 Cross-TBID Ordering (The Gap)

**No built-in comparison exists.** The current codebase:
- Has zero `PartialOrd`/`Ord` implementations on `Foretis`, `TickRecord`, `Calendar`, or `ExternalAttestation`
- Has a single `sort_by_key(|r| r.tick_number)` in `MirrorStore::insert_mirrored()` — single-TBID only
- Has `time_being_reference_time` as a raw string with no parsing or comparison function
- Has Merkle tree primitives in C11 that are unused in the domain logic

---

## 2. Ordering Cases — From Strongest to Weakest

The foretis-audit library must handle five distinct ordering cases, each with different confidence levels:

### Case 1: Same TBID, Different Tick Numbers
- **Strength**: **Strong** (cryptographic total order)
- **Method**: Compare `tick_number` directly. The auto-attestation chain proves `tick_a < tick_b`.
- **Confidence**: 100% (cryptographically proven)
- **Algorithm**: Trivial `tick_number` comparison. Verify chain integrity via `verify_pair()` if needed.

### Case 2: Same TBID, Same Tick Number (Multiple Foretides)
- **Strength**: **None** (unordered)
- **Reason**: Multiple stamps within one chronon share the same tick_number and key. No sub-chronon ordering signal exists.
- **Confidence**: 0% (concurrent by definition)
- **Partial signal**: `time_being_reference_time` provides wall-clock approximation but is NOT trusted.
- **Algorithm**: Report as concurrent. Optionally sort by wall-clock reference time with explicit disclaimer.

### Case 3: Different TBID, Directly Mutually Attested
- **Strength**: **Moderate** (partial order from direct evidence)
- **Data**: An `ExternalAttestation` on A's tick X records that B stamped A at B's tick Y.
- **What we know**: "A.tick X existed during B's chronon Y." This gives us an **interval constraint**, not a point.
- **Chronon window**: B's chronon Y spans `[B_start_Y, B_start_Y + B_chronon_ns)`. A's tick X similarly spans `[A_start_X, A_start_X + A_chronon_ns)`. The intersection of these windows is the ordering constraint.
- **Confidence**: Depends on chronon granularity. Tighter chronons → tighter bounds → higher confidence.
- **Algorithm**: 
  1. Extract constraint pairs from `ExternalAttestation` records
  2. Convert tick numbers to chronon intervals using each party's `chronon_ns`
  3. Apply Allen's Interval Algebra relations to determine overlap/precedence
  4. Build a partial order (poset) from all constraint pairs

### Case 4: Different TBID, Connected Through Shared Attester
- **Strength**: **Weak** (transitive partial order)
- **Scenario**: A is attested by C, and B is also attested by C. We know "A existed during C's chronon X" and "B existed during C's chronon Y."
- **What we know**: If X < Y, then A preceded B (relative to C's timeline). If X and Y overlap, A and B are concurrent.
- **Limitation**: No transitivity guarantee. A attested by C and B attested by C does NOT mean A attested B.
- **Confidence**: Scales with the quality of the shared attestor C (see Section 5 — Defensibility Scoring).
- **Algorithm**:
  1. Build attestation graph: nodes = TBIDs, edges = mutual attestations
  2. For each pair (A, B), find all shared attestors
  3. For each shared attestor C, compare C's chronons when attesting A vs B
  4. Aggregate across all shared attestors to compute a confidence-weighted ordering

### Case 5: Different TBID, No Attestation Connection
- **Strength**: **None** (no reliable ordering)
- **Available signals**:
  - `time_being_reference_time` — wall-clock approximation (untrusted, but usable as heuristic)
  - `received_at_ns` in ExternalAttestation — wall-clock receive time (also untrusted as ordering evidence)
- **Confidence**: 0% for cryptographic ordering. Can provide wall-clock-based approximation with explicit uncertainty disclaimer.
- **Algorithm**: 
  1. Parse `time_being_reference_time` from `"UE+<ns>ns"` format to u64
  2. Compare wall-clock timestamps as best-effort heuristic
  3. Report result as "concurrent — no attestation connection" with wall-clock estimate

---

## 3. Library Architecture

### 3.1 Crate Structure

The foretis-audit library is organized as two crates following the existing foretias-p2p pattern:

```
p2p/
├── audit-core/                    # Pure Rust analysis library (no PyO3)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 # Public API re-exports
│       ├── store.rs               # AuditStore — shared data layer (see §3.2)
│       ├── poset/                 # Partial order construction & manipulation
│       │   ├── mod.rs
│       │   ├── build.rs           # Build poset from ExternalAttestation records
│       │   ├── merge.rs           # Merge multiple calendars' posets
│       │   ├── hasse.rs           # Transitive reduction → Hasse diagram
│       │   └── concurrent.rs      # Detect incomparable/concurrent pairs
│       ├── temporal/              # Interval-based temporal reasoning
│       │   ├── mod.rs
│       │   ├── interval.rs        # Chronon interval type + Allen's relations
│       │   ├── intersection.rs    # Cross-calendar chronon alignment
│       │   └── granularity.rs     # Canonical chronon conversion
│       ├── defensibility/         # Evidence strength analysis
│       │   ├── mod.rs
│       │   ├── centrality.rs      # Graph centrality measures
│       │   ├── trust.rs           # EigenTrust propagation
│       │   ├── witness.rs         # Common witness detection
│       │   └── score.rs           # Composite defensibility score
│       ├── statistics/            # Statistical analysis
│       │   ├── mod.rs
│       │   ├── anomaly.rs         # Inter-event time anomaly detection
│       │   ├── regularity.rs      # Temporal regularity scoring
│       │   └── distribution.rs    # Chronon duration distribution analysis
│       ├── comparison/            # Core comparison primitives
│       │   ├── mod.rs
│       │   ├── foretis_cmp.rs     # Foretis-to-Foretis comparator
│       │   ├── tick_cmp.rs        # TickRecord-to-TickRecord comparator
│       │   └── wallclock.rs       # Parse "UE+<ns>ns" → u64
│       ├── active/                # Active exploration (network queries)
│       │   ├── mod.rs
│       │   ├── connector.rs       # connect_server (JSON-RPC), connect_p2p (libp2p)
│       │   ├── queries.rs         # fetch_calendar_slice, discover_peers, fetch_identity
│       │   └── strategy.rs        # QueryStrategy — what to query next given gaps
│       └── report/                # Audit report generation
│           ├── mod.rs
│           ├── finding.rs         # Individual finding type
│           └── summary.rs         # Aggregate summary
│
├── foretias-audit-python/         # PyO3 Python bindings
│   ├── Cargo.toml                 # cdylib, depends on audit-core
│   ├── pyproject.toml             # maturin build → "foretias_audit" module
│   └── src/lib.rs                 # PyAuditEngine, PyAuditReport, etc.
```

**Workspace**: Add both `audit-core` and `foretias-audit-python` to `p2p/Cargo.toml` members.

**Python shim** (optional): `src/foretias_audit/__init__.py` — re-exports from `foretias_audit` with clean names, following the existing `src/foretias/__init__.py` pattern.

### 3.1.1 AuditStore — The Shared Data Layer

Both static and active modes share a single `AuditStore` data structure. Active mode inserts/updates data; static mode reads and analyzes it. This eliminates cross-mode type duplication and enables a clean feedback loop.

```rust
pub struct AuditStore {
    /// Foretides keyed by (tbid_hex, content_hash)
    foretides: HashMap<(String, [u8; 32]), Foretis>,

    /// Tick records keyed by (tbid_hex, tick_number)
    calendar: HashMap<(String, u64), TickRecord>,

    /// TBID identity info discovered via active queries
    identities: HashMap<String, TbidIdentity>,

    /// Peer relationships (tbid → set of peer tbids)
    peer_graph: HashMap<String, HashSet<String>>,

    /// Provenance: where each datum originated
    sources: HashMap<(String, u64), DataProvenance>,
}

pub struct TbidIdentity {
    pub tbid: String,
    pub tbn: String,
    pub chronon_ns: u64,
    pub signature_algorithm: String,
    pub first_tick: u64,
    pub server_addr: Option<String>,
}

pub enum DataProvenance {
    LocalFile(String),    // file path
    ServerRpc(String),    // server address
    P2pGossip,
    DhtLookup(String),    // peer_id
}
```

### 3.1.2 Active → Static Feedback Loop

The two modes interact through a repeatable enrichment cycle:

```
1. StaticAnalyzer.load_*()        → populates AuditStore with initial data
2. StaticAnalyzer.build_partial_order() → identifies gaps (concurrent pairs)
3. ActiveAuditor.plan_queries(gaps)     → strategy: which server holds bridging data?
4. ActiveAuditor.execute_plan()         → returns enriched AuditStore delta
5. StaticAnalyzer.merge(delta)          → re-builds partial order with new data
6. If gaps remain and budget allows: go to step 3
   (repeat until gaps resolved OR query budget exhausted)
7. StaticAnalyzer.generate_report()     → final AuditReport with provenance
```

Network failures in active mode produce empty deltas (not errors). Static analysis proceeds on whatever data was already available — degraded-mode by default.

### 3.2 Two Modes of Operation

#### Mode 1: Static Analysis (Offline)

The auditor has already downloaded foretides, calendar slices, and attestation data. Analysis is performed entirely on local data.

**Inputs**:
- Foretis objects (JSON, deserialized)
- Calendar slices (TickRecord sequences, JSON)
- External attestation records
- Peer registration records (for chronon duration lookup)

**Operations**:
1. **Load & Verify** — Deserialize and cryptographically verify each foretis and tick record
2. **Build Poset** — Construct a partial order from available ExternalAttestation records
3. **Identify Gaps** — Find concurrent/incomparable pairs
4. **Score Defensibility** — Compute evidence strength per party
5. **Detect Anomalies** — Statistical analysis of timestamp distributions
6. **Generate Report** — Timeline visualization, findings, confidence scores

**API Sketch** (Rust):
```rust
let engine = AuditEngine::new();
engine.load_foretides(foretides_json);
engine.load_calendars(calendars_json);
engine.load_peer_registry(peer_records);

// Verify all loaded data
let verification = engine.verify_all()?;

// Build cross-TBID partial order
let poset = engine.build_partial_order()?;

// Find concurrent pairs (no ordering constraint)
let concurrent_pairs = poset.concurrent_pairs();

// Score defensibility per TBID
let scores = engine.defensibility_scores()?;

// Generate full audit report
let report = engine.generate_report(ReportOptions::default())?;
```

#### Mode 2: Active Exploration (Online)

The audit library connects to live foretis-servers (via JSON-RPC, HTTP, or P2P protocols) to discover additional data and resolve ordering ambiguities.

**Inputs**:
- A set of target TBIDs to investigate
- Network connection parameters (server addresses, multiaddrs)

**Operations**:
1. **Connect** — Establish connections to foretis-servers
2. **Discover** — Query DHT for peer registrations (chronon duration, capabilities)
3. **Fetch** — Retrieve calendar slices, external attestations
4. **Resolve** — Use new data to tighten ordering constraints
5. **Iterate** — Follow new leads (discover additional peers, attesters)

**API Sketch** (Rust):
```rust
let explorer = ActiveExplorer::new();

// Connect to servers
explorer.add_server("127.0.0.1:4001")?;
explorer.add_multiaddr("/ip4/10.0.0.1/tcp/4001/p2p/...")?;

// Discover peers and their configurations
let peers = explorer.discover_peers(target_tbid).await?;

// Fetch calendar data
let calendar_slice = explorer.fetch_calendar_slice(
    target_tbid, start_tick, end_tick
).await?;

// Enrich static analysis with new data
engine.load_calendars(calendar_slice);

// Find shared attestors between two parties
let shared_witnesses = explorer.find_shared_attestors(
    party_a_tbid, party_b_tbid, time_window
).await?;

// Resolve a specific concurrent pair
let resolution = explorer.resolve_concurrent_pair(
    foretis_a, foretis_b
).await?;
```

**Data Flow**: Active mode discovers data → feeds into static engine → static engine produces report. The two modes share the same underlying data structures; active mode is essentially a data enrichment layer.

### 3.3 Python Binding Architecture

Follows the existing foretias-python 6-part template:
1. `#[pyclass]` wrapper struct (PyAuditEngine, PyAuditReport, etc.)
2. `From<&InnerType>` conversion
3. `#[pymethods]` with `new()`, `from_json()`, `to_json()`, `__repr__()`
4. Error handling via `PyErr` (PyValueError, PyRuntimeError)
5. Stateful classes with `Arc` + `parking_lot::Mutex`
6. Module registration in `#[pymodule]`

**Python Jupyter Usage**:
```python
import foretias_audit

# Load foretides from files or API
engine = foretias_audit.AuditEngine()
engine.load_foretides_from_json("party_a_foretides.json")
engine.load_foretides_from_json("party_b_foretides.json")

# Build partial order
poset = engine.build_partial_order()

# Visualize (returns a polars DataFrame or dict)
timeline = engine.timeline_dataframe()
attestation_graph = engine.attestation_graph()

# Get findings
findings = engine.findings()
for f in findings:
    print(f"{f.severity}: {f.description}")

# Defensibility scores
scores = engine.defensibility_scores()
```

---

## 4. Core Data Structures

### 4.1 ForetideRef — Auditable Event Reference

A lightweight reference to a specific foretide, used as a node in the poset:

```rust
pub struct ForetideRef {
    pub tbid_hex: String,             // Hex-encoded TBID
    pub tick_number: u64,             // Tick number within that TBID
    pub content_hash: [u8; 32],       // SHA-256 of the stamped content
    pub wallclock_ns: Option<u64>,    // Parsed from time_being_reference_time
    pub source: EvidenceSource,       // How this foretide was obtained
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvidenceSource {
    /// Directly stamped by the party
    Direct,
    /// Discovered via mutual attestation from another party
    MutuallyAttested(String),  // attester_tbid_hex
    /// Discovered via shared attestor
    TransitiveWitness(String), // witness_tbid_hex
    /// Discovered via active exploration
    ActivelyFetched,
}
```

### 4.2 ChrononInterval — Time Window for a Tick

```rust
pub struct ChrononInterval {
    pub tbid_hex: String,
    pub tick_number: u64,
    pub start_ns: u64,       // Earliest possible wall-clock time
    pub end_ns: u64,         // Latest possible wall-clock time
    pub duration_ns: u64,    // chronon_ns for this TBID
    pub confidence: f64,     // How precisely we know this interval [0.0, 1.0]
}
```

**Allen's Interval Algebra relations** between two ChrononIntervals:
- `Before` — interval A ends before interval B starts
- `Meets` — interval A ends exactly when B starts
- `Overlaps` — intervals partially overlap
- `During` — interval A is entirely within interval B
- `Starts` — same start, A ends before B
- `Finishes` — same end, A starts after B
- `Equals` — identical intervals

### 4.3 OrderingResult — Comparison Outcome

```rust
pub enum OrderingResult {
    /// Cryptographically proven A before B
    Before { confidence: f64, evidence: Vec<EvidenceChain> },
    /// Cryptographically proven A after B
    After { confidence: f64, evidence: Vec<EvidenceChain> },
    /// Concurrent — no ordering constraint, but wall-clock estimate available
    Concurrent { 
        wallclock_estimate: Option<WallclockOrder>,
        reason: ConcurrentReason,
    },
    /// Unknown — insufficient data
    Unknown { missing: Vec<MissingData> },
}

pub enum WallclockOrder {
    ABeforeB(u64),  // nanosecond gap
    BBeforeA(u64),
    Simultaneous,   // within same nanosecond
}

pub enum ConcurrentReason {
    SameTickMultipleStamps,
    OverlappingChronons { overlap_ns: u64 },
    NoAttestationConnection,
    InsufficientEvidence,
}

pub struct EvidenceChain {
    pub path: Vec<String>,           // TBID hex chain: A → C → B
    pub constraint_type: ConstraintType,
    pub strength: f64,              // Combined confidence
}

pub enum ConstraintType {
    DirectMutualAttestation,
    TransitiveThroughWitness,
    WallclockHeuristic,
}
```

### 4.4 DefensibilityScore — Evidence Strength per Party

```rust
pub struct DefensibilityScore {
    pub tbid_hex: String,
    pub overall_score: f64,           // Composite score [0.0, 1.0]
    
    /// Number of external parties that directly attested this party
    pub direct_witnesses: usize,
    
    /// Number of parties reachable through transitive attestation
    pub transitive_reach: usize,
    
    /// Graph centrality measures
    pub degree_centrality: f64,
    pub betweenness_centrality: f64,
    pub eigenvector_centrality: f64,
    
    /// Temporal regularity (higher = more consistent chronons)
    pub temporal_regularity: f64,
    
    /// Coverage: fraction of the investigation window covered by attestations
    pub attestation_coverage: f64,
    
    /// Anomaly flags
    pub anomalies: Vec<AnomalyFlag>,
}

pub enum AnomalyFlag {
    /// Chronons are suspiciously regular (possible bot)
    SuspiciouslyRegularChronons { variance: f64 },
    /// Large gaps in attestation coverage
    AttestationGap { start_ns: u64, end_ns: u64 },
    /// Attested by parties with low defensibility scores
    LowQualityWitnesses { count: usize },
    /// Chronon duration varies significantly
    VariableChrononDuration { min_ns: u64, max_ns: u64 },
}
```

### 4.5 AuditReport — The Final Output

```rust
pub struct AuditReport {
    pub generated_at: String,           // ISO 8601 timestamp
    pub investigation_window: InvestigationWindow,
    
    /// Summary statistics
    pub total_foretides: usize,
    pub total_tbids: usize,
    pub total_attestations: usize,
    
    /// Ordering results
    pub poset: PosetSummary,
    pub concurrent_pairs: Vec<ConcurrentPair>,
    pub resolved_pairs: Vec<ResolvedPair>,
    
    /// Defensibility analysis
    pub defensibility_scores: HashMap<String, DefensibilityScore>,
    
    /// Anomaly detection
    pub anomalies: Vec<AnomalyReport>,
    
    /// Key findings (human-readable)
    pub findings: Vec<AuditFinding>,
}

pub struct AuditFinding {
    pub severity: FindingSeverity,
    pub category: FindingCategory,
    pub description: String,
    pub evidence: Vec<EvidenceReference>,
}
```

---

## 5. Algorithms & Analysis Pipeline

### 5.1 Poset Construction (Cross-TBID Ordering)

**Input**: A set of `ExternalAttestation` records linking multiple calendars.
**Output**: A DAG where nodes are `ForetideRef` and edges represent "before" constraints.

**Algorithm**:
1. For each `ExternalAttestation` on A's tick X:
   - Extract: attester B, B's tick Y, B's chronon duration
   - Compute B's chronon interval Y: `[B_start_Y, B_start_Y + B_chronon_ns)`
   - Compute A's chronon interval X: `[A_start_X, A_start_X + A_chronon_ns)`
   - Determine Allen relation between the two intervals
   - If `Before`: add edge A.tick X → B.tick Y (A before B)
   - If `Overlaps`: mark as concurrent (no edge)
   - If `During`: add edge with reduced confidence (A during B's chronon)

2. Compute transitive closure of the DAG (BFS from each node).

3. For transitive ordering (Case 4): for each pair (A, B) with no direct edge:
   - Find all shared attestors C
   - For each C, compare C's chronons when attesting A vs B
   - If consistent across all shared attestors: add edge with confidence proportional to witness count and quality

**Complexity**: O(V · (V + E)) for BFS-based transitive closure. Acceptable for audit-scale data (see Section 6).

**Rust Implementation**: Build on `petgraph::stable_graph::StableDiGraph` for mutable graph with persistent node IDs. Use `petgraph::algo::toposort` for topological enumeration. Use `hasse` crate (or custom implementation) for transitive reduction.

### 5.2 Defensibility Scoring

**Input**: Attestation graph (nodes = TBIDs, edges = mutual attestations).
**Output**: A `DefensibilityScore` per TBID.

**Algorithm**:
1. **Degree Centrality** — in-degree = number of direct witnesses, out-degree = number of parties attested
2. **Betweenness Centrality** — Brandes algorithm, measures bridge value in the attestation network
3. **Eigenvector Centrality** — importance weighted by neighbors' importance (EigenTrust propagation)
4. **Temporal Regularity** — coefficient of variation of inter-tick intervals (low variance = regular, high variance = irregular). Flag if variance is suspiciously low (possible bot).
5. **Attestation Coverage** — fraction of the investigation window where the party has at least one external attestation
6. **Composite Score** — weighted combination:
   ```
   overall = 0.30 * normalize(degree_centrality)
           + 0.20 * normalize(eigenvector_centrality)
           + 0.20 * normalize(coverage)
           + 0.15 * temporal_regularity
           + 0.15 * normalize(betweenness_centrality)
   ```

**Rust Implementation**: Centrality measures on `petgraph` (betweenness via Brandes, eigenvector via power iteration). Regularity via `ndarray-stats` or `u_analytics`.

### 5.3 Statistical Anomaly Detection

**Input**: Time series of tick events for each TBID.
**Output**: Anomaly flags per TBID.

**Algorithms**:
1. **Inter-Event Time Analysis** — compute distribution of time gaps between consecutive ticks, test against expected Poisson/normal distribution
2. **Modified Z-Score** (Iglewicz) — robust outlier detection on tick timing
3. **Goodness-of-Fit** — Kolmogorov-Smirnov test: observed tick distribution vs expected regular chronon distribution
4. **Cross-Party Comparison** — compare chronon duration distributions across all parties to identify outliers

**Rust Implementation**: `rs-stats` for KS test and distributions, `ndarray-stats` for summary statistics.

### 5.4 Active Exploration Algorithms

**Input**: Target TBIDs, network connections.
**Output**: Enriched dataset for static analysis.

**Algorithms**:
1. **Peer Discovery** — DHT lookup by TBID hex → multiaddr → server connection
2. **Calendar Fetch** — RPC call to `get_calendar_slice(tbid, start, end)` → TickRecord batch
3. **Shared Attestor Search** — BFS through attestation graph to find common witnesses between two parties
4. **Resolution Iteration** — for each concurrent pair, attempt to find a shared attestor that can establish ordering

**Rust Implementation**: `reqwest` for HTTP/JSON-RPC, optional `libp2p` integration for P2P discovery (reusing foretias-node's communerd patterns).

---

## 6. Concrete Use Case: Patent Law Firm Scenario

### 6.1 Scenario Description

A firm of 100 patent lawyers, each operating their own foretis-server (own TBID). Lawyers stamp communications, documents, and meeting notes as they work on cases. A conflict of interest arises between two clients — which side solidified a concept first?

The audit team needs to:
1. Gather stamped evidence from relevant lawyers
2. Establish temporal ordering of key events
3. Determine which party had priority (first to solidify the concept)
4. Assess the defensibility of each party's evidence

### 6.2 Data Volume Estimates

Based on serialized JSON sizes from actual struct definitions:

| Component | Size |
|---|---|
| TickRecord (no ext attestations) | ~1,130 bytes |
| Per ExternalAttestation (nested Foretis + TickRecord) | ~2,300 bytes |
| Ticks/year/lawyer (60s chronon) | 525,600 |

**Per Lawyer** (calendar storage):

| Peer attestation count (K) | Bytes/tick | Calendar/year |
|---|---|---|
| 0 (isolated) | 1,130 | ~594 MB |
| 2 | 4,558 | ~2.4 GB |
| 5 (typical mesh) | 9,700 | ~5.1 GB |
| 10 (dense mesh) | 19,980 | ~10.5 GB |

**Across 100 Lawyers** (1 year, K=5 average):

| Component | Total |
|---|---|
| Calendar storage | ~510 GB |
| User-stamped foretides (separate) | ~580 MB (negligible) |
| Stamps/lawyer/year | 1,825–18,250 |

**For a Specific Investigation** (2–5 relevant lawyers, 3–6 month window):

| Component | Count | Size |
|---|---|---|
| Relevant foretides (stamps) | 2,000–14,000 | ~7–56 MB |
| Calendar ticks (full) | 155K–778K | ~175 MB–870 MB |
| External attestations (K=5) | 775K–3.9M | ~1.8–9 GB |

**Key insight**: External attestations dominate calendar storage (93%+). The audit library must filter by TBID and time range aggressively — loading entire calendars is impractical at scale.

**Performance Budget**:
- Poset construction: O(V · (V + E)) where V is tick count in scope, E is attestation edge count
  - Worst case (778K ticks × 3.9M attestations) — needs optimization
  - Mitigation: filter to relevant time window, prune irrelevant ticks, process per-party
  - Realistic filtered scope: 20K ticks × 100K attestations = ~2 billion — requires pruning
  - After pruning to key events: 2K ticks × 10K attestations = ~20 million — fast
- Defensibility scoring: O(V · E) for Brandes betweenness on TBID-level graph (not tick-level)
  - 20 TBIDs × 100 attestation edges = ~2,000 operations — trivial
- Statistical analysis: O(n) per TBID — trivial at these scales

### 6.3 Analysis Flow for the Scenario

1. **Data Collection** (Active Mode):
   - Connect to relevant lawyers' foretis-servers
   - Fetch calendar slices for the investigation window
   - Discover peer attestations (who attested whom)
   - Fetch external attestation records

2. **Verification** (Static Mode):
   - Verify each foretis signature against its calendar
   - Verify calendar chain integrity (verify_pair for consecutive ticks)
   - Verify external attestation signatures

3. **Ordering** (Static Mode):
   - Build partial order from external attestations
   - Identify concurrent pairs (no ordering constraint)
   - Attempt resolution via shared attestors (Active Mode)

4. **Scoring** (Static Mode):
   - Compute defensibility score per party
   - Identify anomalies in timestamp patterns
   - Flag parties with weak evidence (few external attestations)

5. **Report** (Static Mode):
   - Timeline visualization of key events
   - Attestation graph showing evidence relationships
   - Finding: "Party A's concept X at tick 42 was preceded by Party B's concept Y at tick 38, with 0.87 confidence via shared attestor C"
   - Finding: "Party B's evidence is more defensible (score 0.82) than Party A's (score 0.41) due to broader external attestation coverage"

### 6.4 Defensibility Example

Party A: Mutually attested by 3 well-known external parties during the relevant period.
Party B: No external attestations.

**Result**: Party A's evidence is significantly more defensible because:
- External witnesses corroborate A's timeline independently
- Even if A's server were compromised, the 3 external attestations provide independent verification
- Party B's timeline is self-reported with no external corroboration

**Quantified**: A's defensibility score ~0.85 (multiple witnesses, good coverage). B's score ~0.15 (no external witnesses, self-reported only).

---

## 7. API Surface (Rust)

### 7.1 AuditEngine — Main Entry Point

```rust
pub struct AuditEngine {
    // Internal state: loaded foretides, calendars, peer registry, computed poset
}

impl AuditEngine {
    /// Create a new empty audit engine
    pub fn new() -> Self;
    
    /// Load foretides from a slice of Foretis objects
    pub fn load_foretides(&mut self, foretides: &[Foretis]) -> Result<usize, AuditError>;
    
    /// Load foretides from JSON string
    pub fn load_foretides_from_json(&mut self, json: &str) -> Result<usize, AuditError>;
    
    /// Load calendar data (TickRecord sequences)
    pub fn load_calendars(&mut self, calendars: &[Calendar]) -> Result<usize, AuditError>;
    
    /// Load peer registration records (for chronon duration lookup)
    pub fn load_peer_registry(&mut self, records: &[PeerRegistrationRecord]) -> Result<(), AuditError>;
    
    /// Cryptographically verify all loaded data
    pub fn verify_all(&self) -> Result<VerificationReport, AuditError>;
    
    /// Build cross-TBID partial order from available attestations
    pub fn build_partial_order(&self) -> Result<CrossTbidPoset, AuditError>;
    
    /// Compare two foretides and return ordering result
    pub fn compare_foretides(&self, a: &Foretis, b: &Foretis) -> Result<OrderingResult, AuditError>;
    
    /// Compute defensibility scores for all loaded TBIDs
    pub fn defensibility_scores(&self) -> Result<HashMap<String, DefensibilityScore>, AuditError>;
    
    /// Detect statistical anomalies in loaded data
    pub fn detect_anomalies(&self) -> Result<Vec<AnomalyReport>, AuditError>;
    
    /// Generate a complete audit report
    pub fn generate_report(&self, options: ReportOptions) -> Result<AuditReport, AuditError>;
}
```

### 7.2 ActiveExplorer — Network Query Interface

```rust
pub struct ActiveExplorer {
    // Internal state: server connections, cached peer data
}

impl ActiveExplorer {
    /// Create a new explorer
    pub fn new() -> Self;
    
    /// Add a server connection (JSON-RPC URL or multiaddr)
    pub fn add_server(&mut self, addr: &str) -> Result<(), AuditError>;
    
    /// Discover peers of a given TBID via DHT
    pub async fn discover_peers(&self, tbid_hex: &str) -> Result<Vec<PeerInfo>, AuditError>;
    
    /// Fetch a calendar slice from a server
    pub async fn fetch_calendar_slice(
        &self, tbid_hex: &str, start_tick: u64, end_tick: u64
    ) -> Result<Vec<TickRecord>, AuditError>;
    
    /// Find shared attestors between two parties
    pub async fn find_shared_attestors(
        &self, tbid_a: &str, tbid_b: &str, window: TimeWindow
    ) -> Result<Vec<SharedWitness>, AuditError>;
    
    /// Attempt to resolve a concurrent pair using active queries
    pub async fn resolve_concurrent_pair(
        &self, foretis_a: &Foretis, foretis_b: &Foretis
    ) -> Result<OrderingResult, AuditError>;
}
```

### 7.3 ForetisComparator — Standalone Comparator

```rust
pub struct ForetisComparator {
    /// Chronon duration lookup (tbid_hex → chronon_ns)
    chronon_registry: HashMap<String, u64>,
    /// External attestation index
    attestation_index: AttestationIndex,
}

impl ForetisComparator {
    pub fn new() -> Self;
    
    /// Register chronon duration for a TBID
    pub fn register_chronon(&mut self, tbid_hex: &str, chronon_ns: u64);
    
    /// Add an external attestation as ordering evidence
    pub fn add_attestation(&mut self, att: &ExternalAttestation);
    
    /// Compare two foretides
    pub fn compare(&self, a: &Foretis, b: &Foretis) -> OrderingResult;
}
```

### 7.4 Wallclock Parsing Utility

```rust
/// Parse "UE+<nanoseconds>ns" → u64
pub fn parse_time_being_reference_time(s: &str) -> Result<u64, ParseError>;

/// Format u64 → "UE+<nanoseconds>ns"
pub fn format_time_being_reference_time(ns: u64) -> String;
```

---

## 8. Dependencies

### 8.1 Core Dependencies (audit-core)

| Crate | Purpose | Notes |
|-------|---------|-------|
| `foretias-core` | Type definitions (Foretis, TickRecord, etc.) | Internal workspace dependency |
| `petgraph` | Graph algorithms (toposort, SCC, betweenness) | Foundation for poset |
| `serde` + `serde_json` | Serialization | Already in workspace |
| `hex` | Hex encoding | Already in workspace |
| `sha2` | SHA-256 for verification | Already in workspace |

### 8.2 Statistical Analysis Dependencies (optional)

| Crate | Purpose | Notes |
|-------|---------|-------|
| `rs-stats` | KS test, distributions, hypothesis testing | Statistical anomaly detection |
| `ndarray` + `ndarray-stats` | Array computing, summary statistics | Numerical analysis |
| `statrs` | Probability distributions | Distribution fitting |

### 8.3 Visualization Dependencies (Python-side)

| Crate | Purpose | Notes |
|-------|---------|-------|
| `polars` (Python) | DataFrame analysis, filtering, aggregation | Jupyter-friendly |
| `plotters` (Rust) | Static chart generation | CLI output |
| `ploot` (Rust) | Terminal plotting | CLI output |

### 8.4 Active Mode Dependencies (optional)

| Crate | Purpose | Notes |
|-------|---------|-------|
| `reqwest` | HTTP client for JSON-RPC | Server connections |
| `jsonrpsee` | JSON-RPC client | RPC protocol |
| `libp2p` | P2P protocol stack | Optional, for DHT discovery |

### 8.5 Python Binding Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| `pyo3` 0.21 | Python bindings | Same version as foretias-python |
| `maturin` | Build tool | Same as foretias-python |

---

## 9. Open Questions

1. **Sub-chronon ordering**: Can we ever establish ordering within a single chronon? Currently impossible — multiple stamps per tick share the same key and tick_number. Future work: sequence numbers within a tick?

2. **Trust anchoring**: How does an auditor establish initial trust in a TBID? Is there a root of trust (e.g., genesis signature verification, known-good TBID list)?

3. **Backdating detection**: If a party's chronon server is compromised, can we detect that attestations were produced out of order? The `time_being_reference_time` field provides a wall-clock anchor, but is it tamper-proof?

4. **Scalability**: For very large networks (10,000+ TBIDs), poset construction becomes expensive. Should we support hierarchical poset construction (poset per cluster, then merge)?

5. **Defensibility weighting**: The composite score weights (0.30, 0.20, etc.) are heuristic. Should they be configurable? Should there be a standardized scoring profile?

6. **Active mode protocol**: What RPC methods does a foretis-server need to expose for full audit support? Current methods: stamp, verify, get_calendar_slice. Do we need: get_external_attestations, get_peer_list, get_chronon_config?

7. **Cross-certificate verification**: Should the library verify that an attester's TickRecord matches their registered TBID public key? (Currently assumed but not always enforced.)

8. **Export formats**: Beyond JSON, should the audit report support PDF (for legal proceedings), CSV (for spreadsheet analysis), or DOT (for Graphviz visualization)?

---

## 10. Versioning & Compatibility

- **Target version**: foretis-audit v0.1.0
- **Dependency on foretias types**: Uses foretias-core types by reference (no modification to existing types)
- **Breaking change policy**: foretis-audit is independent; changes to foretias-core types may require audit-core updates
- **Python module name**: `foretias_audit` (distinct from `foretias_p2p`)
- **pip package name**: `foretias-audit`

---

## Appendix A: Terminology

| Term | Definition |
|------|-----------|
| **Foretide** | Plural of foretis. A stamped attestation from a specific TBID at a specific tick. |
| **TBID** | Time-Being ID — the 96-byte dual-key identity of a foretis server. |
| **Chronon** | The fixed tick interval of a TimeBeing (default 60 seconds). |
| **Tick** | A discrete time step in a calendar, identified by tick_number. |
| **Auto-attestation** | Intra-TBID: consecutive ticks sign each other (forward + backward foretis). |
| **Mutual attestation** | Cross-TBID: one party stamps another party's tick record. |
| **External attestation** | The stored record of a cross-TBID attestation (ExternalAttestation struct). |
| **ForetideRef** | A lightweight reference to a foretide (tbid + tick_number + content_hash). |
| **Poset** | Partially ordered set — the result of cross-TBID ordering analysis. |
| **Concurrent pair** | Two foretides with no ordering constraint between them. |
| **Defensibility score** | A measure of how strongly a party's evidence is corroborated by external witnesses. |
| **Shared attestor** | A TBID that has attested both Party A and Party B, enabling transitive ordering. |

---

## Appendix B: Existing Codebase References

- `p2p/core-engine/src/foretias/tick.rs` — Foretis, TickRecord definitions
- `p2p/core-engine/src/foretias/calendar.rs` — Calendar, integrity_check
- `p2p/core-engine/src/foretias/external_attestation.rs` — ExternalAttestation
- `p2p/core-engine/src/foretias/types.rs` — Tbid, TickNumber, type aliases
- `p2p/core-engine/src/chronomatter/mod.rs` — Chronomatter, autonomous ticking
- `p2p/foretias-node/src/communerd/mod.rs` — P2P stamp_peer, route_stamp, DHT
- `p2p/foretias-node/src/server/handlers.rs` — JSON-RPC handlers (verify, stamp)
- `p2p/foretias-python/src/lib.rs` — PyO3 binding patterns (1211 lines)
- `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — Mutual attestation spec
- `specs/TERMINOLOGY_NORMALIZATION_SPEC.md` — Auto-attestation vs mutual attestation distinction
- `specs/foretias-v1.md` — Core model (tick, foretis, calendar, chronomatter)
- `specs/questions.md` — Q#6: tick_number is sequential counter, not wall-clock

---

*End of Specification Draft — 2026-05-12*
