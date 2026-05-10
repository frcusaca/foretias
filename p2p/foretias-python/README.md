# foretias-p2p Python Bindings

Python bindings for the Foretias P2P attestation engine.

## Installation

```bash
pip install maturin
cd foretias-python
maturin develop
```

## Usage

```python
import foretias_p2p

crypto = foretias_p2p.PyCryptoServer()
# ... use crypto for signing/verification
```
