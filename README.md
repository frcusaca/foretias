# Fortias

**Free, Open-source and Resilient Time Integrity Attestation Service**

Fortias makes digital timestamping backdate-proof by cryptographic construction. Each tick of a time being's calendar has its own Ed25519 keypair, and when a tick advances, the previous private key is destroyed. A Fortis stamped at tick *n* cannot be forged from the past.

## Installation

```bash
pip install fortias
```

## Quick Start

```python
from fortias import TimeFamily

# Create a time being with 1-minute ticks
tbf = TimeFamily(name="alpha", chronon_ns=60_000_000_000.0)

# Stamp your first message
fortis = tbf.stamp("hello world")

# Verify it
is_valid = tbf.verify("hello world", fortis)
print(is_valid)  # True

# Verify with window check (requires next tick to exist)
is_valid, window_closed = tbf.verify("hello world", fortis, next_tick_number=1)
print(is_valid, window_closed)  # True, True
```

## CLI

```bash
fortis verify --calendar calendar.json --file message.txt --fortis fortis.json
```

Add `--chain` to run a full chain integrity check on the calendar.

## API Reference

### Core class: `TimeFamily`

`TimeFamily` (*Chrona nuntia*, the messenger) orchestrates between `Chronomatter` (*Chronos authenticus*, the Time Authority) and `Calendar` (*Chrona grapha*, passive storage).

```python
from fortias import TimeFamily, Chronomatter, Fortis, TickRecord, Config

tbf = TimeFamily(name="alpha", chronon_ns=60_000_000_000.0)
```

| Method | Returns | Description |
|--------|---------|-------------|
| `stamp(content)` | `Fortis` | Sign content under the current tick's key |
| `verify(content, fortis)` | `bool` or `(bool, bool)` | Verify a Fortis artifact |
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
    forward_fortis: bytes   # MA(prev) signed by prev_sk
    backward_fortis: bytes  # MA(self) signed by self_sk

@dataclass(frozen=True)
class Fortis:
    tick_number: int
    my_content_hash: bytes  # SHA-256 of the content
    signature: bytes        # 64-byte Ed25519 signature
    tbid: bytes             # Time being identity
    echo: str               # Echoed back from stamp input
    tbn: str                # Time being name
```

### Cryptography

```python
from fortias.crypto import generate_keypair, sha256, sign, verify

private_key, public_key = generate_keypair()  # 32 bytes each
signature = sign(payload, private_key)          # 64 bytes
is_valid = verify(payload, signature, public_key)  # bool
content_hash = sha256(data)                     # 32 bytes
```

### Config

```python
from fortias.config import Config

cfg = Config.resolve(persist_path="/custom/path")  # arg > $FORTIAS_HOME > ~/.fortias/
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
python -m pytest tests/ --cov=fortias --cov-report=term-missing
```

The project uses the `alpha` branch as the center of development.

## License

This project is licensed under the [BSD 3-Clause Clear License](LICENSE).
