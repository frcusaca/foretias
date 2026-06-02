## 2026-06-02 — SAFETY.md exception handling section added

- SAFETY.md has 5 levels (C11 core, signing authority, type-guarded records, component trust, secret lifecycle) plus a summary
- New section added after summary, before end of file
- Confirmed error types: NodeError, CryptoError, CleanAuthError, TransportError, ForetiasError all exist in codebase
- parking_lot is used everywhere for Mutex/RwLock (no std::sync::Mutex in new code)
- resp_error() pattern in handlers.rs maps domain errors to JSON-RPC error codes
- Existing style uses `---` separators between major sections, markdown tables for quick reference, code blocks for examples
