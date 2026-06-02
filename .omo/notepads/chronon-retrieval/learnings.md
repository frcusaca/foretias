
# Learnings — Chronon Retrieval Integration Tests (2026-06-02)

## Key Findings

- **ChrononRecord JSON field name**: The `chronon_number` field serializes as `tick_number` via `#[serde(rename = "tick_number")]` in `core-engine/src/foretias/tick.rs`. Always use `tick_number` when asserting on JSON responses.

- **CalendarStore population**: The CalendarStore (encrypted JSONL) is separate from the in-memory Calendar. `handle_stamp` writes to the in-memory Calendar but NOT to the CalendarStore. For tests, pre-populate by creating a parallel `EncryptedJsonlCalendarStore` instance pointing to the same `calendar_store.jsonl` file using the server's own `CryptoServer` (obtained via `server.chronomatter().crypto_server()`).

- **Gap ranges are exclusive**: `ChrononChainResult::Partial.gaps` uses `std::ops::Range<u64>` (exclusive end). A gap `5..10` means chronons 5–9 are missing.

- **Server creation**: `TimeFamilyServer::new_with_persist` creates a `CalendarStore` backed by `{persist_path}/calendar_store.jsonl`. The crypto key is generated fresh each time.
