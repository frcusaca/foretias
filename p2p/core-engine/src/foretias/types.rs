//! Type aliases for core Foretias data types.
//!
//! These are zero-cost `type` aliases — no runtime difference from the underlying types.
//! They make signatures self-documenting and prevent mixing up byte arrays.

/// Time Being ID — 16-byte unique identifier for a time being.
pub type Tbid = [u8; 16];

/// Ed25519 public key — 32 bytes.
pub type PublicKey = [u8; 32];

/// Ed25519 signature — 64 bytes.
pub type Signature = [u8; 64];

/// SHA-256 hash digest — 32 bytes.
pub type Digest = [u8; 32];

/// Encrypted message payload (before/after encryption).
pub type Message = Vec<u8>;

/// Auto-attestation nonce — 16 bytes of entropy.
pub type AaNonce = [u8; 16];

/// Tick number.
pub type TickNumber = u64;
