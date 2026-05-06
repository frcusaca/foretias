use ed25519_dalek::{SigningKey, VerifyingKey, Signer};
use jni::objects::{JByteArray, JObject};
use jni::sys::jint;
use jni::JNIEnv;
use rand::RngCore;
use sha2::{Digest, Sha256};

fn ok() -> jint { 0 }
fn fail() -> jint { -1 }

fn to_java<'local>(env: &mut JNIEnv<'local>, dst: JObject<'local>, src: &[u8]) -> jint {
    if dst.is_null() || src.is_empty() {
        return fail();
    }
    let ba = JByteArray::from(dst);
    let src_i8: Vec<i8> = src.iter().map(|&b| b as i8).collect();
    if env.set_byte_array_region(&ba, 0, &src_i8).is_err() {
        return fail();
    }
    ok()
}

fn from_java<'local>(env: &JNIEnv<'local>, src: JObject<'local>) -> Option<Vec<u8>> {
    if src.is_null() {
        return None;
    }
    let ba = JByteArray::from(src);
    let len = env.get_array_length(&ba).ok()? as usize;
    let mut buf = vec![0i8; len];
    if env.get_byte_array_region(&ba, 0, &mut buf).is_err() {
        return None;
    }
    Some(buf.iter().map(|&b| b as u8).collect())
}

#[no_mangle]
pub extern "system" fn Java_foretias_Crypto_ed25519GenerateKeyPair<'a>(
    mut env: JNIEnv<'a>,
    _class: JObject<'a>,
    pub_out: JObject<'a>,
    priv_out: JObject<'a>,
) -> jint {
    let kp = SigningKey::generate(&mut rand::thread_rng());
    let vk: VerifyingKey = kp.verifying_key().clone();
    if to_java(&mut env, pub_out, vk.as_bytes()) != ok() {
        return fail();
    }
    to_java(&mut env, priv_out, &kp.to_bytes())
}

#[no_mangle]
pub extern "system" fn Java_foretias_Crypto_ed25519SignNative<'a>(
    mut env: JNIEnv<'a>,
    _class: JObject<'a>,
    priv_key: JObject<'a>,
    message: JObject<'a>,
    sig_out: JObject<'a>,
) -> jint {
    let priv_bytes = match from_java(&env, priv_key) {
        Some(b) => b,
        None => return fail(),
    };
    let msg = match from_java(&env, message) {
        Some(b) => b,
        None => return fail(),
    };
    if priv_bytes.len() != 32 {
        return fail();
    }
    let key_arr: [u8; 32] = match priv_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return fail(),
    };
    let sk = SigningKey::from_bytes(&key_arr);
    let sig = sk.sign(&msg);
    to_java(&mut env, sig_out, &sig.to_bytes())
}

#[no_mangle]
pub extern "system" fn Java_foretias_Crypto_ed25519VerifyNative<'a>(
    env: JNIEnv<'a>,
    _class: JObject<'a>,
    pub_key: JObject<'a>,
    message: JObject<'a>,
    signature: JObject<'a>,
) -> jint {
    let pub_bytes = match from_java(&env, pub_key) {
        Some(b) => b,
        None => return fail(),
    };
    let msg = match from_java(&env, message) {
        Some(b) => b,
        None => return fail(),
    };
    let sig_bytes = match from_java(&env, signature) {
        Some(b) => b,
        None => return fail(),
    };
    if pub_bytes.len() != 32 || sig_bytes.len() != 64 {
        return fail();
    }
    let pk_arr: [u8; 32] = match pub_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return fail(),
    };
    let sig_arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return fail(),
    };
    let vk = match VerifyingKey::from_bytes(&pk_arr) {
        Ok(k) => k,
        Err(_) => return fail(),
    };
    let ed_sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
    match vk.verify_strict(&msg, &ed_sig) {
        Ok(()) => ok(),
        Err(_) => fail(),
    }
}

#[no_mangle]
pub extern "system" fn Java_foretias_Crypto_ed25519DerivePeerId<'a>(
    mut env: JNIEnv<'a>,
    _class: JObject<'a>,
    pub_key: JObject<'a>,
    peer_id_out: JObject<'a>,
) -> jint {
    let pub_bytes = match from_java(&env, pub_key) {
        Some(b) => b,
        None => return fail(),
    };
    if pub_bytes.len() != 32 {
        return fail();
    }
    let pk_arr: [u8; 32] = match pub_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return fail(),
    };
    let vk = match VerifyingKey::from_bytes(&pk_arr) {
        Ok(k) => k,
        Err(_) => return fail(),
    };
    let peer_id = Sha256::digest(vk.as_bytes());
    to_java(&mut env, peer_id_out, &peer_id)
}

#[no_mangle]
pub extern "system" fn Java_foretias_Crypto_hashSha256<'a>(
    mut env: JNIEnv<'a>,
    _class: JObject<'a>,
    data: JObject<'a>,
    out: JObject<'a>,
) -> jint {
    let d = match from_java(&env, data) {
        Some(b) => b,
        None => return fail(),
    };
    let hash = Sha256::digest(&d);
    to_java(&mut env, out, &hash)
}

#[no_mangle]
pub extern "system" fn Java_foretias_Crypto_hashBlake3<'a>(
    mut env: JNIEnv<'a>,
    _class: JObject<'a>,
    data: JObject<'a>,
    out: JObject<'a>,
) -> jint {
    let d = match from_java(&env, data) {
        Some(b) => b,
        None => return fail(),
    };
    let hash = blake3::hash(&d).as_bytes().to_vec();
    to_java(&mut env, out, &hash)
}

#[no_mangle]
pub extern "system" fn Java_foretias_Crypto_rngBytes<'local>(
    env: JNIEnv<'local>,
    _class: JObject<'local>,
    buf: JObject<'local>,
) -> jint {
    if buf.is_null() {
        return fail();
    }
    let ba = JByteArray::from(buf);
    let len = match env.get_array_length(&ba) {
        Ok(l) if l > 0 => l as usize,
        _ => return fail(),
    };
    let mut bytes = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut bytes);
    let bytes_i8: Vec<i8> = bytes.iter().map(|&b| b as i8).collect();
    env.set_byte_array_region(&ba, 0, &bytes_i8).map(|_| ok()).unwrap_or(fail())
}
