//! Ed25519 snapshot signing infrastructure.
//!
//! Derives deterministic Ed25519 keypairs from passphrases via Argon2id,
//! signs content, and verifies signatures.
//!
//! ## Design
//!
//! - **derive_keypair**: `passphrase` → Argon2id (fixed salt) → 32-byte seed → Ed25519 keypair.
//!   Deterministic: same passphrase always yields the same keypair.
//!   Empty passphrase (`""`) is valid and produces the default "computer/AI agent" keypair.
//!
//! - **sign_content**: Signs UTF-8 content bytes with the given `SigningKey`.
//!   Returns `(verifying_key, signature_bytes)`.
//!
//! - **verify_signature**: Returns `true` if `signature` is a valid Ed25519 signature
//!   of `content` bytes under `verifying_key`.
//!
//! ## High-level snapshot API
//!
//! - **canonicalize_block**: strip whitespace, append `\n`.
//! - **sign_snapshot**: progressive triple-signing; each sig covers all preceding blocks.
//! - **verify_snapshot**: verify all three progressive signatures; return [`SnapshotVerification`].
//! - **parse_snapshot_footer**: extract [`SnapshotSignature`] from the SIGNATURES section.
//!
//! ## Progressive signing
//!
//! Each signature covers the cumulative canonical content up to and including its own block:
//! - `input_sig`     = sign(`canon_input`)
//! - `result_sig`    = sign(`canon_input + canon_result`)
//! - `comments_sig`  = sign(`canon_input + canon_result + canon_comments`)
//!
//! Canonical blocks already end with `\n`, so concatenation is unambiguous.
//! This means tampering with any earlier block invalidates all later signatures.

use ed25519_dalek::{Signer, SigningKey, VerifyingKey, Signature};
use argon2::Argon2;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

/// Fixed salt for deterministic key derivation.
const SALT: &[u8] = b"foretias:snapsuite-sig:v1";

/// Derive an Ed25519 keypair from a passphrase.
///
/// Uses Argon2id with a fixed salt to produce a 32-byte seed, which is then
/// used to construct a deterministic Ed25519 keypair.
///
/// # Determinism
///
/// The same passphrase always produces the same keypair. This is intentional:
/// it allows human and AI agents to reproduce signing keys from a known passphrase.
///
/// # Arguments
///
/// * `passphrase` - The passphrase string. May be empty; empty produces
///   the default keypair used by computers and AI agents.
///
/// # Returns
///
/// A `(SigningKey, VerifyingKey)` tuple ready for signing and verification.
pub fn derive_keypair(passphrase: &str) -> (SigningKey, VerifyingKey) {
    let argon2 = Argon2::default();

    // Produce a 32-byte hash suitable as an Ed25519 seed.
    let mut hash_output = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), SALT, &mut hash_output)
        .expect("Argon2id hash should not fail with valid inputs");

    let signing_key = SigningKey::from(&hash_output.into());
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

/// Sign content bytes with the given signing key.
///
/// # Arguments
///
/// * `signing_key` - The Ed25519 signing key.
/// * `content` - The UTF-8 string to sign (signed as its byte representation).
///
/// # Returns
///
/// A tuple of `(VerifyingKey, Vec<u8>)` where the second element is the
/// 64-byte Ed25519 signature.
pub fn sign_content(
    signing_key: &SigningKey,
    content: &str,
) -> (VerifyingKey, Vec<u8>) {
    let signature: Signature = signing_key.sign(content.as_bytes());
    (signing_key.verifying_key(), signature.to_bytes().to_vec())
}

/// Verify an Ed25519 signature.
///
/// # Arguments
///
/// * `verifying_key` - The public key to verify against.
/// * `content` - The original UTF-8 content (verified as its byte representation).
/// * `signature` - The raw signature bytes (expected 64 bytes for Ed25519).
///
/// # Returns
///
/// `true` if the signature is valid; `false` otherwise.
/// Returns `false` for malformed signatures rather than panicking.
pub fn verify_signature(
    verifying_key: &VerifyingKey,
    content: &str,
    signature: &[u8],
) -> bool {
    let sig = match Signature::from_slice(signature) {
        Ok(s) => s,
        Err(_) => return false,
    };
    verifying_key.verify_strict(content.as_bytes(), &sig).is_ok()
}

// ============================================================================
// Canonicalization
// ============================================================================

/// Canonicalize a block for signing.
///
/// Strips leading and trailing whitespace (including final newline), then
/// appends exactly one `\n`. This ensures the signed content is deterministic
/// regardless of trailing whitespace in the source.
pub fn canonicalize_block(text: &str) -> String {
    let mut s = text.trim().to_string();
    s.push('\n');
    s
}

/// Canonicalize all result blocks and join them for signing.
///
/// Each block is passed through `canonicalize_block`, then joined
/// with `\n` as a separator between blocks. An empty slice returns `"\n"`.
pub fn canonicalize_all_results(results: &[String]) -> String {
    if results.is_empty() {
        return "\n".to_string();
    }
    results.iter()
        .map(|r| canonicalize_block(r))
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// High-level snapshot signing
// ============================================================================

/// The SIGNATURES section appended to a signed snapshot file.
///
/// All three signatures are progressive: each covers the cumulative canonical
/// content up to and including its own block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotSignature {
    /// Hex-encoded 32-byte Ed25519 verifying key.
    pub public_key_hex: String,
    /// Base64-encoded 64-byte Ed25519 signature over `canon_input`.
    pub input_sig_b64: String,
    /// Base64-encoded 64-byte Ed25519 signature over `canon_input + canon_result`.
    pub result_sig_b64: String,
    /// Base64-encoded 64-byte Ed25519 signature over `canon_input + canon_result + canon_comments`.
    pub comments_sig_b64: String,
}

impl SnapshotSignature {
    /// Render the SIGNATURES section: label line followed by the four key/sig lines.
    pub fn format_footer(&self) -> String {
        format!(
            "SIGNATURES:\nPublic key: {}\nInput signature: {}\nResult signature: {}\nComments signature: {}",
            self.public_key_hex,
            self.input_sig_b64,
            self.result_sig_b64,
            self.comments_sig_b64,
        )
    }
}

/// Result of verifying a snapshot's three progressive signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotVerification {
    /// The passphrase-derived public key matches the key embedded in the footer.
    pub key_match: bool,
    /// The input signature verifies over `canon_input`.
    pub input_ok: bool,
    /// The result signature verifies over `canon_input + canon_result`.
    pub result_ok: bool,
    /// The comments signature verifies over `canon_input + canon_result + canon_comments`.
    pub comments_ok: bool,
}

impl SnapshotVerification {
    /// Returns `true` if all four checks pass.
    pub fn all_ok(&self) -> bool {
        self.key_match && self.input_ok && self.result_ok && self.comments_ok
    }

    /// Short status string for CLI output.
    pub fn status_line(&self) -> String {
        let key      = if self.key_match    { "match" } else { "no_match" };
        let input    = if self.input_ok     { "ok" } else { "fail" };
        let result   = if self.result_ok    { "ok" } else { "fail" };
        let comments = if self.comments_ok  { "ok" } else { "fail" };
        format!("key={key} input={input} result={result} comments={comments}")
    }
}

/// Sign a snapshot using progressive Ed25519 signatures.
///
/// Each signature covers the cumulative canonical content up to its block:
/// - `input_sig`    = sign(`canon_input`)
/// - `result_sig`   = sign(`canon_input + canon_result`)
/// - `comments_sig` = sign(`canon_input + canon_result + canon_comments`)
///
/// Pass `passphrase = ""` for the default computer/AI-agent key.
pub fn sign_snapshot(
    passphrase: &str,
    input: &str,
    results: &[String],
    comments: &str,
) -> SnapshotSignature {
    let canon_input    = canonicalize_block(input);
    let canon_result   = canonicalize_all_results(results);
    let canon_comments = canonicalize_block(comments);
    let (sk, vk) = derive_keypair(passphrase);
    let result_content   = format!("{canon_input}{canon_result}");
    let comments_content = format!("{canon_input}{canon_result}{canon_comments}");
    let (_, input_sig_bytes)    = sign_content(&sk, &canon_input);
    let (_, result_sig_bytes)   = sign_content(&sk, &result_content);
    let (_, comments_sig_bytes) = sign_content(&sk, &comments_content);
    SnapshotSignature {
        public_key_hex:   hex::encode(vk.to_bytes()),
        input_sig_b64:    B64.encode(&input_sig_bytes),
        result_sig_b64:   B64.encode(&result_sig_bytes),
        comments_sig_b64: B64.encode(&comments_sig_bytes),
    }
}

/// Verify a snapshot's three progressive signatures.
///
/// Uses the key embedded in `sig`. `key_match` reports whether `passphrase`
/// derives the same public key as `sig.public_key_hex`.
pub fn verify_snapshot(
    passphrase: &str,
    input: &str,
    results: &[String],
    comments: &str,
    sig: &SnapshotSignature,
) -> SnapshotVerification {
    let fail = |key_match| SnapshotVerification {
        key_match, input_ok: false, result_ok: false, comments_ok: false
    };
    let vk_bytes: [u8; 32] = match hex::decode(&sig.public_key_hex)
        .ok()
        .and_then(|v| v.try_into().ok())
    {
        Some(b) => b,
        None => return fail(false),
    };
    let vk = match VerifyingKey::from_bytes(&vk_bytes) {
        Ok(k) => k,
        Err(_) => return fail(false),
    };
    let (_, derived_vk) = derive_keypair(passphrase);
    let key_match = derived_vk.to_bytes() == vk.to_bytes();

    let canon_input    = canonicalize_block(input);
    let canon_result   = canonicalize_all_results(results);
    let canon_comments = canonicalize_block(comments);
    let result_content   = format!("{canon_input}{canon_result}");
    let comments_content = format!("{canon_input}{canon_result}{canon_comments}");

    let decode = |b64: &str| B64.decode(b64).ok();
    let input_sig_bytes    = match decode(&sig.input_sig_b64)    { Some(b) => b, None => return fail(key_match) };
    let result_sig_bytes   = match decode(&sig.result_sig_b64)   { Some(b) => b, None => return fail(key_match) };
    let comments_sig_bytes = match decode(&sig.comments_sig_b64) { Some(b) => b, None => return fail(key_match) };

    let input_ok    = verify_signature(&vk, &canon_input,     &input_sig_bytes);
    let result_ok   = verify_signature(&vk, &result_content,  &result_sig_bytes);
    let comments_ok = verify_signature(&vk, &comments_content, &comments_sig_bytes);
    SnapshotVerification { key_match, input_ok, result_ok, comments_ok }
}

/// Extract a [`SnapshotSignature`] from the SIGNATURES section of a snapshot file.
///
/// Finds the last `SIGNATURES:` label line and parses the three key/sig lines that follow.
/// Returns `None` if the label or any required line is missing or malformed.
pub fn parse_snapshot_footer(snapshot_text: &str) -> Option<SnapshotSignature> {
    let lines: Vec<&str> = snapshot_text.lines().collect();
    let sig_pos = lines.iter().rposition(|l| l.trim() == "SIGNATURES:")?;
    let after: Vec<&str> = lines[sig_pos + 1..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();
    if after.len() < 4 {
        return None;
    }
    let public_key_hex   = after[0].strip_prefix("Public key: ")?.trim().to_string();
    let input_sig_b64    = after[1].strip_prefix("Input signature: ")?.trim().to_string();
    let result_sig_b64   = after[2].strip_prefix("Result signature: ")?.trim().to_string();
    let comments_sig_b64 = after[3].strip_prefix("Comments signature: ")?.trim().to_string();
    if public_key_hex.is_empty() || input_sig_b64.is_empty()
        || result_sig_b64.is_empty() || comments_sig_b64.is_empty()
    {
        return None;
    }
    Some(SnapshotSignature { public_key_hex, input_sig_b64, result_sig_b64, comments_sig_b64 })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_keypair_empty_is_deterministic() {
        let (sk1, vk1) = derive_keypair("");
        let (sk2, vk2) = derive_keypair("");

        assert_eq!(
            vk1.to_bytes(),
            vk2.to_bytes(),
            "derive_keypair(\"\") must produce the same keypair on repeated calls"
        );
        assert_eq!(
            sk1.to_bytes(),
            sk2.to_bytes(),
            "Signing keys must also match"
        );
    }

    #[test]
    fn derive_keypair_different_passphrases_produce_different_keys() {
        let (_, vk_empty) = derive_keypair("");
        let (_, vk_human) = derive_keypair("human");

        assert_ne!(
            vk_empty.to_bytes(),
            vk_human.to_bytes(),
            "Different passphrases must produce different verifying keys"
        );
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let (sk, vk) = derive_keypair("test-passphrase");
        let content = "snapshot content to sign";

        let (returned_vk, sig_bytes) = sign_content(&sk, content);
        assert_eq!(vk.to_bytes(), returned_vk.to_bytes(), "Returned verifying key must match");
        assert_eq!(sig_bytes.len(), 64, "Ed25519 signature must be 64 bytes");

        assert!(
            verify_signature(&vk, content, &sig_bytes),
            "Valid signature must verify successfully"
        );
    }

    #[test]
    fn verify_fails_with_wrong_key() {
        let (sk_a, _) = derive_keypair("key-a");
        let (_, vk_b) = derive_keypair("key-b");
        let content = "signed with key-a";

        let (_, sig_bytes) = sign_content(&sk_a, content);

        assert!(
            !verify_signature(&vk_b, content, &sig_bytes),
            "Signature made with key-a must fail verification with key-b"
        );
    }

    #[test]
    fn verify_fails_with_tampered_content() {
        let (_, vk) = derive_keypair("test");
        let original = "original content";
        let tampered = "tampered content";

        let (_, sig_bytes) = sign_content(&derive_keypair("test").0, original);

        assert!(
            !verify_signature(&vk, tampered, &sig_bytes),
            "Signature must fail when content is tampered"
        );
    }

    #[test]
    fn verify_fails_with_truncated_signature() {
        let (_, vk) = derive_keypair("test");
        let content = "some content";
        let (_, full_sig) = sign_content(&derive_keypair("test").0, content);

        // Truncate signature to 32 bytes (half of valid 64)
        let truncated: Vec<u8> = full_sig[..32].to_vec();

        assert!(
            !verify_signature(&vk, content, &truncated),
            "Truncated signature must fail verification"
        );
    }

    #[test]
    fn verify_fails_with_empty_signature() {
        let (_, vk) = derive_keypair("test");
        let content = "content";
        let empty_sig: Vec<u8> = vec![];

        assert!(
            !verify_signature(&vk, content, &empty_sig),
            "Empty signature must fail verification"
        );
    }

    #[test]
    fn empty_passphrase_signing_roundtrip() {
        let (sk, vk) = derive_keypair("");
        let content = "{}";
        let (_, sig) = sign_content(&sk, content);
        assert!(verify_signature(&vk, content, &sig));
    }

    // ---- canonicalization ----

    #[test]
    fn canonicalize_block_empty() {
        assert_eq!(canonicalize_block(""), "\n");
    }

    #[test]
    fn canonicalize_block_strips_whitespace_and_appends_newline() {
        assert_eq!(canonicalize_block("  hello  \n\n"), "hello\n");
        assert_eq!(canonicalize_block("hello"), "hello\n");
    }

    #[test]
    fn canonicalize_block_idempotent_on_already_canonical() {
        let canon = canonicalize_block("hello");
        assert_eq!(canonicalize_block(&canon), canon);
    }

    #[test]
    fn canonicalize_all_results_empty_slice() {
        assert_eq!(canonicalize_all_results(&[]), "\n");
    }

    #[test]
    fn canonicalize_all_results_single_block() {
        let blocks = vec!["  Result [OK]\n".to_string()];
        assert_eq!(canonicalize_all_results(&blocks), "Result [OK]\n");
    }

    #[test]
    fn canonicalize_all_results_multiple_blocks_joined_with_newline() {
        let blocks = vec!["block a\n".to_string(), "block b\n".to_string()];
        let result = canonicalize_all_results(&blocks);
        assert_eq!(result, "block a\n\nblock b\n");
    }

    // ---- sign_snapshot / verify_snapshot ----

    #[test]
    fn sign_snapshot_roundtrip_empty_passphrase() {
        let input = "stamp hello world";
        let results = vec!["{\"chronon_number\":1}".to_string()];
        let comments = "test_name";
        let sig = sign_snapshot("", input, &results, comments);
        assert!(!sig.public_key_hex.is_empty());
        let v = verify_snapshot("", input, &results, comments, &sig);
        assert!(v.all_ok(), "Round-trip must pass: {:?}", v);
    }

    #[test]
    fn verify_snapshot_fails_tampered_input() {
        let input = "stamp hello world";
        let results = vec!["{\"chronon_number\":1}".to_string()];
        let comments = "test_name";
        let sig = sign_snapshot("", input, &results, comments);
        let v = verify_snapshot("", "stamp wrong content", &results, comments, &sig);
        assert!(!v.input_ok,    "Tampered input must fail input_ok");
        assert!(!v.result_ok,   "Tampered input must also fail result_ok (progressive)");
        assert!(!v.comments_ok, "Tampered input must also fail comments_ok (progressive)");
    }

    #[test]
    fn verify_snapshot_fails_tampered_results() {
        let input = "stamp hello world";
        let results = vec!["{\"chronon_number\":1}".to_string()];
        let comments = "test_name";
        let sig = sign_snapshot("", input, &results, comments);
        let tampered = vec!["{\"chronon_number\":99}".to_string()];
        let v = verify_snapshot("", input, &tampered, comments, &sig);
        assert!( v.input_ok,    "Untouched input must still pass input_ok");
        assert!(!v.result_ok,   "Tampered results must fail result_ok");
        assert!(!v.comments_ok, "Tampered results must also fail comments_ok (progressive)");
    }

    #[test]
    fn verify_snapshot_fails_tampered_comments() {
        let input = "stamp hello world";
        let results = vec!["{\"chronon_number\":1}".to_string()];
        let sig = sign_snapshot("", input, &results, "test_name");
        let v = verify_snapshot("", input, &results, "test_name\ntampered", &sig);
        assert!( v.input_ok,    "Untouched input must still pass input_ok");
        assert!( v.result_ok,   "Untouched results must still pass result_ok");
        assert!(!v.comments_ok, "Tampered comments must fail comments_ok");
    }

    #[test]
    fn verify_snapshot_key_mismatch_different_passphrase() {
        let input = "stamp hello world";
        let results = vec!["{\"chronon_number\":1}".to_string()];
        let comments = "test_name";
        let sig = sign_snapshot("human-secret", input, &results, comments);
        let v = verify_snapshot("", input, &results, comments, &sig);
        assert!(!v.key_match, "Wrong passphrase must not match key");
        assert!(v.input_ok);
        assert!(v.result_ok);
        assert!(v.comments_ok);
    }

    // ---- parse_snapshot_footer ----

    #[test]
    fn parse_snapshot_footer_well_formed() {
        let sig = sign_snapshot("", "stamp hello", &["result".to_string()], "test_name");
        let footer = sig.format_footer();
        let snapshot = format!("---\nsome content\n---\nINPUT:\n...\n{footer}\n");
        let parsed = parse_snapshot_footer(&snapshot).expect("should parse footer");
        assert_eq!(parsed, sig);
    }

    #[test]
    fn parse_snapshot_footer_missing_returns_none() {
        assert!(parse_snapshot_footer("").is_none());
        assert!(parse_snapshot_footer("no SIGNATURES label here").is_none());
    }

    #[test]
    fn parse_snapshot_footer_wrong_prefixes_returns_none() {
        let bad = "SIGNATURES:\nPublic key: abc\nBad prefix: xyz\nResult signature: def\nComments signature: jkl";
        assert!(parse_snapshot_footer(bad).is_none());
    }
}
