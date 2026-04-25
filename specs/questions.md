# Fortias MVP — Accumulated Questions

Questions for human review during v0.1 implementation.

---

## C11 Core (v0.1.2)

1. **FROST stub approach** — The spec says stub behind compile flag. I'm using `#if 0` to provide function bodies that return `FORTIAS_ERR_UNSUPPORTED`. Is this acceptable, or would you prefer a separate stub file compiled when `FORTIAS_CORE_FROST=0`?

2. **P-256 stub approach** — User said "stub with not implemented exception." I'm making all `fortias_p256_*` functions return `FORTIAS_ERR_UNSUPPORTED`. The C API won't abort, but the Rust wrapper will translate this to `CryptoError::Unsupported`. Is this the right behavior?

3. **Noise_XX stub** — User said stub for later. I'm providing the full state struct but all functions return `FORTIAS_ERR_UNSUPPORTED`. The `fortias_noise_destroy` does a memzero. Is this sufficient?

4. **BLAKE3 implementation** — Should I vendor the BLAKE3 reference C implementation into `hash_blake3.c`, or is stubbing it for v0.1 acceptable (the Python layer uses `hashlib.blake2b` as fallback)?

5. **Legacy MD5/SHA-1** — Should I vendor pure-C implementations, or stub these as well? They're only needed for "noncrypto legacy" use cases (file checksums).

---

## Rust Node (v0.1.3)

6. **Calendar tick_number lookup** — The Python Inquirer uses `fortis.tick_number` as a counter index into the ticks array (e.g., `ticks[tick_number]`). The spec says `tick_number` is "nanoseconds since Unix epoch." There's a mismatch: the Python code treats `tick_number` as both a wall-clock timestamp AND an array index. The Inquirer's `_lookup_surrounding_ticks` uses it as an index. Should the Rust implementation follow the Python pattern exactly (dual semantics), or should we separate "tick counter (u32)" from "tick timestamp (u64)"?

7. **Chronomatter tick_number vs tick_counter** — In the Python code, `ChronomatterV1.current_tick` returns `_tick_counter` (sequential 0, 1, 2...), but `TickRecord.tick_number` stores nanoseconds. The `Fortis.tick_number` stores the counter value (0, 1, 2...). The verification code looks up by counter index. This is confusing. Should I preserve this exact behavior in Rust?

8. **Dormant loading** — The Python `load()` returns a dormant TimeFamily that can verify but not stamp. The spec says "per-run identity lifecycle: process start = brand new identity." For the Rust server, should `fortias serve` start fresh each time (no load), or should it optionally resume from a saved calendar?

---

## TimeFamilyServer (v0.1.4)

9. **JSON-RPC transport** — Should the server use raw TCP with newline-delimited JSON-RPC, or HTTP/1.1 with JSON body? For the v0.1 local-server MVP, raw TCP is simpler. HTTP would be more standard.

10. **Server config file** — Spec says `~/.config/fortias/fortias.settings.json`. Should I support TOML as well (the overview doc has a TOML config example)?

11. **Client stamp/verify** — The spec says "This instance of time being is alive for only one query." Does this mean the CLI stamp/verify command creates a fresh TimeBeing identity for each invocation (just like the server does), or does it just mean it's a short-lived process?

12. **Calendar slice API** — `get_calendar_slice(cal_tbid, cal_tick_start, count)` — `cal_tick_start` is described as "tick start" but the Python `get()` method uses counter index, not wall-clock. Should this be counter-index based?

---

## Architecture

13. **Git worktree vs single repo** — The spec shows a large directory structure under `fortias/p2p/`. Should I keep everything in the single `fortias/` git repo (adding `p2p/core/`, `p2p/node/`, `p2p/bindings/`), or use separate git worktrees for the Rust/C11 subprojects?

14. **Makefile** — Should I create the Makefile from Part 16 now (v0.1) or defer until all components exist?

---

## P2P / Later Phases (noted for awareness)

15. **libsodium P-256** — libsodium doesn't have P-256. The spec says "use mbedTLS for P-256." Should the C11 code depend on both libsodium AND mbedtls, or should P-256 be compiled separately?

16. **Noise_XX vs libp2p Noise** — libp2p has its own Noise implementation. Should the C11 Noise_XX be designed to interop with libp2p's Noise transport, or is it independent?
