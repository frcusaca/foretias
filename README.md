# Foretias

**Free, Open-source and Resilient Time Integrity Attestation Service**

Foretias makes digital timestamping backdate-proof by cryptographic construction. Each tick of a time being's calendar has its own Ed25519 keypair, and when a tick advances, the previous private key is destroyed. A Foretis stamped at tick *n* cannot be forged from the past.

## Installation

```bash
pip install foretias
```

## Quick Start

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

## CLI

### Verify (v0.1)

```bash
foretis verify --calendar calendar.json --file message.txt --foretis foretis.json
```

Add `--chain` to run a full chain integrity check on the calendar.

### Serve (v0.2)

Start a time family server with optional P2P peers:

```bash
# Basic server
foretias serve --addr 127.0.0.1:4001 --chronon 60000000000

# With P2P peers
foretias serve --addr 127.0.0.1:4001 --peer 127.0.0.1:4002 --mutual-attest-every-chronons 10

# Custom request timeout (default: 5 seconds)
foretias serve --addr 127.0.0.1:4001 --peer 127.0.0.1:4002 --request-timeout-secs 10
```

### Stamp & Verify via CLI (v0.2)

```bash
# Stamp a message against a running server
foretias stamp --message "hello world" --server 127.0.0.1:4001

# Verify a stamp
foretias verify --message "hello world" --foretis '{"tick_number":...}' --server 127.0.0.1:4001
```

### Inspect Attestations (v0.2)

Offline verification of external attestations stored in a calendar:

```bash
foretias inspect-attestations --calendar calendar.json
```

Re-runs signature, hash, and echo verification on every `ExternalAttestation`. Exits 0 if all valid, 1 if any invalid.

## API Reference

### Core class: `TimeFamily`

Time family (`TimeFamily`, *Chronos fidelius adunatrix*) orchestrates between chronomatters (`Chronomatter`, *Chronos fidelius authenticus*, the Time Authority) and calendars (`Calendar` ,*Chronos fidelius grapha*, storage).

```python
from foretias import TimeFamily, Chronomatter, Foretis, TickRecord, Config

tbf = TimeFamily(name="alpha", chronon_ns=60_000_000_000.0)
```

| Method | Returns | Description |
|--------|---------|-------------|
| `stamp(content)` | `Foretis` | Sign content under the current tick's key |
| `verify(content, foretis)` | `bool` or `(bool, bool)` | Verify a Foretis artifact |
| `current_tick()` | `int` | Current tick number |
| `tick()` | `None` | Advance to the next tick |
| `save()` | `None` | Persist calendar to disk |
| `load(persist_path)` | `TimeFamily` | Load a dormant (verify-only) instance |

### Data models

```python
@dataclass(frozen=True)
class TickRecord:
    tick_number: int     # Nanoseconds since Unix epoch
    public_key: bytes    # Ed25519 public key
    forward_foretis: bytes   # auto-attestation signed by prev_sk
    backward_foretis: bytes  # auto-attestation signed by self_sk
    aa_nonce: bytes        # RNG nonce (16 bytes) for replay protection
```

### Auto-Attestation

**Auto-attestation** is the mechanism a time being uses to continue its own clock.
When a tick advances from tick *n* to tick *n+1*, the old private key is destroyed and
a new keypair is generated. The new tick record contains two signatures over the same
auto-attestation blob (both tick numbers, both public keys, and a 16-byte RNG nonce):
`forward_foretis` (signed by the old key) and `backward_foretis` (signed by the new key).

Auto-attestation only applies when the TBID is the **same** — i.e., the time being is
continuing its own clock. When TBID is different, the signatures are no longer called
"auto-attestation" (they represent a different relationship between entities).

@dataclass(frozen=True)
class Foretis:
    tick_number: int
    my_content_hash: bytes  # SHA-256 of the content
    signature: bytes        # 64-byte Ed25519 signature
    tbid: bytes             # Time being identity
    echo: str               # Echoed back from stamp input
    tbn: str                # Time being name
```

### Cryptography

```python
from foretias.crypto import generate_keypair, sha256, sign, verify

private_key, public_key = generate_keypair()  # 32 bytes each
signature = sign(payload, private_key)          # 64 bytes
is_valid = verify(payload, signature, public_key)  # bool
content_hash = sha256(data)                     # 32 bytes
```

### Config

```python
from foretias.config import Config

cfg = Config.resolve(persist_path="/custom/path")  # arg > $FORETIAS_HOME > ~/.foretias/
```

## Development

### Build

```bash
pip install -e .
```

### Test

```bash
python -m pytest tests/ -v
```

### Coverage

```bash
pip install pytest-cov
python -m pytest tests/ --cov=foretias --cov-report=term-missing
```

## Build & Package

Foretias has two implementations: a pure Python prototype (`src/foretias/`) and a production Rust implementation via PyO3 (`p2p/foretias-python/`), wrapped by the `foretias_p2p` package.

### Python (pure prototype)

```bash
# Install in development mode
pip install -e .

# Run tests
python -m pytest tests/ -v

# Build distribution
pip install build
python -m build

# The package is built via hatchling
# Entry point: foretis → foretias.cli:main
```

### pyforetias (Rust-backed, production)

```bash
# Install maturin (needed for PyO3 builds)
pip install maturin

# Build the Rust library with Python bindings
cd p2p/foretias-python && maturin build --release

# This produces a .whl file in target/wheels/
# Install: pip install target/wheels/pyforetias-*.whl

# The pyforetias package wraps the Rust foretias_p2p library
# Usage: import pyforetias; pyforetias.stamp(...), pyforetias.verify(...)
```

### Native Dependencies

- **libsodium** (`libsodium-dev` on Debian/Ubuntu) — required for the Rust library
- **Rust toolchain** — required to build pyforetias
- **libclang** (`clang-dev`) — needed during build for bindgen

## Testing

```bash
# Python prototype tests
python -m pytest tests/ -v

# Rust unit tests
cd p2p && cargo test --workspace

# Cross-language integration tests (Rust vs pyforetias)
python -m pytest p2p/foretias-python/tests/python/test_cross_language.py -v

# All tests
python -m pytest tests/ -v && cd p2p && cargo test --workspace
```

## Publishing

```bash
# To publish on PyPI:
pip install twine
python -m build
twine upload dist/*
```

## Tests

The test suite is organized into two tiers — **functional** (fast, deterministic) and **aggressive** (slow, destructive, edge-case coverage).

| File | Layer | Coverage |
|------|-------|----------|
| `test_functional.py` | Core | Pure functions in `_timebeing` — stamp, tick, verify-pair, verify-chain, verify |
| `test_calendar.py` | Component | Calendar append, get, latest, save/load, integrity-check |
| `test_timebeing.py` | Component | Timebeing base class, Calendar MVP, ChronomatterV1/V1Serial, Inquirer |
| `test_timebeing_aggressively.py` | Defensive | Calendar/Chronomatter/Inquirer edge cases: tampered loads, concurrent shutdown, chronon boundaries |
| `test_timefamily.py` | Integration | TimeFamily orchestration: stamp, verify, tick, persistence, interface accessors |
| `test_timefamily_aggressively.py` | Defensive | TimeFamily persistence roundtrips, stress (100+ ticks), shutdown safety, concurrent stamps |
| `test_crypto.py` | Unit | SHA-256, keypair generation, sign/verify |
| `test_models.py` | Unit | Frozen dataclass invariants — TickRecord, Foretis |
| `test_config.py` | Unit | Config resolution (arg > env > default), frozen dataclass |
| `test_cli.py` | Functional | CLI `foretis verify` — valid, tampered, no-command |

The `_timebeing` module (prefixed with `_`) is the project's internal backbone. Its static methods are deliberately accessible to all other modules — this is an intentional exception to the single-underscore convention.

The project uses the `alpha` branch as the center of development.
# Appendix
The classification for time beings belong to this branch of the **Artificalia** domain.
- Family: **Chronosidae**
- Subfamily: **Chronosinae**
- Tribe: **Chronosini**
- Subtribe: **Chronosina**
- Genus: **Chronos**
- Spieces: **Chronos fidelius**
- Subspecies:
  - Chronomatter: **Chronos fidelius authenticus**
  - Calendar: **Chronos fidelius grapha**
  - Time Family: **Chronos fidelius adunatrix**
  - Inquirer: TBD

## License

This project is licensed under the [BSD 3-Clause Clear License](LICENSE).
