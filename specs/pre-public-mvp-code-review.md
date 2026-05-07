# Foretias — Pre-Public MVP Code Review

**Reviewer:** AI code-review pass (Claude Code, Opus 4.7)
**Scope:** All in-tree code under `foretias/` excluding `target/`, `build/`, `__pycache__/`.
**Sources reviewed:**
- C11 core (`p2p/core/`) and shadow C (`p2p/core-engine/src/core/algorithms.{c,h}`)
- Rust workspace: `p2p/core-engine/`, `p2p/foretias-node/`, `p2p/foretias-python/`, `p2p/foretias-java/`
- Python prototype (`src/foretias/`)
- Tests: `tests/`, `integration-tests/`, `p2p/core/tests/`, `p2p/core-engine/src/integration_tests.rs`, `p2p/foretias-node/tests/`, `p2p/foretias-python/tests/python/`, `p2p/integration_sanity.sh`, `p2p/dht_stress_test.sh`
- CI: `.github/workflows/`
- Specs: `specs/FORETIAS_0_OVERVIEW.md`, `FORETIAS_1_MVP_SPEC.md`, `FORETIAS_2_P2P_SPEC.md`, `FORETIAS_3_PQC_INTEGRATION.md`, `CALENDAR_REPLICATION_SPEC.md`, `CLI_SPECIFIED.md`, `FORETIAS_CLI_SPEC.md`, and others

Severity legend: **[CRIT]** = ship-blocker · **[HIGH]** = fix before public tag · **[MED]** = fix soon · **[LOW]** = nice-to-have.

(@human — "pre-public" means there is no production user yet. Findings prioritise correctness, breaking changes, and shrinking the audit surface before the first published artifact, not backwards compatibility.)

---

## 0. Executive Summary

The project has correct semantic intent and solid spec discipline, but the **implementation has drifted from the spec on four hot paths simultaneously**, several of which combine into a single ship-blocking bug:

1. **`SoftwareCryptoServer::sign()` produces an Ed25519 signature, but `signature_algorithm()` reports SPHINCS+.** Every stamp records a tag/signature mismatch that `verify_with` will dispatch incorrectly. (CRIT, see §2.1.)
2. **The Python prototype's auto-attestation blob and the Rust auto-attestation blob differ by two trailing fields (`stamps_per_tick`, `aa_nonce`).** Calendars cannot interoperate cross-language. The Python prototype is declared the source of truth in spec but is also marked deprecated module-wide. (CRIT, see §2.2.)
3. **PQC C source files (`signing_sphincs.c`, `signing_dilithium.c`, `kem_mlkem.c`) are not compiled by `p2p/core/CMakeLists.txt`.** The Rust binding crate compiles its own copies in `core-engine/src/core/algorithms.{c,h}`, creating a parallel C build that bypasses C11 verified-core invariants. (CRIT, see §1.2.)
4. **`SoftwareCryptoServer::Drop` zeroes SPHINCS+ and ML-KEM secrets but skips Dilithium.** Combined with eager generation of all three PQC keypairs at every server start (regardless of selected algorithm), the running process always carries unzeroed PQC secrets. (HIGH, see §3.1.)

Beyond these, there is **no PR-time CI** (only a release tag job exists), no fuzzing, no cross-language equivalence test for any PQC algorithm, and stack-overflow / nonce-overflow risks in the C Noise XX implementation. Recommended ship-blocker list at end (§9).

---

## 1. Spec ↔ Implementation Drift

### 1.1 Python prototype tagged "deprecated" while spec calls it source of truth

- **[HIGH]** `src/foretias/_timebeing.py:13–19`, `calendar.py:8–14`, `crypto.py:11–17`, `models.py:8–14` — every Python module emits `DeprecationWarning` on import. But `FORETIAS_0_OVERVIEW.md §0.7` explicitly states: *"Existing Python Code Is the Source of Truth for Foretias Semantics."* Either remove the deprecation warnings (and harden the Python prototype) or amend §0.7 to redesignate the Rust crate as the canonical reference. As written, an importer is told to migrate away from the spec's own normative reference. Recommend updating §0.7 to: "The Rust `core-engine` crate is the source of truth from v0.1.4 onward; the pure-Python prototype is retained for documentation and historical equivalence tests only."

### 1.2 Duplicated C build paths (PQC files not compiled in C11 core)

- **[CRIT]** `p2p/core/CMakeLists.txt:23–40` — `signing_sphincs.c`, `signing_dilithium.c`, `kem_mlkem.c` are NOT in `FORETIAS_CORE_SOURCES`. The Rust `build.rs` instead compiles `p2p/core-engine/src/core/algorithms.{c,h}` (a parallel source tree) without the strict flags `-Wpedantic -Werror -fstack-protector -fvisibility=hidden` mandated by `FORETIAS_1_MVP_SPEC §4.1`. Result: the C11 verified-core boundary is not enforced for PQC code. Spec calls for `p2p/core/src/algorithms.c` — currently absent.

- **[HIGH]** Function names diverge: spec uses `foretias_sphincs_*`, `foretias_dilithium_*`, `foretias_mlkem_*`; implementation in `p2p/core/src/` uses `foretias_sphincs_sha2_128s_*`, `foretias_dilithium3_*`, `foretias_mlkem_768_*`. `foretias_core.h` declares both forms inconsistently — the Rust FFI and C build will silently disagree on the symbol surface. Pick one canonical form, regenerate bindings, update spec.

- **[HIGH]** `p2p/core/src/hash_legacy_insecure_md5.c:2`, `hash_legacy_insecure_sha1.c:2` — link OpenSSL EVP. `FORETIAS_1_MVP_SPEC §4.3` explicitly forbids: *"Do NOT link against OpenSSL for this — vendor a small pure-C implementation."* CMakeLists also adds OpenSSL as a hard dependency, expanding the supply-chain surface beyond libsodium + liboqs.

### 1.3 `TickRecord` and `Foretis` field divergence

- **[CRIT]** Auto-attestation blob layouts differ between languages:
  - **Python** (`src/foretias/_timebeing.py:114–120, :152–158`): `tbid || A.tick || A.pk || B.tick || B.pk` — **no `stamps_per_tick`, no nonce.**
  - **Rust** (`p2p/core-engine/src/foretias/tick.rs:149–168`): `tbid || A.tick || A.pk || B.tick || B.pk || stamps_per_tick (u64 BE) || nonce (16B)`.
  - Calendars produced by one cannot be verified by the other. The README documents the Rust shape (`aa_nonce: bytes`) but only the Python `_timebeing` code is referenced in spec §0.7 as canonical. Pick the Rust layout (which is strictly stronger; replay-resistant), update Python to match, then run a cross-language equivalence test.

- **[HIGH]** `src/foretias/models.py:20–35` — `TickRecord` has no `aa_nonce` field; the README's "Data models" table claims one. Either README or model is wrong.

- **[HIGH]** Field naming divergence: Python `Foretis.my_content_hash` vs Rust `Foretis.content_hash`. Python `aa_nonce` (claimed in README, absent in model) vs Rust `TickRecord.aa_nonce`. Cross-language JSON cannot round-trip without a translation shim.

- **[MED]** `p2p/core-engine/src/foretias/tick.rs:27` — `#[serde(default)] pub stamps_per_tick: u64` directly contradicts `FORETIAS_3 §front-matter`: *"no `#[serde(default)]` for new fields, no legacy migration paths."* Remove the attribute and bump a migration note instead.

- **[MED]** `tick.rs:44 signature_algorithm: String` and `:52 time_being_reference_time: String` — present in Rust `Foretis`, absent from `FORETIAS_1_MVP_SPEC §7.1`. Update spec to declare these (PQC spec §1 covers `signature_algorithm` but not the time string).

- **[MED]** `tick.rs:31 external_attestations: Vec<ExternalAttestation>` is an undocumented addition. Add to spec or move to a side-channel structure. Mixing third-party attestations into `TickRecord` widens the calendar's signed surface.

### 1.4 CryptoServer trait drift

- **[HIGH]** `p2p/core-engine/src/crypto_server/software.rs:151` — `sign()` always calls `ed25519_sign_with_handle`, but `signature_algorithm()` returns `SPHINCS_SHA2_128S` (line 229). The `tick.rs:84–98` stamp path then writes Ed25519 bytes into `Foretis.signature` and tags `signature_algorithm = "SPHINCS+-SHA2-128s-simple"`. Verification dispatches by tag — **so verify will call SPHINCS+ verify on Ed25519 bytes and reject every legitimate stamp**. (Verify in `tick.rs:135` calls `verify_with(&pub_key, &rec.signature_algorithm, ...)`.) This is the most critical bug in the codebase; it is masked only because the unit tests construct `Foretis` and `TickRecord` from the same `server` instance and never exercise the inconsistency end-to-end on a non-Ed25519 algorithm. See §3 for fix sketch.

- **[HIGH]** `software.rs:162–168` — `ecdh_ed25519` and `ecdh_p256` return `Unsupported`. Noise XX is unusable through the trait. `FORETIAS_2_P2P_3_libp2p_handshake.md` requires a working handshake. Today's libp2p code path must be using its own ECDH outside the CryptoServer trait — a violation of the spec design that "private key never escapes."

- **[HIGH]** `software.rs:158–160` and `core/src/signing_p256.c:4–28` — P-256 verify, sign, keypair are all stubs. Spec `FORETIAS_0 §0.6` says *"Every Foretias node ships software implementations of both Ed25519 and P-256."* Either remove P-256 from spec or implement it.

- **[MED]** `software.rs:220–222` — `frost_sign_partial` is `Unsupported`. `FORETIAS_2_P2P_8_epoch_consensus.md` depends on FROST. v0.6 cannot land while this is a stub.

- **[LOW]** `frost_ed25519.c:4–51` (C side) — all FROST C primitives return `UNSUPPORTED` without a `FORETIAS_CORE_FROST=0` compile guard. Spec §4.3 allows gated stub-out; add the compile guard or implement.

### 1.5 JSON-RPC surface drift

- **[HIGH]** `p2p/foretias-node/src/server/handlers.rs:31, :74, :130, :240` — every handler reads `params.get("id")`, but per `jsonrpc.rs`, the `id` field is at the **top of the JSON-RPC envelope**, not inside `params`. Unless the dispatcher manually injects the request id into `params` (not visible in `server/mod.rs`), every handler returns a response with `id: null`, breaking JSON-RPC 2.0 correlation. Audit the dispatch path; if the dispatcher does inject `id`, document it; if not, fix.

- **[HIGH]** `handlers.rs:11` — `MAX_CONTENT_BYTES = 1_073_741_824` (1 GiB). A single stamp request can cost ~1 GiB of memory before validation. Spec gives no hard limit; recommend 16 MiB pre-public, raised only with explicit operator config.

- **[HIGH]** `handlers.rs:114, :182` — `tokio::runtime::Handle::current().block_on(...)` from inside a sync function called from a tokio task. If the surrounding context is already inside a `current_thread` or single-threaded runtime, this deadlocks. Refactor handlers to be `async fn` and dispatch via `axum::routing` directly (the file already imports `axum`).

- **[MED]** `handlers.rs:171–173` — `verify` returns `valid: false` with method `"local"` when the foretis's TBID is not in the calendar. The caller cannot distinguish "I don't know that calendar" from "signature is invalid." Add a third state (e.g., `method: "unknown_tbid"`) or return a JSON-RPC error.

- **[MED]** `handlers.rs:12, :250–253` — `MAX_CALENDAR_SLICE_COUNT = 10_000` × SPHINCS+ 7,856-byte signatures × 2 (forward/backward) ≈ **~157 MiB JSON response** for a max slice. Either lower the cap, stream the response, or move to length-delimited binary.

- **[MED]** `FORETIAS_1_MVP_SPEC §2` says `get_calendar_slice` returns *"1 or 2 records depending on whether cal_tick_start's chronon duration has elapsed or not"*. Current implementation (`handlers.rs:239–264`) honours an arbitrary `count` parameter and ignores that spec rule. Decide which behaviour is canonical.

- **[LOW]** Spec uses `cal_tbid` in `get_calendar_slice` parameters; implementation uses no tbid filter (returns this server's calendar only). For dormant multi-tbid lookups (`FORETIAS_0 §0.1` post-final addition), the tbid parameter must be wired through.

### 1.6 Calendar persistence drift

- **[MED]** `p2p/foretias-node/src/calendar_store/encrypted_jsonl.rs` exists in the tree (347 lines, fully implemented). Per `FORETIAS_0 §14`, encrypted JSONL is a **v0.7 feature** — and the current branch is targeting MVP/v0.1 (per recent commit history). The store is self-contained and dormant in the codebase, but its presence violates the milestone discipline laid out in spec. Either tag it as v0.7 work and feature-gate it (cargo feature `encrypted-calendar-store`), or fold it into the active calendar path and update the spec.

- **[MED]** `src/foretias/calendar.py:113–129` — Python `save()` does `path.write_text(...)` directly: **no atomic rename**. A crash mid-write corrupts the calendar. The Rust `core-engine/src/foretias/calendar.rs:84–95` uses tempfile+rename correctly. Bring Python into line.

- **[MED]** `src/foretias/calendar.py:111–129` — Python `save()` does NOT serialize `aa_nonce` or `signature_algorithm` (writes only `tick_number`, `public_key`, `forward_foretis`, `backward_foretis`). If the model is updated, the persistence path silently truncates. See §1.3.

### 1.7 Identity collision detection drift

- **[MED]** `p2p/core-engine/src/collision/detector.rs:42–64` — `on_heartbeat` accepts heartbeats with arbitrary `timestamp_ns`; no freshness window. An attacker can replay yesterday's heartbeat with a fresh nonce and trigger a spurious collision. Add `now_ns - hb.timestamp_ns < heartbeat_max_age_ns` check.

- **[LOW]** `detector.rs:34–40` — nonce window is a fixed-size `VecDeque<[u8; 16]>` keyed only by insertion order. Under load, an attacker who knows the ring size can replay a nonce just-evicted from the deque. Bound by time, not count.

---

## 2. Critical Cross-Cutting Bugs

### 2.1 Algorithm/signature-bytes mismatch on every stamp (CRIT)

**Root cause.** `SoftwareCryptoServer::sign()` is hard-coded to Ed25519 (`software.rs:150`), while `signature_algorithm()` returns `SPHINCS_SHA2_128S` (`software.rs:228`). `tick::stamp()` writes those two values into the same `Foretis`, producing artifacts where `signature` is Ed25519 bytes but `signature_algorithm` says SPHINCS+. Verification dispatches by tag (`software.rs:251–267`), so legitimate stamps verify only when the tag happens to be Ed25519, which it never is for the default backend.

**Why tests don't catch this.** The unit tests in `tick.rs:259–322` always stamp and verify against the same in-memory `server`, never deserialize across processes, and never exercise an SPHINCS+ Foretis. Cross-language tests in `p2p/foretias-python/tests/python/test_cross_language.py` likely use the same server identity for both sides.

**Fix sketch (one of):**
1. Make `sign()` dispatch to the algorithm reported by `signature_algorithm()`. The SPHINCS+ path already exists at `signing_sphincs::sphincs_sign`. Then `Foretis.signature` becomes `Vec<u8>` (already is) carrying SPHINCS+ bytes.
2. Or: change `signature_algorithm()` to return `Ed25519` for the default backend, defer SPHINCS+ to opt-in. Update `FORETIAS_3 §0.3` accordingly.

Option 1 matches `FORETIAS_3 §0.3` ("Default signing algorithm becomes SPHINCS+"). Option 2 matches MVP scope. Pick one and add a regression test:

```rust
let s = SoftwareCryptoServer::generate(Ed25519).unwrap();
let alg = s.signature_algorithm();
let sig = s.sign(b"x").unwrap();
let pk = match s.public_key() { Ed25519(p) => p.bytes.to_vec(), _ => unreachable!() };
let sig_bytes = match alg {
    SignatureAlgorithm::Ed25519 => sig.bytes.to_vec(),
    _ => panic!("dispatch via sign_with for PQC"),
};
assert!(s.verify_with(&pk, alg.to_id_string(), b"x", &sig_bytes).unwrap());
```

### 2.2 Python ↔ Rust auto-attestation incompatibility (CRIT)

See §1.3. The two implementations cannot verify each other's calendars. Either:
- **Rust → Python:** add `stamps_per_tick` and `aa_nonce` to Python `TickRecord`, update `_tick`, `_verify_pair`, `Calendar.save/load`. (Preferred — Rust shape is replay-resistant.)
- **Python → Rust:** drop the two extra fields, accept the replay weakness. Not recommended.

Add a cross-language equivalence test that creates a calendar in Python, opens it in Rust, and runs `integrity_check`, then the reverse.

### 2.3 Eager PQC keygen + leaky Drop (HIGH)

`software.rs:51–79` and `:88–118` generate **SPHINCS+, Dilithium, and ML-KEM keypairs at every `SoftwareCryptoServer::generate` call**, even if only Ed25519 is selected. SPHINCS+ keygen is ~10–20 ms, materially slower than Ed25519. More importantly:

- **`Drop` impl skips `dilithium_secret_key`** (`software.rs:121–130` zeroes `seal_key`, `sphincs_secret_key`, `mlkem_secret_key` — no `dilithium_secret_key`). Process exit leaks the Dilithium private key in pages until reuse.
- The keys are stored as `Option<SignatureBytes>` (= `Option<Vec<u8>>`), not behind the `PrivKeyHandle` opaque-handle pattern that `FORETIAS_0 §0.3` mandates. Bytes-in-Rust-`Vec` can be moved/cloned without cleanup hooks.

**Fix:** lazy-generate per algorithm only when first used; wrap PQC secrets in `Zeroizing<Vec<u8>>` (already imported); explicitly zero Dilithium in `Drop` (or replace all three with `Zeroizing` and remove the manual `Drop`).

---

## 3. Security Findings

### 3.1 C11 core (`p2p/core/src/`)

(Findings condensed from the dedicated C-core review — keep file:line precise.)

- **[CRIT]** `noise_xx.c:69, :86` — Nonce counter `(*n)++` has **no overflow guard**. After 2^64 messages a wrap reuses a nonce with the same key — catastrophic for ChaCha20-Poly1305. Add `if (*n == UINT64_MAX) return -1;` before increment, plus a session-end re-key in the wrapper.

- **[HIGH]** `privkey.c:236–240` — Stack buffer `expand_input[64]` is filled with `memcpy(expand_input, info, info_len)` then `expand_input[info_len] = 0x01` with **no length check** on `info_len`. Caller-supplied `info_len > 63` overflows. Add `if (info_len > 63) return FORETIAS_ERR_BAD_INPUT;` at the top of the function.

- **[HIGH]** `privkey.c:74–78, :17–22` — `instance_kek`, `instance_kek_initialized`, `key_gen_counter` are global mutable state. `FORETIAS_1_MVP_SPEC §4.1` forbids this. Either move to a caller-owned context, or carve out an explicit spec exemption with rationale.

- **[HIGH]** `privkey.c:83, :125` — `calloc/free` from `<stdlib.h>` violate "no dynamic allocation in core" (`§4.1`). All other core files honour this.

- **[HIGH]** `hash_legacy_insecure_md5.c:2`, `hash_legacy_insecure_sha1.c:2` — see §1.2 (OpenSSL link).

- **[MED]** `privkey.c:122–156` — `foretias_privkey_ed25519_from_seed` does not zero the caller's `seed` despite the header doc-comment claiming it does (`foretias_core.h:382`). The `const` qualifier prevents zeroing. Drop `const` and zero, or fix the doc.

- **[MED]** `noise_xx.c:259, :415` — 64 KiB stack buffers (`uint8_t p[FORETIAS_NOISE_MAX_MSG]`) on the hot path. Default thread stacks on Alpine/musl are ~80 KiB; this is fragile and may trip stack-protector. Heap-allocate or shrink the per-step bound.

- **[MED]** `noise_xx.c:241–247, :286–287, :313, :350, :361, :369, :394, :419` — early-return paths leave stack-local key material un-zeroed. Add `sodium_memzero` cleanup at every `return` after handshake-key bytes touch the locals.

- **[MED]** `signing_sphincs.c:39, :17`, `signing_dilithium.c:39, :17` — `OQS_MEM_cleanse` uses caller-supplied `len`; if caller passed `{.len=0}`, no cleanse occurs. Use `sizeof(buf->bytes)` or the algorithm's max constant.

- **[MED]** `signing_sphincs.c, signing_dilithium.c, kem_mlkem.c` — no `OQS_init()` anywhere. liboqs requires it once per process for CPU-feature dispatch. Add a one-shot init in `PrivKeyHandle::init()` or a dedicated `foretias_pqc_init`.

- **[LOW]** `rng_mix.c` — name is misleading; no mixing happens. Reads `/dev/urandom` directly per call. For ARM/embedded targets switch to `getrandom(2)` and add a fallback for sandboxed containers.

- **[LOW]** `merkle.c:5–9, :18–22, :49–61` — ACSL/Frama-C annotations exist but `proofs/` directory is empty. Either drive proofs in CI or remove the annotations to avoid implying verification that doesn't exist.

### 3.2 Rust core-engine

- **[HIGH]** `software.rs:121–130` — `Drop` skips `dilithium_secret_key`. See §2.3.

- **[HIGH]** `software.rs:150–249` — `sign()` / `signature_algorithm()` mismatch. See §2.1.

- **[HIGH]** `chronomatter/mod.rs:46–47` — `Chronomatter::new` constructs its own `SoftwareCryptoServer` ignoring the server passed in. The `from_calendar` path (line 73) does take an injected `crypto: Arc<dyn CryptoServer>`. Inconsistent: dependency injection should be uniform. Plumb the crypto server through `Chronomatter::new` so a custom backend can be used.

- **[MED]** `tick.rs:88–91` — `SystemTime::now()` for `time_being_reference_time`. Wall clock can move backward (NTP slew, manual adjustment, leap seconds). For artifact provenance use a monotonic source, or document the wall-clock dependency.

- **[MED]** `chronomatter/mod.rs:31` — `keypairs: RwLock<Vec<TickKeyPair>>` is unbounded; over time, the running process accumulates per-tick keys. Spec `FORETIAS_0 §0.3` says *"Private keys never persisted"* but doesn't bound in-memory retention. Add a configurable LRU and zero evicted keys.

- **[MED]** `chronomatter/mod.rs:140–149` — `generate_and_store_keypair` holds a `write` lock for the duration of `PrivKeyHandle::generate` (which calls libsodium / the PQC stack). On a hot stamp path, this serialises stamping. Move the generation outside the lock.

- **[LOW]** `software.rs:36` — `frost_shares: parking_lot::Mutex<HashMap<String, Zeroizing<Vec<u8>>>>` — committee_id is a free-form `String`. Add length and charset bounds to prevent unbounded growth from malformed peers.

### 3.3 Rust foretias-node (server / P2P / replication)

- **[HIGH]** `server/handlers.rs:11` — 1 GiB content cap; see §1.5.

- **[HIGH]** `server/handlers.rs:114, :182` — `block_on` deadlock vector; see §1.5.

- **[HIGH]** `server/mod.rs:74, :83–84` — `noise_static_priv: [u8; 32]` is held as plain `[u8; 32]` on the server struct, not `Zeroizing<[u8; 32]>` and not behind `PrivKeyHandle`. It is generated at server start and never zeroed on `Drop` (no `Drop` impl). Wrap or move into a CryptoServer.

- **[MED]** `calendar_store/encrypted_jsonl.rs:107–112` — `read_all` does `read_to_string` over the entire file. Unbounded memory. Use a streaming reader (`BufReader::lines`) and lazy-decode each block.

- **[MED]** `calendar_store/encrypted_jsonl.rs:48` — `compute_next_block_id` returns `Result`; on error, falls back to `0`. A transient I/O failure can therefore reset block IDs and create a duplicate. Fail loudly.

- **[MED]** `server/mod.rs:78, :105` — Hard-codes `/tmp/foretias-mirrors` for the mirror store. Must be config-driven for production. /tmp also has world-readable defaults on most distros — secret material adjacent in path confuses operators.

- **[MED]** `server/mod.rs:43–86` — `TimeFamilyServer::new` does not take auth credentials, ACLs, or rate-limit configuration. Any client on the listen port can stamp, verify, and request calendar slices. For pre-public, document this and add at minimum a token-based admin gate for `serve` mode behind a config flag.

- **[MED]** `server/handlers.rs:62–65` — On stamp, calls `server.save()` synchronously inside the request handler. Each stamp triggers a full JSON write (via Rust calendar.rs:84). For high-throughput stamping this serialises on disk. Consider a background flusher with `Notify`-coalesced writes.

- **[LOW]** `server/mod.rs:166` — `json_path.to_str().unwrap()` panics on non-UTF-8 paths. Use `to_string_lossy` or propagate.

### 3.4 Python prototype

- **[HIGH]** `src/foretias/calendar.py:111–129` — non-atomic save (see §1.6).

- **[MED]** `src/foretias/_timebeing.py:107` — `_now_ns()` uses `int(time.time() * 1e9)`, which loses precision past 2^53 ns (~104 days into 1970). Use `time.time_ns()`.

- **[MED]** `src/foretias/_timebeing.py:126–136` — On `_tick`, the *new* private key is generated in `_tick`, then **returned by value to the caller**. The old private key is whatever the caller passed in — the function does not zero or destroy it. Documenting in spec that callers must zero is fragile; consider a class-owned secret-handle pattern even in the prototype.

- **[MED]** `src/foretias/cli.py:71–73, :107` — CLI commands instantiate `PyTimeFamilyServer` without auth or config. `serve` is delegated to a Rust binary, but `stamp` and `verify` start a server-style instance for one shot. If `persist_path` is read-writable, an attacker who can edit the calendar JSON can rewrite the chain (no signature on the JSON envelope itself).

- **[LOW]** `src/foretias/crypto.py:55–57` — `Ed25519PrivateKey.generate()` then `private_bytes_raw()` returns a Python `bytes` object, never zeroed. Python's GC can leave key material in heap arenas. Acceptable for prototype but flag in spec.

- **[LOW]** `src/foretias/cli.py:64, :89` — `open(args.message_file, "rb").read()` reads into memory unbounded. Add a `--max-bytes` cap.

### 3.5 PyO3 bindings (`p2p/foretias-python/src/lib.rs`)

(Surveyed by file size only; 1,202 lines. Detailed review deferred — flagged for follow-up.)

- **[HIGH-FOLLOWUP]** Audit every `#[pyfunction]` / `#[pymethods]` for: panics that cross the Rust→Python boundary (use `catch_unwind`), `unwrap()` on user-supplied JSON, GIL release around blocking I/O, and reference-cycle hazards (`Py<...>` storing `Arc<Mutex<...>>` cycles). Add a fuzz target with `cargo-fuzz` for the PyO3 layer.

---

## 4. Correctness

- **[HIGH]** `core-engine/src/integration_tests.rs` (475 lines) and `foretias-node/tests/integration.rs` (610 lines) exist but neither runs in CI (see §6). Effectively dead.

- **[MED]** `foretias-node/src/server/handlers.rs:138` — `serde_json::from_value(v.clone()).ok()` swallows the deserialisation error. Caller cannot distinguish "missing field" from "wrong type". Return `INVALID_PARAMS` with the parse error string.

- **[MED]** `core-engine/src/foretias/calendar.rs:107–141` — `Calendar::load` recovers from `.tmp` if it has more ticks. But a malicious actor with write access can drop a fabricated `.tmp` with more (forged) ticks and the loader will prefer it. The Python load path runs `integrity_check` on load (`calendar.py:166–171`); the Rust path does not (lines 105–141). Add an `integrity_check` after recovery from `.tmp`.

- **[MED]** `core-engine/src/foretias/calendar.rs:82–95` — `save()` uses `std::fs::rename`; on Windows this is not atomic across volumes. Either mark as POSIX-only or use `cap-std` / `tempfile::persist`.

- **[MED]** `core-engine/src/foretias/calendar.rs:33–43` — `append` rejects non-strictly-ascending tick numbers but does not verify the `forward_foretis` / `backward_foretis` against the previous tick at append time. A Calendar can be in an invalid intermediate state if `append` is misused programmatically. Add a debug-only `verify_pair` after append.

- **[MED]** `core-engine/src/chronomatter/mod.rs:46` — daemon thread loop is not visible in this excerpt; verify (a) clean shutdown on SIGINT, (b) bounded retry on disk-write failures (currently `tracing::warn!` and continue can mask persistent failures), and (c) no panics inside the daemon loop.

- **[LOW]** `core-engine/src/foretias/tick.rs:88` — `SystemTime::now()` may panic on systems before 1970 (`duration_since` returns `Err`); this is mapped to `Internal`. Acceptable, but tests don't cover the negative path.

- **[LOW]** `foretias-node/src/main.rs:56` — `default_value = "9900..9999"` for `--p2p-port-range` parsed as a string and presumably split downstream. Not visible in excerpt; ensure `start <= end` and both are valid u16.

---

## 5. Efficiency

- **[MED]** `core-engine/src/crypto_server/software.rs:51–79, :88–118` — eager generation of all PQC keypairs. ~10–20 ms wasted at startup; ~10 KB of secret material kept alive for the process lifetime even when never used. Lazy-init.

- **[MED]** `foretias-node/src/server/handlers.rs:62–65, :119–121` — `server.save()` after every stamp. Re-encodes the entire JSON calendar each call. With N ticks accumulated, this is O(N) bytes per stamp. Replace with append-only JSONL or coalesced flush.

- **[MED]** `core-engine/src/foretias/tick.rs:79–92, :129–140` — `Vec::with_capacity` then `extend_from_slice` in a hot path. Acceptable, but the same `sig_input` shape is built twice (stamp and verify). Extract into `fn build_sig_input(tbid, tick_number, content) -> Vec<u8>`.

- **[LOW]** `core/src/signing_ed25519.c:12` — re-derives 64-byte secret-key from seed every sign. ~30 µs overhead. Document or expose `_with_keypair` variant for hot loops.

- **[LOW]** `core-engine/src/probity/store.rs` — only 60 lines; expected to grow. Watch for O(N) scans over peer set on every gossip event.

---

## 6. Test Coverage and CI

### 6.1 Inventory

| Layer | Files | LOC | Coverage feel | Notable gap |
|---|---|---|---|---|
| Python prototype unit | `tests/test_*.py` (10 files) | ~50 KB | OK happy path | No fuzz, no PQC, no atomic-save crash recovery |
| Cross-validation | `integration-tests/test_cross_validation.py` | 3.7 KB | Thin | Only one direction; no PQC |
| C11 core | `p2p/core/tests/test_*.c` | one per source | Patchy | No round-trip Noise XX, no SPHINCS+/Dilithium/ML-KEM tests at all |
| Rust core-engine | inline `#[cfg(test)]` in 19 files + `integration_tests.rs` (475 LOC) | mid | OK happy path | No adversarial peer, no fuzz, no PQC cross-language |
| Rust node | inline tests + `tests/integration.rs` (610 LOC) | mid | Sanity-only | No JSON-RPC fuzz, no oversize-request, no malformed-foretis, no DHT eclipse |
| PyO3 bindings | `tests/python/test_bindings.py` (165), `test_cross_language.py` (271), `py_server.py` (86) | small | Light | Same-process only |
| Shell sanity | `p2p/integration_sanity.sh` (10.7 KB), `dht_stress_test.sh` (15.6 KB) | — | Manual run only | Not in CI |

### 6.2 Critical missing tests (HIGH priority before public)

1. **PQC cross-language round-trip**: stamp with SPHINCS+ in Rust, deserialize foretis in Python, verify in Rust again. Repeat for Dilithium and (where applicable) ML-KEM. Currently zero coverage.
2. **Calendar tamper resistance**: flip a byte in `forward_foretis`, `aa_nonce`, `public_key`, `signature_algorithm`; expect `integrity_check == [.., false, ..]`. Coverage exists for `forward_foretis` only (Rust `verify_pair_tampered_returns_false`).
3. **Crash recovery for Python `save`**: there's no equivalent of Rust's `calendar_crash_recovery_from_tmp` test.
4. **Identity-collision dual termination end-to-end**: spawn two processes with forced identical TBID; verify both go dormant within the heartbeat window.
5. **JSON-RPC fuzz**: malformed envelope, oversized `content`, deeply nested JSON (stack overflow), invalid hex, unknown method, empty params, batch requests.
6. **Property tests** (`hypothesis` / `proptest`):
   - `stamp(content); verify(content) == true` ∀ content ∈ Bytes.
   - For any tampered single byte of `Foretis.signature`, `verify == false`.
   - `calendar.append(t); calendar.latest() == t.tick_number`.
7. **Replay protection**: capture a heartbeat, replay it after a window, expect rejection.
8. **C-core fuzz**: libfuzzer targets for `foretias_merkle_verify`, `foretias_noise_step`, `foretias_*_verify`, `foretias_privkey_derive_seal_key`.
9. **PrivKey memzero verification**: post-tick, scan the process's heap pages (or a known buffer) for the previous private-key bytes; assert absence. Use `procfs` or jemalloc poisoning hooks on Linux.
10. **End-to-end MVP test**: spawn `foretias serve` (Rust), `foretias stamp` (Rust client), `foretias verify` (Rust client) in three processes — required by `FORETIAS_0 §2`. Then repeat with the Python CLI. Today `integration_sanity.sh` exists but is not CI-gated.

### 6.3 CI gaps

- **[CRIT]** `.github/workflows/release.yml` is the **only** workflow. It runs on tag push, builds, and publishes — but **no PR-time tests, no lint, no audit, no cross-language equivalence**. For a pre-public security-sensitive project this is the largest single gap.

Recommended CI matrix (one PR-blocking workflow):

```yaml
jobs:
  c-core:        # Linux + clang + gcc; CMake + ctest; -fsanitize=address,undefined
  rust-build:    # cargo build --workspace + clippy -D warnings
  rust-test:     # cargo test --workspace
  rust-miri:     # cargo +nightly miri test for unsafe blocks
  python-test:   # pytest + coverage gate (>=80% lines on _timebeing, calendar, models)
  cross-lang:    # build wheel via maturin, run integration-tests/ + tests/python/
  fuzz-smoke:    # cargo fuzz run … 30s smoke tests on each fuzz target
  audit:         # cargo audit + pip-audit + cargo deny
```

### 6.4 Test-quality smells

- **[MED]** `core-engine/src/foretias/calendar.rs:464–483` — `calendar_crash_recovery_corrupt_tmp` documents that *"corrupt .tmp not removed by current implementation"*. The test asserts the bug; should be a `// FIXME` with a `#[should_panic]` or fixed.
- **[MED]** Many Rust tests use `unwrap()` on test data that includes `random_bytes()` calls — non-deterministic. For property tests, seed an RNG.
- **[MED]** Python tests rely on wall-clock `time.time()` for tick numbers, making timing-sensitive tests flaky. Add fake clocks for testing.

---

## 7. Documentation Drift

- **[HIGH]** `README.md` "Data models" section claims `aa_nonce: bytes` on `TickRecord`, but `src/foretias/models.py` does not have it. README also documents `my_content_hash` on `Foretis` but Rust uses `content_hash`.
- **[MED]** `README.md` build instructions mention `python -m build` and `pip install -e .`, but `pyproject.toml:1–3` declares `maturin` as the build backend. `pip install -e .` will invoke maturin, which expects a Rust toolchain. Document the dependency or add a pure-Python fallback build.
- **[MED]** `README.md` lists `pyforetias` as the Rust-backed package, but `pyproject.toml` exports `foretias_p2p` and the Python entry point is `foretis = "foretias.cli:main"`. Reconcile.
- **[MED]** Spec `FORETIAS_0_OVERVIEW.md` Part 1 describes `foretias/p2p/node/` and `foretias/p2p/bindings/python/`, but the actual layout is `p2p/core-engine/`, `p2p/foretias-node/`, `p2p/foretias-python/`, `p2p/foretias-java/`. Update the spec or restructure the tree.
- **[LOW]** `FORETIAS_0_OVERVIEW.md` Part 16 Makefile references `foretias/p2p/core/build`, `foretias/p2p/node/`, etc. — stale paths.

---

## 8. Dependency / Supply-Chain

- **[MED]** No `cargo audit` configuration; no pinned dependency versions checked. Run `cargo audit` and `pip-audit` regularly. Add to CI per §6.3.
- **[MED]** `liboqs` 0.13.0 (required for SPHINCS+ + Dilithium availability) per `FORETIAS_3 §0.3` — ensure the build script pins this version. Check `Cargo.lock`/`build.rs` for `oqs` 0.11.0.
- **[MED]** OpenSSL link via legacy hash files (`§1.2`) — eliminates the goal of a small audited surface. Vendor MD5/SHA-1 reference C as spec mandates.
- **[LOW]** `parking_lot` and `serde_cbor` (the latter unmaintained as of 2024) — replace `serde_cbor` with `ciborium`.
- **[LOW]** Workspace `Cargo.toml` not surveyed for pinned vs floating versions; recommend `cargo update --dry-run` and lockfile commit policy.

---

## 9. Recommended Pre-Public Action Plan

Ordered by dependency / risk priority. Everything under "P0" is a ship-blocker for a public artifact.

### P0 — Ship blockers

1. **Fix `sign() / signature_algorithm()` mismatch.** §2.1. Decide algorithm policy, dispatch correctly, add regression test.
2. **Reconcile Python and Rust auto-attestation blob layout.** §2.2. Choose Rust shape (with `stamps_per_tick`, `aa_nonce`); update Python; add cross-language equivalence test.
3. **Fix C-core nonce overflow and stack-buffer overflow.** §3.1 (`noise_xx.c` nonce wrap, `privkey.c` HKDF info-len overflow).
4. **Vendor MD5/SHA-1 (remove OpenSSL dep) or remove the legacy hash surface.** §1.2.
5. **Fix `SoftwareCryptoServer::Drop` to zero `dilithium_secret_key`.** §2.3.
6. **Add a PR-blocking CI workflow** (build, test, clippy, ASAN/UBSAN on C, miri on Rust unsafe). §6.3.
7. **Compile PQC C files in `p2p/core/CMakeLists.txt` or remove the parallel build path.** §1.2.
8. **Lower `MAX_CONTENT_BYTES` and remove `block_on` deadlock in handlers.** §1.5, §3.3.

### P1 — Fix before public tag

9. Resolve Python "deprecated" vs spec "source of truth" tension. §1.1.
10. Lazy-init PQC keypairs; wrap all PQC secrets in `Zeroizing`. §2.3, §5.
11. Implement P-256 (or remove from spec). §1.4.
12. Implement `ecdh_ed25519` (Noise XX is broken otherwise). §1.4.
13. Calendar `.tmp` recovery should run `integrity_check`. §4.
14. Streaming reader for `EncryptedJsonlCalendarStore::read_all`. §3.3.
15. Atomic save in Python `calendar.py`. §1.6.
16. Wire CLI `--persist-path`, `cal_tbid` parameter through to `get_calendar_slice`. §1.5.
17. Add the cross-language equivalence and tamper-resistance tests listed in §6.2.
18. Wire `integration_sanity.sh` and the 3-process MVP integration test into CI. §6.2.10.

### P2 — Soon after public tag

19. Property-based testing (hypothesis + proptest). §6.2.6.
20. Fuzz harnesses for C primitives and PyO3 bindings. §6.2.8.
21. `tracing` filter audit: ensure no key bytes ever reach a `debug!` / `trace!` log.
22. Replace `serde_cbor` with `ciborium`.
23. Document the trust/auth model for the JSON-RPC server (no auth today).
24. Implement FROST and integration with v0.6 epoch consensus, or feature-gate behind `cfg(feature = "epoch")`.

### P3 — Hygiene

25. Make spec paths and Makefile targets match the actual `p2p/` layout.
26. Reconcile README field names (`my_content_hash` vs `content_hash`, `pyforetias` vs `foretias_p2p`).
27. Move `keypairs: RwLock<Vec<TickKeyPair>>` to a bounded LRU.
28. Replace ad-hoc `tracing::warn!` on `save()` failures with operator-visible counters that fail fast under sustained errors.

---

## 10. Where to Look First

For a maintainer with one afternoon, these eight files account for the majority of P0 / P1 risk:

1. `p2p/core-engine/src/crypto_server/software.rs` — algorithm dispatch + Drop.
2. `p2p/core-engine/src/foretias/tick.rs` — auto-attestation blob shape + serde defaults.
3. `src/foretias/_timebeing.py` — Python auto-attestation; reconcile with Rust.
4. `p2p/core/src/noise_xx.c` — nonce overflow, stack residue.
5. `p2p/core/src/privkey.c` — HKDF stack overflow + dynamic alloc + global state.
6. `p2p/foretias-node/src/server/handlers.rs` — request limits, block_on, id-from-params bug.
7. `p2p/core/CMakeLists.txt` and `p2p/core-engine/src/core/algorithms.{c,h}` — duplicated PQC build path.
8. `.github/workflows/` — add a real CI workflow.

— END —
