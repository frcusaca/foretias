# fortias-p2p Python Bindings

Python bindings for the Fortias P2P attestation engine.

## Installation

```bash
pip install maturin
cd fortias-python
maturin develop
```

## Usage

```python
import fortias_p2p

crypto = fortias_p2p.PyCryptoServer()
# ... use crypto for signing/verification
```
