use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::SignatureBytes;

pub fn mlkem_768_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    let mut secret: ForetiasKemSecretKey = unsafe { std::mem::zeroed() };
    let mut public: ForetiasKemPubKey = unsafe { std::mem::zeroed() };
    let rc = unsafe { foretias_mlkem_768_keypair(&mut secret, &mut public) };
    c_result_to_error(rc)?;
    let secret_bytes = secret.bytes[..secret.len as usize].to_vec();
    let public_bytes = public.bytes[..public.len as usize].to_vec();
    // Zero C struct after extraction (defense-in-depth)
    unsafe {
        foretias_memzero(
            &mut secret as *mut _ as *mut _,
            std::mem::size_of::<ForetiasKemSecretKey>(),
        );
    }
    Ok((
        SignatureBytes::from(public_bytes),
        SignatureBytes::from(secret_bytes),
    ))
}

pub fn mlkem_768_encapsulate(
    public_key: &SignatureBytes,
) -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    let mut pk: ForetiasKemPubKey = unsafe { std::mem::zeroed() };
    pk.bytes[..public_key.len()].copy_from_slice(public_key);
    pk.len = public_key.len();
    let mut ct: ForetiasKemCiphertext = unsafe { std::mem::zeroed() };
    let mut ss: [u8; 32] = [0u8; 32];
    let rc = unsafe { foretias_mlkem_768_encapsulate(&pk, &mut ct, ss.as_mut_ptr()) };
    c_result_to_error(rc)?;
    let ct_bytes = ct.bytes[..ct.len as usize].to_vec();
    Ok((
        SignatureBytes::from(ct_bytes),
        SignatureBytes::from(ss.to_vec()),
    ))
}

pub fn mlkem_768_decapsulate(
    secret_key: &SignatureBytes,
    ciphertext: &SignatureBytes,
) -> Result<SignatureBytes, CryptoError> {
    let mut sk: ForetiasKemSecretKey = unsafe { std::mem::zeroed() };
    sk.bytes[..secret_key.len()].copy_from_slice(secret_key);
    sk.len = secret_key.len();
    let mut ct: ForetiasKemCiphertext = unsafe { std::mem::zeroed() };
    ct.bytes[..ciphertext.len()].copy_from_slice(ciphertext);
    ct.len = ciphertext.len();
    let mut ss: [u8; 32] = [0u8; 32];
    let rc = unsafe { foretias_mlkem_768_decapsulate(&sk, &ct, ss.as_mut_ptr()) };
    c_result_to_error(rc)?;
    Ok(ss.to_vec().into())
}
