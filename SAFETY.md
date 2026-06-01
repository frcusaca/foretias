# SAFETY.md — Foretias Security Architecture

**Source of truth for security requirements.** All code, specs, and reviews defer to this document on questions of secret storage, signing authority, type-guarded records, and trust boundaries.

**Canonical specs referenced:**
- `specs/HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md` — Secret key lifecycle (Five Hard Requirements)
- `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md` — Signing boundary and Communerdette architecture
- `specs/COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_SPEC.md` — Type-enforced trust boundaries
- `specs/FORETIAS_3_PQC_INTEGRATION.md` — Post-quantum cryptography integration

---

## Level 1 — C11 Core: Secret Key Protection

The C11 core (`p2p/core/`) is the foundation. Secrets are protected at the lowest level with no programmatic or linguistic access from higher layers.

### KEK (Key Encryption Key) Hiding

Every long-lived secret is encrypted in memory using a process-singleton KEK:

- A 32-byte KEK is generated at startup via `randombytes_buf` and stored in **file-scope static memory** inside `privkey.c`
- The KEK never leaves this translation unit — it is not exported, not passed across FFI, not accessible from Rust
- Every persistent secret is encrypted with the KEK using ChaCha20-Poly1305 AEAD with a unique nonce
- The encrypted form (`ciphertext + 16-byte MAC + 24-byte nonce`) is what is stored in structs
- The plaintext exists only during a single decrypt-use-zero operation

**Even if an attacker dumps the encrypted key handle from memory, they cannot recover the seed without the KEK.**

### Opaque Pointer Pattern

Secrets cross the FFI boundary as opaque pointers:

```c
// C11 — secret lives inside, encrypted under KEK
struct ForetiasPrivKey {
    uint8_t encrypted_key[48];  // 32 ciphertext + 16 MAC
    uint8_t nonce[24];
    uint8_t public_key[32];     // Public info only
    ForetiasCurve curve;
};
```

```rust
// Rust — opaque handle, no access to internals
pub struct PrivKeyHandle(ManuallyDrop<NonNull<ForetiasPrivKey>>);
// No Deref, no Debug, no Clone, no Copy
```

### Zeroization

- **C11:** `sodium_memzero()` on cleanup
- **Rust:** `zeroize::Zeroizing<T>` for owned secrets; explicit `fill(0)` for caller-owned buffers

### No Linguistic or Programmatic Access

- Rust code cannot access raw key bytes — the `PrivKeyHandle` has no `Deref` implementation
- The KEK lives only in C file-scope static memory
- No path exists from Rust to the plaintext secret
- All cryptographic operations happen inside C; the higher language never sees raw bytes

---

## Level 2 — Component Signing Authority

Each component (Chronomatter, Calendar, Communerd) owns its own signing key and signs only what it produced.

### Signing Boundary Principle

```
Internal calls (Calendar → Chronomatter) → no signing needed
Calendar hands result to CommunerdetteLine → Calendar signs with Calendar's key
Chronomatter hands result to CommunerdetteLine → Chronomatter signs with Chronomatter's key
```

- **Internal calls** between components in the same family do NOT need signing — they're within the same trust boundary
- **Signing happens at the external boundary:** just before transmitting to `CommunerdetteLine` for external transmission
- **Signing key ownership:** the signing key stays with the time being (component) that has the corresponding TBID
- **Signing initiation:** Communerdette requests signing when external transmission is needed. The producing component signs with its own key at that point

### What Signing Guarantees

When a component signs a response, it is **guaranteeing the integrity and authenticity of that response**. The signature proves:

1. The response was produced by this component (not forged by another)
2. The response has not been tampered with since signing
3. The component stands behind the content of the response

Between family members (internal calls), there is implicit trust — no signature is needed because the components are within the same trust boundary.

---

## Level 3 — Type-Guarded Records (Take 3)

Foretias enforces a three-stage type progression for all inbound data. The compiler enforces that data flows through this progression — you cannot skip steps.

### The Three Types

```
Unprocessed<X>  →  CleanAuthenticated<X>  →  Externalized<X>
(parsed,        →  (authenticated +       →  (wire/disk format,
 untrusted)         cleansed, trusted)          minimal fields)
```

### Where Each Type May Appear

| Type | Allowed Locations | Forbidden Locations |
|------|------------------|---------------------|
| **`Unprocessed<X>`** | Communerd inbound handlers ONLY, unit tests | Chronomatter, Calendar, core-engine domain logic |
| **`CleanAuthenticated<X>`** | Chronomatter, Calendar, core-engine domain logic, intra-family communication | Never at wire/disk boundaries |
| **`Externalized<X>`** | Communerd outbound (wire), Calendar storage (disk) | Never in domain logic or intra-family communication |

### Why This Matters

- `Unprocessed<X>` is **untrusted** — it came from the network or disk and may be malicious
- `CleanAuthenticated<X>` is **trusted** — it has passed cryptographic verification
- `Externalized<X>` is **wire/disk format** — minimal fields, no runtime context

The compiler refuses any code path that consumes untrusted inbound data as if it were safe. This is not a convention — it is enforced by private constructors and type constraints.

### Constructor Discipline

`CleanAuthenticated<X>` has **private constructors**. The only two gates are:

- `into_clean_authenticated()` — inbound gate (Unprocessed → CleanAuthenticated, requires verification)
- `from_trusted()` — local gate (domain type → CleanAuthenticated, for data created locally)

Never construct `CleanAuthenticated<X>` directly. The compiler enforces this.

### Code Review Checklist

Before approving any Rust changes, verify:
- **No `Unprocessed<X>`** escapes Communerd or test code
- **No `Externalized<X>`** appears in domain logic (Chronomatter, core-engine)
- **No direct `CleanAuthenticated<X>` construction** outside `clean_auth.rs` (private constructors)
- **No raw domain types** (`ChrononRecord`, `Foretis`, etc.) at trust boundaries — always wrapped

---

## Level 4 — Component Trust Boundaries

### Communerd as the Only Network Authority

- Calendar and Chronomatter never receive raw transport objects, PeerPool mutation access, swarm command channels, or `&Communerd` directly
- They obtain `CommunerdetteLine` handles via `Communerd::line_for_tbid(tbid)`
- `CommunerdetteLine` is the narrow capability handle for all TBID-scoped network services

### Take 3 Inbound Gate

Raw remote replies enter as untrusted bytes and exit as `CleanAuthenticated<R>` only after:
1. `Unprocessed<R>::verify(...)` — cryptographic verification
2. TBID-match check — confirms the record belongs to the expected TBID

Calendar may NOT store remote replies as trust-bearing evidence except via this path.

### Binding Status vs Transport Identity

- `TbidBindingStatus::ClaimedByDht` is a discovery hint (transport identity claimed but not proven)
- `Verified` requires an application-level binding proof
- Until binding proof is implemented, trust-bearing code must not treat a DHT claim as a verified TBID binding

---

## Level 5 — Secret Material Lifecycle

### HR-1: All Long-Lived Secrets Encrypted in Memory

Every secret key that exists in memory beyond a single immediate operation must be stored in encrypted form. "In memory" means heap allocations, stack variables that outlive their producing function, struct fields, `Vec` buffers, `HashMap` values, global state, and any FFI-shared struct.

### HR-2: All Ephemeral Secrets Zeroed Immediately After Use

Every secret that exists only transiently must be zeroed before the producing function returns or the enclosing scope exits.

### HR-3: Secret-Holding Types Must NOT Leak Through the Type System

Any Rust type whose fields hold secret bytes must NOT derive `Debug`, `Clone`, `Copy`, or `Serialize` unless explicitly justified.

### HR-4: Constant-Time Comparison of Secret-Derived Values

Any comparison of secrets, signatures, authentication tags, MACs, key shares, or any byte sequence whose comparison-timing could leak information about a secret MUST use a constant-time comparison primitive.

### HR-5: No Logging, Tracing, or Display of Secret Material

Secrets must never be emitted to any log, trace, stdout/stderr, error message, panic message, or display formatter.

---

## Summary: The Security Stack

```
┌─────────────────────────────────────────────────┐
│  Level 5: Secret Material Lifecycle             │  HR-1 through HR-5
│  (encryption, zeroization, no-leak, const-time) │
├─────────────────────────────────────────────────┤
│  Level 4: Component Trust Boundaries            │  Communerd = only network authority
│  (CommunerdetteLine, Take 3 gate, binding)      │
├─────────────────────────────────────────────────┤
│  Level 3: Type-Guarded Records (Take 3)         │  Unprocessed → Clean → Externalized
│  (compiler-enforced progression)                │
├─────────────────────────────────────────────────┤
│  Level 2: Component Signing Authority           │  Each component signs its own work
│  (sign_response, signing boundary, key owner)   │
├─────────────────────────────────────────────────┤
│  Level 1: C11 Core: Secret Key Protection       │  KEK, opaque pointers, zeroization
│  (no linguistic/programmatic access to secrets) │
└─────────────────────────────────────────────────┘
```

Each level builds on the one below. The C11 core protects the raw keys. Component signing ensures authenticity. Type-guarded records prevent trust bypass. Component boundaries prevent unauthorized network access. Secret lifecycle rules prevent accidental exposure.
