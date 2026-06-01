
## 2026-05-31 Initial Assessment

### Code State vs Plan State
- Many Phase 13/15/16/17 checkboxes are STALE - code is implemented but plan not updated
- Phases 1-12 production code: COMPLETE (merged on alpha)
- Phase 13 (FB gossip): Production code COMPLETE
- Phase 15 (FamilyRecord): COMPLETE with tests
- Phase 16 (Canonical encoding): COMPLETE
- Phase 17 (Test suites): TinmanSuite has rustdoc JSON bug

### Key Files
- communerdette.rs (3384 lines) - all spawn functions
- clean_auth.rs (1196 lines) - trust boundary types
- report.rs (453 lines) - ProbityReport
- gossip_handler.rs (375 lines) - FB/GNF reception
- toppoli.rs (552 lines) - multi-peer harness
- trust_boundary_type_usage.rs (703 lines) - snapshot test
- family_record.rs (220 lines) - FamilyRecord

### Build
- Need `rustup default stable` before cargo
- CMAKE_BUILD_PARALLEL_LEVEL=10

## 2026-05-31 Design Decisions (Q1-Q5)

### Q1: stamp_chronon API
- Communerd exposes `stamp_chronon` to externals with serialization control
- CommunerdetteLine provides this for remote time families
- New API, not breaking existing `stamp`

### Q2: TBID binding proof levels
- Full dual-key at first exchange (channel binding)
- Fast-key only for subsequent messages (L2 ongoing)
- Already implemented

### Q3: Liveness signals
- Primary: successful application RPCs (stamp, get_tick)
- Secondary: libp2p transport-level ping
- Tertiary: JSON-RPC ping/pong ~daily (86400000ms)

### Q4: Mirror queue
- Two cases: block-by-block push, most-recent-readyed-block push
- Both push-based (remote calls to submit calendar data)
- Deferred until mirror streaming resumes

### Q5: Outbound envelope type
- `Externalized<T>` with `ExternalizedBuilder<T>` is the correct pattern
- Concrete `Externalized*` structs being removed (violation of generic pattern)

### ExternalizedChrononRecord Issue
- 5 concrete types found: ExternalizedChrononRecord, ExternalizedForetis, ExternalizedEpochSnapshot, ExternalizedAttestation, ExternalizedProbityReport
- 31 usages across 7 files
- Being replaced with generic `Externalized<T>`
