# COMBINED_GROUP5_PLAN.md
# Architectural Foundations — Parallel Agent Plan

**Date:** 2026-05-22
**Paired Spec:** `COMBINED_GROUP5_SPEC.md`
**Status:** Ready for execution. Sections A, B, D are immediately actionable. Section C is blocked on a human decision.
**Pre-flight:** `COMBINED_GROUP2_PLAN.md` Phase A

---

## Agent Assignment

| Branch | Section | Files | Phase | Est. effort |
|--------|---------|-------|-------|-------------|
| `g5-b-crash-recovery` | B — `.tmp` cleanup fix | `foretias/calendar.rs` | 1 | 2-4 hr |
| `g5-d-agents-rules` | D — AGENTS.md gap-fill | `AGENTS.md` only | 1 | 1-2 hr |
| `g5-a-clock-inject` | A — Clock injection + DI | 8 files, 17 sites | 2 | 2-4 days |
| `g5-c-rpc-auth` | C — JSON-RPC auth | server, config, HOWTO | 3 | 1-2 days (after decision) |

---

## Branch g5-b-crash-recovery — Crash recovery test + impl (Phase 1)

**Spec:** `COMBINED_GROUP5_SPEC.md` §3
**Worktree:** `${HOME}/tmp/foretias-worktrees/g5-b-crash-recovery-$(date +%s)`

### Steps

- [ ] **Pre-flight** — `COMBINED_GROUP2_PLAN.md` Phase A passes
- [ ] **Read** `p2p/core-engine/src/foretias/calendar.rs:456-540` to understand
      the three crash-recovery tests (`from_tmp`, `ignores_stale_tmp`, `corrupt_tmp`)
      and the load implementation they test
- [ ] **Locate the load function** that processes `.tmp` files; understand its
      current logic for malformed `.tmp` (likely silently retains)
- [ ] **Fix the implementation**: on `.tmp` deserialization failure, remove the
      `.tmp` file and `tracing::warn!` with context. Continue loading from main file.
- [ ] **Update test `calendar_crash_recovery_corrupt_tmp`** (line 506-524):
      - Replace the existing assertion `assert!(Path::new(&tmp_path).exists(), "corrupt .tmp not removed by current implementation");`
      - With: `assert!(!Path::new(&tmp_path).exists(), "corrupt .tmp must be removed by crash-recovery scan");`
      - The test name remains accurate (it tests crash recovery with corrupt .tmp)
- [ ] **Verify** the other two crash-recovery tests still pass without changes:
      `cargo test -p foretias-core -- calendar_crash_recovery`
- [ ] **Verify** no other test relies on the corrupt-`.tmp` being retained:
      `cargo test --workspace`
- [ ] **Commit:**
      ```
      Major: Architectural Foundations (Group 5), Section B — fix calendar crash recovery to remove corrupt .tmp
      Claude Code 2.1.119 (Claude Code); claude-opus-4-7
      ```
- [ ] **Merge to alpha; remove worktree.**

---

## Branch g5-d-agents-rules — AGENTS.md gap-fill (Phase 1)

**Spec:** `COMBINED_GROUP5_SPEC.md` §5
**Worktree:** Not strictly required (text-only); but if used:
`${HOME}/tmp/foretias-worktrees/g5-d-agents-rules-$(date +%s)`

### Steps

- [ ] **Pre-flight** — Phase A passes
- [ ] **Inventory** what's already in AGENTS.md:
      ```bash
      grep -in "secret material\|zeroize\|atomic counter\|fetch_add\|unverified\|verified type\|length valid\|stub.*returns\|stub.*succ\|todo!" AGENTS.md
      ```
- [ ] **For each of the four candidate rules** (see spec §5.2):
      - If grep shows the rule is present and complete → mark "verified present"
      - If absent or incomplete → add the missing text in the most fitting section
      - Take care to extend existing rules, not duplicate them
- [ ] **Read the entire AGENTS.md** after edits to verify nothing got broken
- [ ] **Diff check:**
      ```bash
      git diff AGENTS.md | grep "^-" | grep -v "^---" | head -10
      ```
      Should show no deleted content (only additions).
- [ ] **Commit:**
      ```
      Major: Architectural Foundations (Group 5), Section D — AGENTS.md Rust rule gap-fill
      ```
- [ ] **Merge to alpha; remove worktree.**

---

## Branch g5-a-clock-inject — Clock injection sweep + DI fix (Phase 2)

**Spec:** `COMBINED_GROUP5_SPEC.md` §2
**Worktree:** `${HOME}/tmp/foretias-worktrees/g5-a-clock-inject-$(date +%s)`
**Prerequisite:** All Phase 1 branches merged (g5-b, g5-d, g3-b, g3-e, g3-f, g4-a)
**Note:** This branch must not run in parallel with anything else touching the
17 sites listed in spec §2.1. It is the bottleneck in the dependency graph.

### Steps

- [ ] **Pre-flight** — Phase A passes
- [ ] **Snapshot the current state:**
      ```bash
      grep -rn "SystemTime::now()" p2p/{core-engine,foretias-client,foretias-server}/src \
        > /tmp/clock-sites-before.txt
      wc -l /tmp/clock-sites-before.txt   # should be 17 (or 18 incl. clock.rs:32)
      ```
- [ ] **Plumbing pass — add `Clock` field where missing:**
      - `Communerd::new(...)` add `clock: Arc<dyn Clock>` parameter
      - `Communerd` struct add `clock: Arc<dyn Clock>` field
      - `TbidHandshake::new` add `clock` parameter
      - `EncryptedJsonlCalendarStore::new` add `clock` parameter
      - Other constructors as required by compile errors
- [ ] **Conversion pass — replace each `SystemTime::now()` call** with
      `self.clock.now_ns()?` (or `.expect("invariant: ...")` where appropriate;
      see spec §2.2)
- [ ] **Top-level injection:** in `p2p/foretias-server/src/main.rs` and any
      other binary entry, construct `Arc::new(crate::clock::SystemClock) as Arc<dyn Clock>`
      and pass through
- [ ] **DI fix in `Chronomatter::new`**: per spec §2.4, accept `crypto:
      Arc<dyn CryptoServer>` explicitly. Remove the internal `crypto_server::new_software(...)`
      call. Update `TimeFamilyServer::new` at `server/mod.rs:69` to construct
      both `crypto` and `clock` and pass them.
- [ ] **Test seam:** Rewrite at least one existing chronomatter test to use
      `FixedClock` to prove the seam is functional:
      ```rust
      let clock = Arc::new(crate::clock::FixedClock::new(1_700_000_000_000_000_000));
      ```
- [ ] **Build and test repeatedly during the work:**
      ```bash
      cd p2p && cargo build --workspace && cargo test --workspace
      ```
- [ ] **Final verification:**
      ```bash
      grep -rn "SystemTime::now()" p2p/{core-engine,foretias-client,foretias-server}/src \
        | grep -v "core-engine/src/clock.rs"
      # expect: empty output
      ```
- [ ] **Determinism check:** run `cargo test --workspace` twice in a row;
      results identical
- [ ] **Commit:**
      ```
      Major: Architectural Foundations (Group 5), Section A — clock injection + Chronomatter::new DI fix
      ```
- [ ] **Merge to alpha; remove worktree.**

---

## Branch g5-c-rpc-auth — JSON-RPC authentication (Phase 3)

**Spec:** `COMBINED_GROUP5_SPEC.md` §4
**Status:** ⏸️ BLOCKED on human decision (Option A, B, or C)
**Worktree:** `${HOME}/tmp/foretias-worktrees/g5-c-rpc-auth-$(date +%s)`

### Step 0 — Human decision gate

- [ ] **(@human)** Choose Option A, B, or C from spec §4.1
- [ ] **(@human)** Record decision and brief rationale below:
      `Decision: ___________`
      `Rationale: ___________`

### Steps if A (Bearer Token) was chosen

(Skip if B or C was chosen — separate steps required)

- [ ] **Pre-flight** — Phase A passes; Group 5-A is merged
- [ ] Add `pub struct AuthConfig { token: String }` to `TimeFamilyConfig`
      (`p2p/core-engine/src/config/time_family.rs`) under `pub auth: Option<AuthConfig>`
- [ ] In `p2p/foretias-server/src/server/mod.rs`, intercept JSON-RPC request
      handling: extract auth from header or params field, compare against
      `config.auth.token` if set, reject with `401`-equivalent JSON-RPC error
      otherwise
- [ ] **Exempt** the `ping` method from auth check (liveness probes must work)
- [ ] Update `HOWTO.md` operator section: how to configure and rotate the token
- [ ] Update `AGENTS.md` security section: document the auth model
- [ ] **Add integration test** in `p2p/foretias-server/tests/`:
      - `auth_token_rejects_unauthenticated_stamp` — no token → 401
      - `auth_token_accepts_authenticated_stamp` — correct token → stamp succeeds
      - `auth_token_ignores_ping` — no token + ping → succeeds (liveness exempt)
- [ ] **Commit:**
      ```
      Major: Architectural Foundations (Group 5), Section C — JSON-RPC bearer-token auth (Option A)
      ```
- [ ] **Merge to alpha; remove worktree.**

### Steps if C (Defer) was chosen

- [ ] Add to `AGENTS.md` Security section: "JSON-RPC has no auth layer; deploy
      behind firewall / VPN / mTLS-terminating proxy."
- [ ] Add to `HOWTO.md`: same warning in operator-facing language
- [ ] No code changes
- [ ] **Commit:**
      ```
      Major: Architectural Foundations (Group 5), Section C — defer JSON-RPC auth (document only)
      ```

### Steps if B (mTLS) was chosen

(Out of scope for this spec; would require its own detailed plan)

- [ ] Escalate to coordinator for a dedicated mTLS spec/plan

---

## Group-Level Completion Criteria

- [ ] `g5-b-crash-recovery` merged: corrupt `.tmp` is removed
- [ ] `g5-d-agents-rules` merged: AGENTS.md gap-fill complete (or rules verified present)
- [ ] `g5-a-clock-inject` merged: zero `SystemTime::now()` calls outside `clock.rs`
- [ ] `g5-c-rpc-auth` decision made; if A or B, implementation merged
- [ ] `cargo test --workspace` passes deterministically (two runs identical)
- [ ] `cargo build --workspace` zero warnings
