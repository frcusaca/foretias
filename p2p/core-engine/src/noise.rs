//! Safe Rust wrapper for the C11 Noise_XX handshake and encrypted transport.
//!
//! Wraps `ForetiasNoiseState` with RAII lifetime management and safe APIs.
//! Protocol: `Noise_XX_25519_ChaChaPoly_SHA256`
//!
//! # Handshake Flow
//!
//! **Initiator (client):**
//! 1. `step()` → sends 32 bytes (ephemeral public key)
//! 2. `step()` ← receives 80 bytes (responder ephemeral + encrypted static)
//! 3. `step()` → sends ≤48 bytes (encrypted static)
//!
//! **Responder (server):**
//! 1. `step()` ← receives 32 bytes (initiator ephemeral)
//! 2. `step()` → sends 80 bytes (ephemeral + encrypted static)
//! 3. `step()` ← receives ≤48 bytes (encrypted static)

use std::ptr::NonNull;
use std::mem::ManuallyDrop;

use crate::core::bindings::*;
use crate::core::identity::PrivKeyHandle;
use crate::error::{CryptoError, c_result_to_error};

/// Maximum payload size supported by the Noise_XX cipher.
pub const NOISE_MAX_MSG: usize = FORETIAS_NOISE_MAX_MSG as usize;

/// Maximum handshake message size (ephemeral + encrypted static + padding).
const HANDSHAKE_MAX: usize = 128;

/// Opaque handle to a C11 `ForetiasNoiseState` with RAII cleanup.
pub struct NoiseSession(ManuallyDrop<NonNull<ForetiasNoiseState>>);

// NoiseSession is intentionally NOT Send. The wrapped C11 ForetiasNoiseState
// contains mutable nonce counters (`send_nonce`, `recv_nonce`) advanced by
// foretias_noise_send/recv. Cross-thread access without synchronization
// can corrupt the nonce sequence and trigger ChaCha20-Poly1305 nonce reuse.
// If you need to send a session across threads, wrap it in
// Arc<tokio::sync::Mutex<NoiseSession>> at the call site.

impl NoiseSession {
    /// Create a new initiator (client) session.
    ///
    /// # Arguments
    /// * `static_priv` - 32-byte Ed25519 static private key (used to derive X25519 key)
    /// * `their_static_pub` - Optional remote static public key (None for Noise_XX)
    pub fn new_initiator(
        static_priv: &[u8; 32],
        their_static_pub: Option<&[u8; 32]>,
    ) -> Result<Self, CryptoError> {
        Self::new(static_priv, their_static_pub, true)
    }

    /// Create a new responder (server) session.
    ///
    /// # Arguments
    /// * `static_priv` - 32-byte Ed25519 static private key
    /// * `their_static_pub` - Optional remote static public key (None for Noise_XX)
    pub fn new_responder(
        static_priv: &[u8; 32],
        their_static_pub: Option<&[u8; 32]>,
    ) -> Result<Self, CryptoError> {
        Self::new(static_priv, their_static_pub, false)
    }

    /// Create a new initiator (client) session from an opaque key handle.
    ///
    /// # Arguments
    /// * `handle` - PrivKeyHandle (private key bytes never leave C memory)
    /// * `their_static_pub` - Optional remote static public key (None for Noise_XX)
    pub fn new_initiator_with_handle(
        handle: &PrivKeyHandle,
        their_static_pub: Option<&[u8; 32]>,
    ) -> Result<Self, CryptoError> {
        Self::new_with_handle(handle, their_static_pub, true)
    }

    /// Create a new responder (server) session from an opaque key handle.
    ///
    /// # Arguments
    /// * `handle` - PrivKeyHandle (private key bytes never leave C memory)
    /// * `their_static_pub` - Optional remote static public key (None for Noise_XX)
    pub fn new_responder_with_handle(
        handle: &PrivKeyHandle,
        their_static_pub: Option<&[u8; 32]>,
    ) -> Result<Self, CryptoError> {
        Self::new_with_handle(handle, their_static_pub, false)
    }

    fn new_with_handle(
        handle: &PrivKeyHandle,
        their_static_pub: Option<&[u8; 32]>,
        is_initiator: bool,
    ) -> Result<Self, CryptoError> {
        let their_pub: *const ForetiasPubKey32 =
            their_static_pub.map_or(std::ptr::null(), |p| p as *const _ as *const ForetiasPubKey32);

        let layout = std::alloc::Layout::new::<ForetiasNoiseState>();
        let state_ptr = unsafe {
            let ptr = std::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(CryptoError::BadInput("noise state allocation failed"));
            }
            ptr as *mut ForetiasNoiseState
        };

        let rc = unsafe {
            foretias_noise_init_with_handle(
                state_ptr,
                handle.as_ptr(),
                their_pub,
                is_initiator,
            )
        };

        c_result_to_error(rc)?;

        let ptr = NonNull::new(state_ptr)
            .ok_or(CryptoError::BadInput("noise: C library returned null state pointer"))?;
        Ok(Self(ManuallyDrop::new(ptr)))
    }

    fn new(
        static_priv: &[u8; 32],
        their_static_pub: Option<&[u8; 32]>,
        is_initiator: bool,
    ) -> Result<Self, CryptoError> {
        let mut priv_key = ForetiasPrivKey32 { bytes: [0u8; 32] };
        priv_key.bytes.copy_from_slice(static_priv);

        let their_pub: *const ForetiasPubKey32 =
            their_static_pub.map_or(std::ptr::null(), |p| p as *const _ as *const ForetiasPubKey32);

        let layout = std::alloc::Layout::new::<ForetiasNoiseState>();
        // SAFETY: alloc_zeroed returns a valid, aligned pointer for the layout; null check below guards allocation failure.
        let state_ptr = unsafe {
            let ptr = std::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(CryptoError::BadInput("noise state allocation failed"));
            }
            ptr as *mut ForetiasNoiseState
        };

        // SAFETY: state_ptr is valid and aligned (from alloc_zeroed above); priv_key and their_pub remain valid for duration of this call.
        let rc = unsafe {
            foretias_noise_init_ed25519(
                state_ptr,
                &priv_key,
                their_pub,
                is_initiator,
            )
        };

        // Zeroize private key immediately
        priv_key.bytes.fill(0);

        c_result_to_error(rc)?;

        // SAFETY: C11 API guarantees non-null pointer on success (rc == 0).
        let ptr = NonNull::new(state_ptr)
            .ok_or(CryptoError::BadInput("noise: C library returned null state pointer"))?;
        Ok(Self(ManuallyDrop::new(ptr)))
    }

    /// Execute the next handshake step.
    ///
    /// # Returns
    /// The handshake message bytes to send (or empty if consuming input only).
    ///
    /// # Arguments
    /// * `input` - Incoming handshake message from the peer (None for first initiator step)
    pub fn step(&mut self, input: Option<&[u8]>) -> Result<Vec<u8>, CryptoError> {
        let mut output = vec![0u8; HANDSHAKE_MAX];
        let mut output_len = output.len() as usize;

        // SAFETY: self.0 is valid (invariant of NoiseSession); output buf is HANDSHAKE_MAX bytes; input ptr is valid for its length or null.
        let rc = unsafe {
            foretias_noise_step(
                self.0.as_ptr(),
                input.map_or(std::ptr::null(), |s| s.as_ptr()),
                input.map_or(0, |s| s.len()),
                output.as_mut_ptr(),
                &mut output_len,
            )
        };
        c_result_to_error(rc)?;

        output.truncate(output_len);
        Ok(output)
    }

    /// Encrypt a plaintext payload after handshake completion.
    ///
    /// Returns ciphertext of length `plaintext.len() + 16` (Poly1305 tag).
    pub fn send(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let ct_len = plaintext.len() + 16;
        let mut ciphertext = vec![0u8; ct_len];
        let mut actual_len = ct_len;

        // SAFETY: self.0 is valid; plaintext and ciphertext buffers are valid for their respective lengths.
        let rc = unsafe {
            foretias_noise_send(
                self.0.as_ptr(),
                plaintext.as_ptr(),
                plaintext.len(),
                ciphertext.as_mut_ptr(),
                &mut actual_len,
            )
        };
        c_result_to_error(rc)?;

        ciphertext.truncate(actual_len);
        Ok(ciphertext)
    }

    /// Decrypt a ciphertext payload after handshake completion.
    ///
    /// # Arguments
    /// * `ciphertext` - Encrypted message (must be ≥ 16 bytes for the tag)
    pub fn recv(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut plaintext = vec![0u8; ciphertext.len()];
        let mut actual_len = plaintext.len();

        // SAFETY: self.0 is valid; ciphertext and plaintext buffers are valid for their respective lengths.
        let rc = unsafe {
            foretias_noise_recv(
                self.0.as_ptr(),
                ciphertext.as_ptr(),
                ciphertext.len(),
                plaintext.as_mut_ptr(),
                &mut actual_len,
            )
        };
        c_result_to_error(rc)?;

        plaintext.truncate(actual_len);
        Ok(plaintext)
    }

    /// Check if the handshake has completed and encrypted transport is ready.
    pub fn is_complete(&self) -> bool {
        // SAFETY: self.0 is valid for the lifetime of this NoiseSession.
        unsafe { (*self.0.as_ptr()).handshake_complete != 0 }
    }

    /// Get the local static public key (X25519 point derived from Ed25519 key).
    pub fn local_static_pub(&self) -> [u8; 32] {
        // SAFETY: self.0 is valid for the lifetime of this NoiseSession.
        unsafe { (*self.0.as_ptr()).local_static_pub }
    }

    /// Get the remote static public key (populated after handshake).
    pub fn remote_static_pub(&self) -> Option<[u8; 32]> {
        if self.is_complete() {
            // SAFETY: self.0 is valid; remote_static is populated by C11 after handshake completion.
            let pub_key = unsafe { (*self.0.as_ptr()).remote_static };
            if pub_key.iter().any(|&b| b != 0) {
                Some(pub_key)
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Drop for NoiseSession {
    fn drop(&mut self) {
        // SAFETY: self.0 is valid (RAII guarantee); layout matches the allocation used in NoiseSession::new.
        unsafe {
            foretias_noise_destroy(self.0.as_ptr());
            let layout = std::alloc::Layout::new::<ForetiasNoiseState>();
            std::alloc::dealloc(self.0.as_ptr() as *mut u8, layout);
        }
    }
}

// ── Tokio I/O helpers for noisy TCP ──────────────────────────────────────────

async fn read_length_prefix(reader: &mut (impl tokio::io::AsyncReadExt + Unpin)) -> Result<u32, std::io::Error> {
    let mut buf = [0u8; 4];
    tokio::io::AsyncReadExt::read_exact(reader, &mut buf).await?;
    Ok(u32::from_le_bytes(buf))
}

async fn write_length_prefix(writer: &mut (impl tokio::io::AsyncWriteExt + Unpin), len: u32) -> Result<(), std::io::Error> {
    tokio::io::AsyncWriteExt::write_all(writer, &len.to_le_bytes()).await
}

pub async fn noise_handshake(
    mut stream: tokio::net::TcpStream,
    static_priv: &[u8; 32],
    their_static_pub: Option<&[u8; 32]>,
    is_initiator: bool,
) -> Result<(NoiseSession, tokio::net::TcpStream), CryptoError> {
    let mut session = if is_initiator {
        NoiseSession::new_initiator(static_priv, their_static_pub)?
    } else {
        NoiseSession::new_responder(static_priv, their_static_pub)?
    };

    if is_initiator {
        let e_out = session.step(None)?;
        write_len(&mut stream, &e_out).await?;

        let e2 = read_len(&mut stream).await?;
        session.step(Some(&e2))?;

        let e3 = session.step(None)?;
        write_len(&mut stream, &e3).await?;
    } else {
        let e1 = read_len(&mut stream).await?;
        session.step(Some(&e1))?;

        let e2 = session.step(None)?;
        write_len(&mut stream, &e2).await?;

        let e3 = read_len(&mut stream).await?;
        session.step(Some(&e3))?;
    }

    if !session.is_complete() {
        return Err(CryptoError::BadInput("noise handshake incomplete"));
    }
    Ok((session, stream))
}

pub async fn noise_handshake_with_handle(
    mut stream: tokio::net::TcpStream,
    handle: &PrivKeyHandle,
    their_static_pub: Option<&[u8; 32]>,
    is_initiator: bool,
) -> Result<(NoiseSession, tokio::net::TcpStream), CryptoError> {
    let mut session = if is_initiator {
        NoiseSession::new_initiator_with_handle(handle, their_static_pub)?
    } else {
        NoiseSession::new_responder_with_handle(handle, their_static_pub)?
    };

    if is_initiator {
        let e_out = session.step(None)?;
        write_len(&mut stream, &e_out).await?;

        let e2 = read_len(&mut stream).await?;
        session.step(Some(&e2))?;

        let e3 = session.step(None)?;
        write_len(&mut stream, &e3).await?;
    } else {
        let e1 = read_len(&mut stream).await?;
        session.step(Some(&e1))?;

        let e2 = session.step(None)?;
        write_len(&mut stream, &e2).await?;

        let e3 = read_len(&mut stream).await?;
        session.step(Some(&e3))?;
    }

    if !session.is_complete() {
        return Err(CryptoError::BadInput("noise handshake incomplete"));
    }
    Ok((session, stream))
}

async fn write_len(stream: &mut tokio::net::TcpStream, msg: &[u8]) -> Result<(), CryptoError> {
    let len = (msg.len() as u32).to_le_bytes();
    tokio::io::AsyncWriteExt::write_all(stream, &len).await.map_err(|e| CryptoError::IoWrite(e.to_string()))?;
    tokio::io::AsyncWriteExt::write_all(stream, msg).await.map_err(|e| CryptoError::IoWrite(e.to_string()))?;
    tokio::io::AsyncWriteExt::flush(stream).await.map_err(|e| CryptoError::IoWrite(e.to_string()))?;
    Ok(())
}

async fn read_len(stream: &mut tokio::net::TcpStream) -> Result<Vec<u8>, CryptoError> {
    let mut len_buf = [0u8; 4];
    tokio::io::AsyncReadExt::read_exact(stream, &mut len_buf).await.map_err(|e| CryptoError::IoRead(e.to_string()))?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > NOISE_MAX_MSG {
        return Err(CryptoError::BadInput("message exceeds maximum size"));
    }
    let mut buf = vec![0u8; len];
    tokio::io::AsyncReadExt::read_exact(stream, &mut buf).await.map_err(|e| CryptoError::IoRead(e.to_string()))?;
    Ok(buf)
}

/// Send an encrypted message over an established Noise session.
///
/// Frame format: 4-byte LE length prefix + ciphertext
pub async fn noise_send(
    session: &mut NoiseSession,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    plaintext: &[u8],
) -> Result<(), CryptoError> {
    let ct = session.send(plaintext)?;
    write_length_prefix(writer, ct.len() as u32).await
        .map_err(|e| CryptoError::IoWrite(format!("length prefix: {}", e)))?;
    tokio::io::AsyncWriteExt::write_all(writer, &ct).await
        .map_err(|e| CryptoError::IoWrite(format!("ciphertext write: {}", e)))?;
    tokio::io::AsyncWriteExt::flush(writer).await
        .map_err(|e| CryptoError::IoWrite(format!("flush: {}", e)))?;
    Ok(())
}

/// Receive a decrypted message over an established Noise session.
///
/// Frame format: 4-byte LE length prefix + ciphertext
pub async fn noise_recv(
    session: &mut NoiseSession,
    reader: &mut tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<Vec<u8>, CryptoError> {
    let ct_len = read_length_prefix(reader).await
        .map_err(|e| CryptoError::IoRead(format!("length prefix: {}", e)))? as usize;
    let mut ct_buf = vec![0u8; ct_len];
    tokio::io::AsyncReadExt::read_exact(reader, &mut ct_buf).await
        .map_err(|e| CryptoError::IoRead(format!("ciphertext read: {}", e)))?;
    session.recv(&ct_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_session_init_and_complete() {
        PrivKeyHandle::init();
        let (_alice_pub, alice_priv) = crate::core::identity::generate_ed25519_keypair().unwrap();
        let (_bob_pub, bob_priv) = crate::core::identity::generate_ed25519_keypair().unwrap();

        // Convert Ed25519 keys to raw bytes for Noise
        let alice_priv_bytes: [u8; 32] = alice_priv.bytes;
        let bob_priv_bytes: [u8; 32] = bob_priv.bytes;

        let mut alice = NoiseSession::new_initiator(&alice_priv_bytes, None).unwrap();
        let mut bob = NoiseSession::new_responder(&bob_priv_bytes, None).unwrap();

        // Step 0: initiator sends e→
        let e1 = alice.step(None).unwrap();
        assert_eq!(e1.len(), 32, "M1 should be 32 bytes (ephemeral pubkey)");

        // Step 0: responder receives e→
        bob.step(Some(&e1)).unwrap();

        // Step 1: responder sends e←,es→,ec→
        let e2 = bob.step(None).unwrap();
        assert!(e2.len() >= 64, "M2 should be >= 64 bytes");

        // Step 1: initiator receives e←,es→,ec→
        alice.step(Some(&e2)).unwrap();

        // Step 2: initiator sends es←,ec←
        let e3 = alice.step(None).unwrap();
        assert!(e3.len() > 0, "M3 should be non-empty");

        // Step 2: responder receives es←,ec←
        bob.step(Some(&e3)).unwrap();

        assert!(alice.is_complete(), "Alice handshake should complete");
        assert!(bob.is_complete(), "Bob handshake should complete");
    }

    #[test]
    fn noise_session_encrypt_decrypt_roundtrip() {
        PrivKeyHandle::init();
        let (_alice_pub, alice_priv) = crate::core::identity::generate_ed25519_keypair().unwrap();
        let (_bob_pub, bob_priv) = crate::core::identity::generate_ed25519_keypair().unwrap();

        let alice_priv_bytes: [u8; 32] = alice_priv.bytes;
        let bob_priv_bytes: [u8; 32] = bob_priv.bytes;

        let mut alice = NoiseSession::new_initiator(&alice_priv_bytes, None).unwrap();
        let mut bob = NoiseSession::new_responder(&bob_priv_bytes, None).unwrap();

        // Complete handshake
        let e1 = alice.step(None).unwrap();
        bob.step(Some(&e1)).unwrap();
        let e2 = bob.step(None).unwrap();
        alice.step(Some(&e2)).unwrap();
        let e3 = alice.step(None).unwrap();
        bob.step(Some(&e3)).unwrap();

        // Alice sends a message to Bob
        let msg = b"Hello through encrypted Noise!";
        let ct = alice.send(msg).unwrap();
        assert_ne!(ct, msg, "ciphertext should differ from plaintext");

        let pt = bob.recv(&ct).unwrap();
        assert_eq!(pt, msg, "Bob should decrypt Alice's message");

        // Bob replies to Alice
        let reply = b"Message received securely!";
        let ct2 = bob.send(reply).unwrap();
        let pt2 = alice.recv(&ct2).unwrap();
        assert_eq!(pt2, reply, "Alice should decrypt Bob's reply");
    }

    #[test]
    fn noise_session_rejects_tampered_ciphertext() {
        PrivKeyHandle::init();
        let (alice_priv, _) = crate::core::identity::generate_ed25519_keypair().unwrap();
        let (bob_priv, _) = crate::core::identity::generate_ed25519_keypair().unwrap();

        let alice_priv_bytes: [u8; 32] = alice_priv.bytes;
        let bob_priv_bytes: [u8; 32] = bob_priv.bytes;

        let mut alice = NoiseSession::new_initiator(&alice_priv_bytes, None).unwrap();
        let mut bob = NoiseSession::new_responder(&bob_priv_bytes, None).unwrap();

        let e1 = alice.step(None).unwrap();
        bob.step(Some(&e1)).unwrap();
        let e2 = bob.step(None).unwrap();
        alice.step(Some(&e2)).unwrap();
        let e3 = alice.step(None).unwrap();
        bob.step(Some(&e3)).unwrap();

        let msg = b"test";
        let mut ct = alice.send(msg).unwrap();
        // Tamper with ciphertext
        ct[0] ^= 0xFF;

        assert!(bob.recv(&ct).is_err(), "Tampered ciphertext should be rejected");
    }

    #[test]
    fn noise_session_large_message() {
        PrivKeyHandle::init();
        let (alice_priv, _) = crate::core::identity::generate_ed25519_keypair().unwrap();
        let (bob_priv, _) = crate::core::identity::generate_ed25519_keypair().unwrap();

        let alice_priv_bytes: [u8; 32] = alice_priv.bytes;
        let bob_priv_bytes: [u8; 32] = bob_priv.bytes;

        let mut alice = NoiseSession::new_initiator(&alice_priv_bytes, None).unwrap();
        let mut bob = NoiseSession::new_responder(&bob_priv_bytes, None).unwrap();

        let e1 = alice.step(None).unwrap();
        bob.step(Some(&e1)).unwrap();
        let e2 = bob.step(None).unwrap();
        alice.step(Some(&e2)).unwrap();
        let e3 = alice.step(None).unwrap();
        bob.step(Some(&e3)).unwrap();

        // Send a 4KB message
        let big_msg = vec![0xABu8; 4096];
        let ct = alice.send(&big_msg).unwrap();
        let pt = bob.recv(&ct).unwrap();
        assert_eq!(pt, big_msg, "Large message roundtrip should succeed");
    }

    #[test]
    fn noise_session_with_handle_init_and_complete() {
        PrivKeyHandle::init();

        let alice_handle = PrivKeyHandle::generate().unwrap();
        let bob_handle = PrivKeyHandle::generate().unwrap();

        let mut alice = NoiseSession::new_initiator_with_handle(&alice_handle, None).unwrap();
        let mut bob = NoiseSession::new_responder_with_handle(&bob_handle, None).unwrap();

        let e1 = alice.step(None).unwrap();
        bob.step(Some(&e1)).unwrap();
        let e2 = bob.step(None).unwrap();
        alice.step(Some(&e2)).unwrap();
        let e3 = alice.step(None).unwrap();
        bob.step(Some(&e3)).unwrap();

        assert!(alice.is_complete(), "Alice handshake should complete");
        assert!(bob.is_complete(), "Bob handshake should complete");

        let msg = b"handle-based noise test";
        let ct = alice.send(msg).unwrap();
        let pt = bob.recv(&ct).unwrap();
        assert_eq!(pt, msg, "Bob should decrypt Alice's message via handle");
    }
}
