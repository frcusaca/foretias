# Fortias

**Free, Open-source and Resilient Time Integrity Attestation Service**

Fortias attests to digital happenings. The project develops a suite of principles, algorithms, libraries, and deployable hardware and software components for time integrity attestation. The entire project is open-source and freely available.

## Installation

```bash
pip install fortias
```

## Quick Start

```python
import hashlib
from fortias import Fortias
from fortias.calendar import Calendar
from fortias.crypto import generate_keypair
from fortias.models import StampRequest, VerifyRequest
from fortias.timestamp import TimestampFactoryV0

# 1. Generate an Ed25519 keypair (32 bytes each)
private_key, public_key = generate_keypair()

# 2. Create a calendar and publish the key at a point in time
calendar = Calendar()
calendar.tick(
    TimestampFactoryV0.make(year=2026, month=1, day=1),
    public_key,
)

# 3. Create the Fortias service
fortias = Fortias()

# 4. Stamp a payload
payload = b"proof of this message at 2026-04-19"
payload_hash = hashlib.sha256(payload).hexdigest()
stamp_response = fortias.stamp(
    StampRequest(payload=payload, stamp_request_hash=payload_hash)
)

# 5. Verify the stamp
verify_response = fortias.verify(
    VerifyRequest(
        payload=payload,
        stamp=stamp_response,
        stamp_request_hash=payload_hash,
    )
)
assert verify_response.valid is True
```

## API Reference

### Import

```python
from fortias import Fortias, __version__
from fortias.calendar import Calendar
from fortias.crypto import generate_keypair
from fortias.models import StampRequest, VerifyRequest, StampResponse, VerifyResponse
from fortias.timestamp import TimestampFactoryV0
from fortias.exceptions import InvalidKeyError, PayloadHashMismatchError
```

### Core Class: `Fortias`

The main entry point. Owns an Ed25519 signing key and a `Calendar`, exposing two protocol operations.

#### `Fortias(secret_key: bytes, calendar: Calendar)`

**Args:**
- `secret_key` — 32-byte raw Ed25519 private key. Must be exactly 32 bytes.
- `calendar` — A `Calendar` instance used by `verify()` to resolve the authoritative public key at a stamp's timestamp.

**Raises:** `InvalidKeyError` if `secret_key` is not 32 raw bytes.

#### `fortias.stamp(request: StampRequest) -> StampResponse`

Signs a payload with Ed25519. Returns a stamp containing the signature, a UUID4 correlation ID (TBID), and a nanosecond-precision timestamp.

**Pre-check:** `SHA-256(payload)` must equal `request.stamp_request_hash`. If it does not, returns a `StampResponse` with `status="abnormal: ..."` and an empty signature.

**Returns:** `StampResponse`

```python
@dataclass(frozen=True, kw_only=True)
class StampResponse:
    TBID: str                          # UUID4 correlation ID
    fortias_version: str               # "0.0.1"
    fortias_timestamp: str             # ISO-8601, nanosecond precision, e.g. "2026-04-19T14:30:00.500000000Z"
    stamp_request_hash: str            # SHA-256 hex digest of the signed payload
    signature: str                     # Hex-encoded 64-byte Ed25519 signature over the raw payload
    echo: Any | None = None            # Optional opaque value, round-tripped from request
    status: str = "normal"             # "normal" or "abnormal: <message>"
```

#### `fortias.verify(request: VerifyRequest) -> VerifyResponse`

Verifies a stamp against the original payload. Runs four checks in order:

| # | Check | What it validates |
|---|-------|-------------------|
| 1 | `version_check` | Stamp's `fortias_version` equals protocol version `"0.0.1"` |
| 2 | `payload_integrity` | `SHA-256(payload)` matches the stamp's `stamp_request_hash` |
| 3 | `calendar_lookup` | A calendar tick exists with `tick_ts ≤ stamp.fortias_timestamp` |
| 4 | `signature` | Ed25519 verification under the public key from the calendar tick found in (3) |

**Pre-check:** `SHA-256(payload)` must equal `request.stamp_request_hash`. If it does not, returns `VerifyResponse` with `valid=False`, `results=()`, and `status="abnormal: ..."`.

**Unparseable timestamp** (e.g. `fortias_timestamp="not-a-timestamp"`) also produces an abnormal response.

**Returns:** `VerifyResponse`

```python
@dataclass(frozen=True)
class VerifyResult:
    name: str    # "version_check", "payload_integrity", "calendar_lookup", or "signature"
    passed: bool # True if this check passed
    detail: str  # Human-readable description

@dataclass(frozen=True, kw_only=True)
class VerifyResponse:
    TBID: str                          # UUID4 correlation ID for this verification
    fortias_version: str               # "0.0.1"
    fortias_timestamp: str             # When verification completed
    stamp_request_hash: str            # The validated hash, echoed back
    valid: bool                        # True iff ALL checks in results passed
    results: tuple[VerifyResult, ...]  # Ordered per-check outcomes
    echo: Any | None = None            # Echoed from the request
    status: str = "normal"             # "normal" (ran to completion) or "abnormal: ..." (stopped early)
```

**Status semantics:**
- `status="normal"` + `valid=True` — all checks passed, stamp is valid
- `status="normal"` + `valid=False` — verification ran to completion, but one or more checks failed (e.g. tampered payload, wrong key)
- `status="abnormal: ..."` — verification could not run (hash mismatch, unparseable timestamp)

### Data Models

#### `StampRequest`

Input to `/stamp`.

```python
@dataclass(frozen=True, kw_only=True)
class StampRequest:
    payload: bytes                     # Raw bytes to be stamped
    stamp_request_hash: str            # Hex SHA-256 of payload (client-supplied, server-revalidated)
    echo: Any | None = None            # Optional opaque value, round-tripped into response
```

#### `VerifyRequest`

Input to `/verify`.

```python
@dataclass(frozen=True, kw_only=True)
class VerifyRequest:
    payload: bytes                     # The original stamped bytes
    stamp: StampResponse               # The StampResponse returned by /stamp, echoed back
    stamp_request_hash: str            # Hex SHA-256 of payload
    echo: Any | None = None            # Optional opaque value, round-tripped into response
```

### Calendar: `Calendar`

In-memory, monotonically ordered store of `(timestamp, public_key)` ticks. Acts as a time-indexed public-key registry.

#### `calendar.tick(timestamp: TimestampV0, verifier: bytes) -> None`

Publishes a public key as authoritative from `timestamp` onward.

**Monotonicity enforced:** `timestamp` must be strictly greater than every previously inserted timestamp. Equal or earlier timestamps are rejected.

**Args:**
- `timestamp` — `TimestampV0` strictly greater than the current head
- `verifier` — 32-byte raw Ed25519 public key

**Raises:** `ValueError` if monotonicity is violated.

#### `calendar.get_tick(timestamp: TimestampV0) -> tuple[TimestampV0, bytes] | None`

Returns the greatest tick with `tick_ts ≤ timestamp` (the **latest-key-at-time** policy).

O(log n) binary search on the sorted list.

**Example of the cutoff policy:**

```python
# Tick published at exactly 00:05:00
calendar.tick(TS(2026, 1, 1, 0, 5, 0), new_key)

# Stamp at 00:05:30 — still within the 5-minute window
calendar.get_tick(TS(2026, 1, 1, 0, 5, 30))
# → returns the 00:05:00 tick (key is valid)

# Stamp at 00:06:00 — clock has turned the next minute
calendar.get_tick(TS(2026, 1, 1, 0, 6, 0))
# → returns the NEXT tick (00:06:00), not the 00:05:00 one.
#   The 00:05:00 key is no longer authoritative.
```

### Timestamps: `TimestampV0`

Nanoseconds since Unix epoch stored as a `ctypes.c_uint64`. Range: 0 to 18,446,744,073,709,551,615 ns (≈ 1970–2554).

#### `TimestampFactoryV0`

Canonical factory for constructing `TimestampV0` instances.

```python
# From explicit calendar components
ts = TimestampFactoryV0.make(
    year=2026, month=1, day=1,
    hour=14, minute=30, second=0,
    microseconds=0, nanoseconds=0,
)

# From a datetime
from datetime import datetime, timezone
ts = TimestampFactoryV0.from_datetime(
    datetime.now(tz=timezone.utc),
    nanoseconds=0,
)

# From an ISO 8601 string
ts = TimestampFactoryV0.from_iso_string("2026-04-19T14:30:00.500000000Z")
```

#### `TimestampV0`

```python
ts.value              # int — raw nanosecond count
ts.to_iso_string()    # str — "2026-04-19T14:30:00.500000000Z"
ts.to_bytes()         # bytes — 8 big-endian bytes
ts.from_bytes(data)   # TimestampV0 — classmethod, deserialise from 8 bytes
```

### Cryptography: `crypto`

```python
from fortias.crypto import generate_keypair, sign, verify_signature, hash_payload_for_version, PROTOCOL_VERSION

# Generate keypair — returns (private_key_bytes, public_key_bytes), each 32 bytes
private_key, public_key = generate_keypair()

# Sign — returns hex-encoded 64-byte signature
signature_hex = sign(payload, private_key)

# Verify — returns True/False (never raises)
is_valid = verify_signature(payload, signature_hex, public_key)

# Hash — hex SHA-256 digest for protocol version
payload_hash = hash_payload_for_version(payload, PROTOCOL_VERSION)
```

**Note:** Ed25519 signs over the raw payload bytes directly (not over a pre-computed hash).

### Exceptions

```python
from fortias.exceptions import FortiasError, InvalidKeyError, PayloadHashMismatchError

# InvalidKeyError — raised when secret_key is not 32 valid Ed25519 bytes
try:
    Fortias(secret_key=b"too_short", calendar=Calendar())
except InvalidKeyError as exc:
    print(f"Key error: {exc}")

# PayloadHashMismatchError — raised internally when client-supplied hash doesn't match SHA-256(payload)
```

### Complete End-to-End Example

```python
import hashlib
from fortias import Fortias
from fortias.calendar import Calendar
from fortias.crypto import generate_keypair
from fortias.models import StampRequest, VerifyRequest
from fortias.timestamp import TimestampFactoryV0

# Setup: generate keys and publish to calendar
sk, pk = generate_keypair()
calendar = Calendar()
calendar.tick(
    TimestampFactoryV0.make(year=2026, month=1, day=1),
    pk,
)
server = Fortias(secret_key=sk, calendar=calendar)

# Stamp
payload = b"important document hash"
payload_hash = hashlib.sha256(payload).hexdigest()

stamp = server.stamp(StampRequest(
    payload=payload,
    stamp_request_hash=payload_hash,
    echo={"request_id": "abc-123"},
))

assert stamp.status == "normal"
assert stamp.fortias_version == "0.0.1"
assert len(bytes.fromhex(stamp.signature)) == 64  # 64-byte Ed25519 sig

# Verify — valid
result = server.verify(VerifyRequest(
    payload=payload,
    stamp=stamp,
    stamp_request_hash=payload_hash,
))
assert result.status == "normal"
assert result.valid is True
# result.results contains: version_check, payload_integrity, calendar_lookup, signature

# Verify — tampered payload
tampered_result = server.verify(VerifyRequest(
    payload=b"tampered document",
    stamp=stamp,
    stamp_request_hash=hashlib.sha256(b"tampered document").hexdigest(),
))
assert tampered_result.status == "normal"
assert tampered_result.valid is False
# payload_integrity and signature checks failed
```

## Development

This project uses the `alpha` branch as the center of development. It represents the most current and accepted branch of code.

## License

This project is licensed under the [BSD 3-Clause Clear License](LICENSE).
