use std::sync::Once;

use foretias_core::core::bindings::{
    foretias_privkey_decrypt, foretias_privkey_encrypt, foretias_privkey_init,
    ForetiasResult_FORETIAS_ERR_BAD_SIG, ForetiasResult_FORETIAS_OK,
};

static INIT: Once = Once::new();

fn ensure_kek() {
    INIT.call_once(|| unsafe {
        foretias_privkey_init();
    });
}

fn roundtrip(pt_len: usize) {
    ensure_kek();

    let plaintext: Vec<u8> = (b'A'..=0xFF).cycle().take(pt_len).collect();
    let mut ciphertext = vec![0u8; pt_len + 16];
    let mut nonce = [0u8; 24];
    let mut decrypted = vec![0u8; pt_len];

    let rc = unsafe {
        foretias_privkey_encrypt(
            plaintext.as_ptr(),
            pt_len,
            ciphertext.as_mut_ptr(),
            nonce.as_mut_ptr(),
        )
    };
    assert_eq!(
        rc, ForetiasResult_FORETIAS_OK,
        "encrypt failed for size {}",
        pt_len
    );

    let rc = unsafe {
        foretias_privkey_decrypt(
            ciphertext.as_ptr(),
            pt_len + 16,
            nonce.as_ptr(),
            decrypted.as_mut_ptr(),
        )
    };
    assert_eq!(
        rc, ForetiasResult_FORETIAS_OK,
        "decrypt failed for size {}",
        pt_len
    );

    assert_eq!(
        plaintext, decrypted,
        "roundtrip mismatch for size {}",
        pt_len
    );
}

#[test]
fn test_encrypt_decrypt_roundtrip_32() {
    roundtrip(32);
}

#[test]
fn test_encrypt_decrypt_roundtrip_96() {
    roundtrip(96);
}

#[test]
fn test_encrypt_decrypt_roundtrip_128() {
    roundtrip(128);
}

#[test]
fn test_encrypt_decrypt_roundtrip_160() {
    roundtrip(160);
}

#[test]
fn test_encrypt_decrypt_roundtrip_1184() {
    roundtrip(1184);
}

#[test]
fn test_encrypt_decrypt_roundtrip_1632() {
    roundtrip(1632);
}

#[test]
fn test_encrypt_decrypt_roundtrip_4000() {
    roundtrip(4000);
}

#[test]
fn test_encrypt_decrypt_roundtrip_4112() {
    roundtrip(4112);
}

#[test]
fn test_decrypt_wrong_nonce_fails() {
    ensure_kek();

    let plaintext = [0u8; 32];
    let mut ciphertext = vec![0u8; 48];
    let mut nonce = [0u8; 24];
    let mut decrypted = vec![0u8; 32];

    let rc = unsafe {
        foretias_privkey_encrypt(
            plaintext.as_ptr(),
            32,
            ciphertext.as_mut_ptr(),
            nonce.as_mut_ptr(),
        )
    };
    assert_eq!(rc, ForetiasResult_FORETIAS_OK, "encrypt should succeed");

    let mut wrong_nonce = nonce;
    wrong_nonce[0] ^= 0xFF;

    let rc = unsafe {
        foretias_privkey_decrypt(
            ciphertext.as_ptr(),
            48,
            wrong_nonce.as_ptr(),
            decrypted.as_mut_ptr(),
        )
    };
    assert_eq!(
        rc, ForetiasResult_FORETIAS_ERR_BAD_SIG,
        "decrypt with wrong nonce must fail"
    );
}

#[test]
fn test_decrypt_tampered_ciphertext_fails() {
    ensure_kek();

    let plaintext = [0u8; 32];
    let mut ciphertext = vec![0u8; 48];
    let mut nonce = [0u8; 24];
    let mut decrypted = vec![0u8; 32];

    let rc = unsafe {
        foretias_privkey_encrypt(
            plaintext.as_ptr(),
            32,
            ciphertext.as_mut_ptr(),
            nonce.as_mut_ptr(),
        )
    };
    assert_eq!(rc, ForetiasResult_FORETIAS_OK, "encrypt should succeed");

    ciphertext[10] ^= 0xFF;

    let rc = unsafe {
        foretias_privkey_decrypt(
            ciphertext.as_ptr(),
            48,
            nonce.as_ptr(),
            decrypted.as_mut_ptr(),
        )
    };
    assert_eq!(
        rc, ForetiasResult_FORETIAS_ERR_BAD_SIG,
        "decrypt with tampered ciphertext must fail"
    );
}
