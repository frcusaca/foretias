## Learnings — Group 7 Signing

## 2026-05-31 15:02 Task 1-3
- postcard added to core-engine dependencies with `alloc` feature
- ChrononRecord already derives Serialize (from line 13 tick.rs)
- UnverifiedSignatureEnvelope generic impl blocks (from_parsed, inner, into_inner, from_bytes, from_json_value, TrustedInner) are intact at clean_auth.rs lines 75-110
- SignatureEntry/SignerRole/SigAlgorithm already defined in clean_auth.rs lines 25-49 (from Task 2 rename phase)
- RecordBase trait at clean_auth.rs lines 112-120, ChrononRecord impl at tick.rs lines 439-443
- Test `record_base_always_require_full_signature_default` passes
- Workspace compiles clean, 235 lib tests pass after Tasks 1-3
