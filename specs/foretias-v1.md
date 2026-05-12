# Foretias v1 — Product and Technical Specification

**Free and Open-source Resilient Time Integrity Attestation Service**

*Version 1.1 — 2026-04-20*

---

## 1. Product Overview

### 1.1 The Problem

Any party with access to a system clock can set the time backward. Any party holding a private key can sign data at any time, claiming any moment. This means **digital timestamps are inherently forgeable**: there is no cryptographic guarantee that a timestamp was produced at the moment it claims, only that someone with the right keys signed it.

### 1.2 The Foretias Guarantee

Foretias makes backdating impossible not by policy, but by cryptographic construction. A Foretias timestamp (a **Foretis**) binds content to a tick number in a time being's calendar. Each tick has its own Ed25519 keypair. When a time being advances from tick n to tick n+1, it **destroys the private key for tick n**. After destruction, no valid signature for tick n can ever be produced again.

The guarantee is simple:

> `Foretis(content)` always returns a tick number >= any previously issued tick. It is structurally impossible to produce a Foretis that appears to be from the past.

### 1.3 What Foretias v1 Is

Foretias v1 is a **Python library** that provides the core time being: a self-sovereign entity that stamps content and verifies stamps through its own local chronon chain. This time being can be instantiated within your own system to record when things happened.

---

## 2. Time Being (Latin: *Chronos fidelis*)

### 2.1 Time Family (Latin: *Chronos adunatrix*)

A **time family** is the conceptual entity that manages temporal integrity. In the v1 implementation, this role is split across three collaborating classes: `Chronomatter`, `Calendar`, and `TimeFamily`. See Section 2.7 for the architecture.

A **time being** is a computational entity devoted to maintaining temporal integrity. It has a persistent identity, an internal clock, and a way of proving Foretises that it issued for a piece of data was signed at the time of the foretis.

**Attributes:**

- **Genetic features** (set at creation, never change):
  - `tbid` (bytes): opaque internal identity — UUID v4, unique per TimeBeing.
  - `tbn` (str): human-readable external name, formatted as `"Time Family {tbid.hex()}"`.
  - `chronon_ns` (float): fixed tick interval in nanoseconds.
  - `serialized` (bool): whether the time being computes sparse ticks or only when it stamps.
- **Epigenetic information** (changes over time, mutex-protected):
  - `tick` (uint64): current tick counter, counting nanoseconds since Unix epoch.
  - `calendar` (list[TickRecord]): append-only log of all ticks.
- **Runtime secret** (in-memory only):
  - The current private key (Ed25519).

**Lifecycle:**

1. **Created** with `tbid`, `tbn`, `chronon`, and `serialized`. Generates its first keypair for tick 0. The calendar is initialized with one record: `tick_number` = 0, `public_key` = the new public key, `forward_foretis` and `backward_foretis` are computed from an existing chrononchain auto-attestation (auto-attestation(genesis, genesis)).
2. **Advances ticks**:
   - If `serialized = False`: a background daemon thread calls `tick()` every `chronon_ns` nanoseconds. `stamp()` simply signs under the current tick.
   - If `serialized = True`: no background thread. `tick()` advances only when `stamp()` is called, and only one caller is permitted at a time (mutex-protected).
3. **Stamps content** under the current tick's private key.
4. **Dies** when its Python process ends. The calendar is persisted to disk for future resurrection. The private key is never persisted — a resurrected time being can only verify, never stamp.

**States:**

- **Active**: freshly created, possesses the current private key. Can `stamp()` and `verify()`.
- **Dormant**: loaded from a calendar file on disk. No private key available. Can only `verify()`.

**Invariants:**

- A time being never produces two different signatures for the same `(tick, content_hash)` pair.
- No entry in the calendar has `forward_foretis = None` or `backward_foretis = None`. Genesis records an existing chrononchain; every other record records a transition from its predecessor.
- All consecutive pairs in the calendar are auto-attesting via `backward_foretis` and `forward_foretis`, verifiable using their respective public keys.
- Public keys can only attest to Foretises signed after their own start boundary. The existence of a subsequent tick defines the end of the period during which a Foretis could have taken place.

### 2.2 Tick: the Foretias Self-Attestation Methodology

A **tick** is a discrete duration of time in the Foretias protocol. Each tick has its own Ed25519 keypair. Ticks form a numbered chain: tick 0, tick 1, tick 2, ...

Ticks are numbered from genesis; each `tick_number` represents nanoseconds past the Unix epoch. The `chronon_ns` is adhered to for non-serialized time beings at best effort — system load and clock synchronization may cause slight drift.  Nanoseconds are a practical convenience for v1 (fits in a float with sub-nanosecond jitter); not a fundamental limit of the protocol.

**The tick: Foretias Auto-Attestation:**

A tick occurs between two chronon. For clarity sake, Let's call the chronon `OLD`, and `NEW`. `OLD` has duration preceeds NEW entirely.
For the old chronon, we have it's begining `OLD.tick_number`, we also have `OLD.private_key`, `OLD.secrete_key`:

1. Determine and store `NEW.tick_number`, roughly the current unix epoch nanoseconds.
2. Generate keypair `(NEW.public_key, NEW.secret_key)`
3. Generate 16-byte RNG nonce (via `foretias_rng_bytes` in C11).
4. Compute the `auto_attestation_blob=concat(TBID, OLD.tick_number, OLD.public_key, NEW.tick_number, NEW.public_key, nonce)`.
5. Compute **forward auto-attestation** by signing the `auto_attestation_blob` from the old chronon:
   `forward_foretis = _stamp(content=auto_attestation_blob, tbid=TBID, tick_number=OLD.tick_number, private_key=OLD.private_key)`
   This proves the old time being acknowledges that NEW follows it.
6. Compute **backward auto-attestation** by signing `auto_attestation_blob` with the new private key:
   `backward_foretis = _stamp(content=auto_attestation_blob, tbid=TBID, tick_number=NEW.tick_number, private_key=NEW.private_key)`
    This proves the new key acknowledges that it follows the old chronon.
7. Append the new tick record to the calendar: `TickRecord(NEW.tick_number, NEW.public_key, forward_foretis, backward_foretis, nonce)`.
8. Destroy `old_sk` — it is never held in memory again.
9. The new keypair becomes the active signing key.

**Why this matters:**

- Forward auto-attestation (`forward_foretis`) answers: "Can the new key sign for itself?" — proving the key was actively created at this tick.
- Backward auto-attestation (`backward_foretis`) answers: "Does the new key acknowledge the old key?" — establishing the chain from the new end.
- Neither signature requires the old private key. This is critical: after `cur_sk` is destroyed, a successor can independently verify the entire transition.

**Auto-Attestation Semantics:**

**Auto-attestation** is the mechanism a time being uses to continue its own clock. When a tick advances, the old private key is destroyed and a new keypair generated. The new tick record contains `forward_foretis` (signed by the old key) and `backward_foretis` (signed by the new key) — both over the same auto-attestation blob. This proves both keys acknowledge the transition.

Auto-attestation only applies when the TBID is the **same** — i.e., the time being is continuing its own clock. When TBID is different, the signatures represent a different relationship between entities and are **not** called "auto-attestation".

**TickRecord:**

```python
@dataclass
class TickRecord:
    tick_number: uint64      # Nanoseconds since Unix epoch; the tick's time boundary.
    public_key: bytes        # Ed25519 public key for this tick.
    forward_foretis: bytes    # Auto-attestation signed by prev_sk; genesis signs itself.
    backward_foretis: bytes   # Auto-attestation signed by self_sk. Never None.
    aa_nonce: bytes          # RNG nonce (16 bytes) for replay protection.
```

### 2.3 Calendar (Latin: *Chronos graphus*)

A **calendar** is the append-only log of a time being's chronon chain. It stores every tick's public key and transition proof in sequential order.

**Operations:**

- `append(tick_number, public_key, forward_foretis, backward_foretis)` — add a new tick record.
- `get(tick_number, count)` — retrieve the earliest `count` ticks at or after `tick_number`. Returns list[TickRecord].
- `latest()` → `int` — the highest tick number.
- `integrity_check()` → `(bool, list[int] | None)` — checks all consecutive pairs via `_verify_pair()`. If `return_failures` is False (default), returns just `bool`. If True, returns `(bool, list_of_failed_indexes)`.

**Persistence format (calendar.json):**

```json
{
  "tbid": "hex-encoded UUID",
  "tbn": "Time  being abc123...",
  "stamp_tbid": "hex-encoded UUID",
  "ticks": [
    {
      "tick_number": 0,
      "public_key": "hex-encoded Ed25519 public key",
      "forward_foretis": "hex-encoded signature",
      "backward_foretis": "hex-encoded signature"
    },
    {
      "tick_number": 1697000000000000000,
      "public_key": "hex-encoded Ed25519 public key",
      "forward_foretis": "hex-encoded signature",
      "backward_foretis": "hex-encoded signature"
    }
  ]
}
```

**Persistence rules:**

- All byte arrays are stored as hex-encoded strings.
- `tick_number` is an integer.
- On load, the calendar validates:
  - All `tick_number` values are non-negative.
  - `tick_number` values are strictly ascending.
  - All hex strings parse correctly.
  - Full chain integrity check via `integrity_check()`.
- Calendar is persisted to: `{persist_path}/{tbid}/calendar.json`.

### 2.4 Chronomatter (Latin: *Chronos authenticus*)

A **Chronomatter** (*Chronos authenticus*, the Time Authority) is the active entity that manages ticking an internal clock, and stamping requests. It so by managing Ed25519 key lifecycle, performs stamping and ticking, and publishes ticks to all attached Calendars. It is a standalone class, separate from Calendar storage.

The Chronomatter creates its own genesis tick record for internal tracking, then publishes an *adapted genesis* per Calendar: Calendar.tbid + Chronomatter's current public key + self-signatures. Note the word can be pronounced /kruh-noh-MAY-ter/.

**Input:**

- `content` (str | bytes): the data to timestamp.
- `tick_number` (uint64): current tick, nanoseconds since Unix epoch.
- `public_key` (bytes): active tick's public key.
- `private_key` (bytes): active tick's private key.
- `tbid` (bytes): time being's identity.

**Process:**

1. Hash the content: `content_hash = SHA-256(content)`.
2. Concatenate the signature input: `signature_input = concat(tbid, tick_number, content)`.
3. Sign: `signature = Ed25519_sign(signature_input, private_key)`.
4. Return a `Foretis` containing `(tick_number, content_hash, signature, tbid, echo, tbn)`.

**Functional implementation:**

```python
def _stamp(content, tbid, tick_number, public_key, private_key) -> Foretis:
    """Pure function. No side effects. Returns Foretis: `signature = Ed25519_sign(concat(tbid, tick_number, content), private_key)`."""
    ...
```

### 2.5 Foretis

A **Foretis** (Foretias TimeStamp) is the stamped artifact — the cryptographically signed proof that content existed at a specific tick.

```python
@dataclass
class Foretis:
    tick_number: uint64
    my_content_hash: bytes
    signature: bytes
    tbid: bytes
    echo: str
    tbn: str
```

**Field semantics:**

- `tbid` (bytes): the time being's internal UUID v4 identity, used for cross-timebeing references.
- `tbn` (str): the human-readable external name of the time being, formatted as `"Time Being {tbid.hex()}"`.
- `echo` (str): a pass-through of the original content string sent to `stamp()`. It is echoed back from the requester and is **not** part of the cryptographic signature input.

A Foretis is self-contained. To verify it, a verifier needs the Foretis plus access to the time being's calendar (to look up the public key for the claimed tick).

### 2.6 Time Family (Latin: *Chronos adunatrix*)

The **Time Family** (*Chronos adunatrix*, the messenger) is the orchestrator that coordinates between `Chronomatter` (the Time Authority) and `Calendar` (passive tick storage). It provides the user-facing API, delegating to the Chronomatter for stamping/ticking and to the Calendar for storage.

**Architecture: three collaborating entities**

| Entity | Latin Name | Responsibility |
|--------|-----------|----------------|
| **Chronomatter** | *Chronos authenticus* | Key lifecycle, stamping, ticking, thread management |
| **Calendar** | *Chronos graphus* | Passive tick storage, persistence, integrity checks |
| **Time Family** | *Chronos adunatrix* | Orchestrator, settings, coordinates Chronomatter + Calendar(s) |

**Data flows:**
- `stamp(content)` → Chronomatter signs, returns Foretis (NOT stored in Calendar)
- `tick()` → Chronomatter advances, publishes TickRecord to all attached Calendars
- `verify(content, foretis, calendar)` → Chronomatter uses Calendar's ticks for key lookup
- Calendar creates its own genesis via Chronomatter's adapted genesis (Calendar.tbid + Chronomatter's key)

**Verification process:**

1. Extract `tick_number`, `content_hash`, and `signature` from the Foretis.
2. Look up the public key for `tick_number` in the calendar using `calendar.get(tick_number, 1)`.
3. Verify `SHA-256(content) == content_hash`.
4. Verify the Ed25519 signature against the public key.
5. If a `next_tick_number` is provided, find the exact record matching next_tick_number from calendar and perform `_verify_pair` on the calendar entries for tick_number and next_tick_number.
6. Return results.

In v1, the same TimeFamily instance can act as both chronomatter and inquirer.

**Return type:**

```python
def verify(content, foretis, next_tick_number: uint64 | None = None) -> bool | tuple[bool, bool | None]:
    """
    If next_tick_number is None:
        Returns bool: True if signature and content hash are valid.
    If next_tick_number is not None:
        Returns (sig_valid, window_closed):
            sig_valid: True if signature and content hash are valid.
            window_closed: True if next tick exists (key destroyed), False if not (still active).
                           None if sig_valid is False.
    """
    ...
```

---

## 3. Technical Overview

### 3.1 Cryptographic Primitives

| Function | Algorithm | Rationale |
|----------|-----------|-----------|
| Hashing | SHA-256 (FIPS 180-4) | Universally available, collision-resistant |
| Digital Signatures | Ed25519 (RFC 8032) | Fast keygen, short keys, no parameter choices |

### 3.2 Chronomatter Verification

To verify a Foretis:

1. **Signature check**: The Ed25519 signature in the Foretis must verify against the public key published for the claimed tick in the calendar.
2. **Content check**: `SHA-256(provided_content)` must equal the `content_hash` in the Foretis.
3. **Window check** (optional): If a `next_tick_number` is provided, verify that the tick's `forward_foretis` is valid — proving a subsequent tick exists and the current key has been destroyed.

If the signature and content checks pass, the Foretis is valid. The window check additionally proves the key is gone, making backdating cryptographically impossible.

### 3.3 Chain Verification

The chronon chain provides temporal ordering and continuity guarantees:

- **Pair verification** (`_verify_pair(A, B)`): Given two consecutive tick records A and B, verify both cross-stamp signatures:
  - `forward_foretis` of B verifies against A's public key.
  - `backward_foretis` of B verifies against B's public key.
- **Chain verification** (`_verify_chain(ticks, return_failures=False)`): Iterates `_verify_pair()` across the entire chain. Returns `(bool, list[int] | None)`. If `return_failures=True`, returns the indexes of failed pairs.

The calendar loader runs full chain verification on load. The CLI `foretis verify` command also runs chain verification.

---

## 4. Library Specification

### 4.1 Module Structure

```
foretias/
  __init__.py        # Package init, exports TimeFamily, Chronomatter, Foretis, Config
  chronomatter.py    # Chronomatter class (Chronos authenticus): key lifecycle, stamping, ticking
  calendar.py        # Calendar class (Chronos grapha): load, save, lookup, chain verify
  timefamily.py      # TimeFamily class (Chronos adunatrix): orchestrator
  _time.py           # Pure functional: _tick(), _stamp(), _verify(), _genesis_ma()
  crypto.py          # Ed25519 primitives + SHA-256
  models.py          # TickRecord, Foretis dataclasses
  config.py          # Config dataclass for persistence path resolution
  cli.py             # `foretis verify --calendar --file --foretis` CLI tool
```

### 4.2 Public API

```python
from foretias import TimeFamily, Chronomatter, Foretis, Config

# Create a time being
tbf = TimeFamily(name="alpha", chronon_ns=60_000_000_000.0)

# Stamp content
foretis = tbf.stamp("my message")

# Verify content against stamp (no chain check)
valid = tbf.verify("my message", foretis)  # True

# Verify with chain check
valid, window_closed = tbf.verify("my message", foretis, next_tick_number=1)
# valid=True, window_closed=True (tick 0's key is destroyed)

# Load from disk (dormant mode — verify only)
tbf2 = TimeFamily.load(persist_path="/path/to/foretias/")
valid = tbf2.verify("my message", foretis)  # True, but cannot stamp
```

### 4.3 Config

```python
@dataclass
class Config:
    """Persistence path resolution: arg > env var > default."""
    persist_path: str  # Resolved path for calendar storage

    @classmethod
    def resolve(cls, persist_path: str | None = None) -> Config:
        """
        Resolution order:
        1. persist_path argument (highest priority)
        2. $FORETIAS_HOME environment variable
        3. Default: ~/.foretias/
        """
        ...
```

### 4.4 TimeFamily

The `TimeFamily` is the main public class. It encapsulates the time being's identity, calendar, threading, and persistence.

**Constructor:**

```python
TimeFamily(
    name: str = "timebeing",         # TBN prefix — final name becomes "Time Being {tbid.hex()}"
    chronon_ns: float = 60_000_000_000.0,
    tbid: bytes | None = None,       # Auto-generated UUID v4 if None
    serialized: bool = False,        # Tick only advances on stamp
    persist_path: str | None = None, # Path resolved via Config
)
```

**Internal architecture:**

`TimeFamily` creates a `Chronomatter` and a `Calendar`, attaches the Calendar to the Chronomatter, and delegates all operations. Users interact only with `TimeFamily`.

**Methods:**

| Method | Signature | Returns | Description |
|--------|-----------|---------|-------------|
| `stamp` | `(content: str \| bytes) -> Foretis` | Foretis | Sign content under current tick key. For serialized timebeings, advances tick before signing. |
| `verify` | `(content, foretis, next_tick_number=None) -> bool \| (bool, bool \| None)` | bool or tuple | Verify signature, content hash, and optionally window closed. |
| `current_tick` | `() -> int` | int | Current tick number. |
| `tick` | `() -> None` | None | Advance the calendar to next tick. For non-serialized: background thread calls this. |
| `get` | `(tick_number: int, count: int) -> list[TickRecord]` | list[TickRecord] | Return earliest `count` ticks at or after `tick_number`. |
| `save` | `() -> None` | None | Persist calendar to disk. |
| `load` | `(persist_path=None) -> TimeFamily` | TimeFamily | Class method. Load calendar from disk. Returns a dormant time being (no private key). |

**Threading:**

- A `threading.Lock` protects all state mutations.
- Non-serialized timebeings spawn a `threading.Thread` daemon in `__init__` that runs `tick()` every `chronon_ns` nanoseconds.
- The daemon thread acquires the lock, calls `_tick()`, mutates state, persists.

**Functional separation:**

All cryptographic logic is in pure functions (no side effects) inside `timebeing.py`. The TimeFamily manages mutation/disk/entropy and other runtime concerns.

### 4.5 Chronomatter Class

The `Chronomatter` (*Chronos authenticus*) is the active entity managing key lifecycle, stamping, ticking, and thread coordination. It is in-memory only — never persisted to disk.

**Constructor:**

```python
Chronomatter(
    name: str = "timebeing",
    chronon_ns: float = 60_000_000_000.0,
    tbid: bytes | None = None,
    serialized: bool = False,
    persist_path: str | None = None,
)
```

**Methods:**

| Method | Signature | Returns | Description |
|--------|-----------|---------|-------------|
| `stamp` | `(content: str \| bytes) -> Foretis` | Foretis | Sign content under current tick key. Returns Foretis to caller (not stored). |
| `verify` | `(content, foretis, calendar) -> bool` | bool | Verify signature and content hash using calendar for key lookup. |
| `tick` | `() -> None` | None | Advance to next tick, publish to all attached Calendars. |
| `get` | `(tick_number: int, count: int) -> list[TickRecord]` | list[TickRecord] | Return ticks by Chronomatter's internal counter. |
| `attach_calendar` | `(calendar: Calendar) -> None` | None | Attach a Calendar, publish adapted genesis. |
| `save` | `() -> None` | None | No-op. Only the Calendar is persisted to disk. |
| `shutdown` | `() -> None` | None | Stop daemon thread, wait. |

**Properties:**

| Property | Type | Description |
|----------|------|-------------|
| `tbid` | `bytes` | Internal identity (UUID v4 bytes). |
| `tbn` | `str` | Human-readable name. |
| `serialized` | `bool` | Whether tick advances only on stamp. |
| `chronon_ns` | `float` | Tick interval in nanoseconds. |
| `active` | `bool` | True if holding current private key. |
| `current_tick` | `int` | Chronomatter's internal sequential counter. |

### 4.6 Calendar Class

```python
class Calendar:
    ticks: list[TickRecord]

    def __init__(self, tbid, tbn, ticks=None, stamp_tbid=None):
        """Create calendar. stamp_tbid defaults to tbid; set when Chronomatter attaches for integrity checks."""
        ...

    def append(self, tick_record: TickRecord) -> None:
        ...

    def get(self, tick_number: int, count: int) -> list[TickRecord]:
        """Return earliest `count` ticks at or after `tick_number`."""
        ...

    def latest(self) -> int | None:
        """Return the highest tick number, or None if empty."""
        ...

    def load(path: str) -> Calendar:
        """Load from JSON file. Validates ascending tick_numbers, hex parse, chain integrity."""
        ...

    def save(self, path: str) -> None:
        """Save to JSON file. Hex-encodes all byte arrays."""
        ...

    def integrity_check(self, return_failures: bool = False) -> bool | tuple[bool, list[int] | None]:
        ...
```

---

## 5. Testing

### 5.1 Functional Tests (`test_functional.py`)

Direct unit tests of the pure functions:

- `_tick()` returns correct cross-stamp signatures.
- `_stamp()` produces verifiable Foretis.
- `_verify_pair()` accepts valid pairs, rejects mismatched pairs.
- `_verify_chain()` accepts full chains, rejects chains with broken links. `return_failures=True` returns correct indexes.
- `_verify()` with and without `next_tick_number`.

### 5.2 Calendar Tests (`test_calendar.py`)

- `load()` / `save()` roundtrip with hex encoding.
- `get(tick_number, count)` returns correct subset.
- `integrity_check()` validates correct chains.
- Invalid JSON (malformed hex, non-ascending tick_numbers) raises on load.

### 5.3 Timebeing Tests (`test_timebeing.py`)

- Active time being: can stamp and verify.
- Dormant time being: can verify, cannot stamp (raises).
- `serialized=True`: stamp triggers tick.
- `serialized=False`: stamp does not trigger tick; background thread does.
- `get(tick_number, count)` returns correct records.
- Mutex safety: concurrent stamp calls do not corrupt state.

### 5.4 Persistence Tests (`test_persistence.py`)

- Create time being → stamp → `save()` → destroy object → `load()` → verify dormant.
- Verify that dormant time being cannot `stamp()`.

### 5.5 Stress Tests (`test_stress.py`)

- Fixed random seed for reproducibility.
- Generate 1000+ ticks.
- Stamp 100+ random messages.
- Verify each stamp live.
- Verify tampered messages fail.
- Write calendar to `/tmp`.
- Re-read calendar from `/tmp`.
- Re-verify all previous stamps after reload.

---

## 6. CLI Tool

### 6.1 Command

```bash
foretis verify --calendar calendar.json --file message.txt --foretis foretis.json
```

### 6.2 Behavior

1. Load calendar from `calendar.json` using the same JSON parser as the library.
2. Read message content from `message.txt`.
3. Load Foretis from `foretis.json` (same format as `calendar.json`, hex-encoded fields).
4. Run `verify(message, foretis)`.
5. Print result: `valid` or `invalid` with reason.

---

## 7. Installation

```bash
pip install foretias
```

**Dependencies:** `cryptography>=42`

---

## 8. Quick Start

```python
from foretias import TimeFamily

# Create a time being with 1-minute ticks
tbf = TimeFamily(name="alpha", chronon_ns=60_000_000_000.0)

# Stamp your first message
foretis = tbf.stamp("hello world")

# Verify it
is_valid = tbf.verify("hello world", foretis)
print(is_valid)  # True

# Verify with window check (requires next tick to exist)
is_valid, window_closed = tbf.verify("hello world", foretis, next_tick_number=1)
print(is_valid, window_closed)  # True, True
```

---

## 9. The Minimal Correct Pattern

```python
tbf = TimeFamily()
foretis = tbf.stamp("my message")
assert tbf.verify("my message", foretis)  # True
```

This is the core loop. Stamp content, keep the foretis alongside the content, verify whenever needed.
