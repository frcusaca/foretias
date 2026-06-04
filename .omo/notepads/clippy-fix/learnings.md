# Clippy Fix Learnings — Wave 2-3

## Wave 2: Struct Extraction (A5)

### GossipLoopConfig
- `gossip_event_loop` had 11 args -> wrapped in `GossipLoopConfig` struct
- Key lesson: `Arc<dyn CryptoServer>` and `Arc<dyn Clock>` don't implement `Debug` — cannot derive `Debug` on structs containing trait objects
- Solution: Omitted `#[derive(Debug, Clone)]` entirely; struct is constructed via `new()` and destructured via pattern matching

### RegistrationConfig
- `refresh_self_registration` had 9 args -> wrapped in `RegistrationConfig` struct
- Same Debug/Clone limitation as GossipLoopConfig
- Changed `json_rpc_addr: &str` to `json_rpc_addr: String` to avoid lifetime issues with owned struct

## Wave 3: Type Aliases + Visibility

### A4: type_complexity
- `PendingLookupMap` and `PendingFamilyLookupMap` used in both struct fields and function params
- `FbMockHostResult` for test fixture return type
- `AutoAttestationResult` for chronomatter return type — must be placed OUTSIDE `impl` block (inherent associated types are unstable)

### A7: private_interfaces
- `CommunerdetteExecutor` was `pub(self)` but used in `pub(super)` methods -> widened to `pub(super)`
- `Communerdette` was `pub(super)` but used in `pub(crate)` methods -> widened to `pub(crate)`
- `CommunerdetteHost` was `pub(super)` but used in `pub(crate)` methods -> widened to `pub(crate)`
- Rule: visibility of type must be >= visibility of any method that exposes it

### A16: large_enum_variant
- `NetworkEvent::Identified { info: identify::Info }` was 640 bytes
- Boxed the variant: `info: Box<identify::Info>`
- Updated construction site in swarm.rs to use `Box::new(info.clone())`
- Pattern matches with `{ .. }` still work without changes

### A17: dead_code
- `verify_report_signature` reserved for future binding proof verification
- Added `#[allow(dead_code)]` with docstring explaining reservation

## Warning Reduction
- Before: 48 warnings (266 tests pass)
- After: 35 warnings (27 unique, 266 tests pass)
- Remaining warnings are from Waves 4-7 (too_many_arguments, deprecated, pedantic, suppressions)
