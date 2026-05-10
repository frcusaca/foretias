/* foretias JNI bridge — C wrapper for foretias_core.h
 *
 * Compiled as libforetias_java.so and loaded by Java via System.loadLibrary("foretias_java").
 * Links against libforetias_core.so (built by foretias/p2p/core-engine).
 */

#include <jni.h>
#include "foretias_core.h"
#include <string.h>

/* Helper: copy Java byte[] to C buffer */
static void jbyte_to_buf(JNIEnv *env, jbyteArray arr, uint8_t *buf, size_t len) {
    jbyte *bytes = (*env)->GetByteArrayElements(env, arr, NULL);
    memcpy(buf, bytes, len);
    (*env)->ReleaseByteArrayElements(env, arr, bytes, JNI_ABORT);
}

/* Helper: copy C buffer to Java byte[] */
static void buf_to_jbyte(JNIEnv *env, jbyteArray arr, const uint8_t *buf, size_t len) {
    jbyte *bytes = (*env)->GetByteArrayElements(env, arr, NULL);
    memcpy(bytes, buf, len);
    (*env)->ReleaseByteArrayElements(env, arr, bytes, 0);
}

/* ── Ed25519 keypair ─────────────────────────────────────────── */

JNIEXPORT jint JNICALL
Java_foretias_Crypto_ed25519GenerateKeyPair(JNIEnv *env, jobject this_obj,
                                             jbyteArray pub_out, jbyteArray priv_out) {
    ForetiasPubKey32 pub;
    ForetiasPrivKey32 priv;
    ForetiasResult rc = foretias_ed25519_generate_keypair(&pub, &priv);
    if (rc == FORETIAS_OK) {
        buf_to_jbyte(env, pub_out, pub.bytes, 32);
        buf_to_jbyte(env, priv_out, priv.bytes, 32);
        foretias_memzero(&priv, sizeof(priv));
    }
    return (jint)rc;
}

/* ── Ed25519 sign ────────────────────────────────────────────── */

JNIEXPORT jint JNICALL
Java_foretias_Crypto_ed25519SignNative(JNIEnv *env, jobject this_obj,
                                        jbyteArray privateKey, jbyteArray message,
                                        jbyteArray sig_out) {
    ForetiasPrivKey32 priv;
    jbyte_to_buf(env, privateKey, priv.bytes, 32);

    jsize msg_len = (*env)->GetArrayLength(env, message);
    uint8_t *msg = (uint8_t *)malloc((size_t)msg_len);
    jbyte_to_buf(env, message, msg, (size_t)msg_len);

    ForetiasSig64 sig;
    ForetiasResult rc = foretias_ed25519_sign(&priv, msg, (size_t)msg_len, &sig);
    free(msg);
    foretias_memzero(&priv, sizeof(priv));

    if (rc == FORETIAS_OK) {
        buf_to_jbyte(env, sig_out, sig.bytes, 64);
    }
    return (jint)rc;
}

/* ── Ed25519 verify ──────────────────────────────────────────── */

JNIEXPORT jint JNICALL
Java_foretias_Crypto_ed25519VerifyNative(JNIEnv *env, jobject this_obj,
                                          jbyteArray publicKey, jbyteArray message,
                                          jbyteArray signature) {
    ForetiasPubKey32 pub;
    ForetiasSig64 sig;
    jbyte_to_buf(env, publicKey, pub.bytes, 32);
    jbyte_to_buf(env, signature, sig.bytes, 64);

    jsize msg_len = (*env)->GetArrayLength(env, message);
    uint8_t *msg = (uint8_t *)malloc((size_t)msg_len);
    jbyte_to_buf(env, message, msg, (size_t)msg_len);

    ForetiasResult rc = foretias_ed25519_verify(&pub, msg, (size_t)msg_len, &sig);
    free(msg);
    return (jint)rc;
}

/* ── Peer ID derivation ──────────────────────────────────────── */

JNIEXPORT jint JNICALL
Java_foretias_Crypto_ed25519DerivePeerId(JNIEnv *env, jobject this_obj,
                                          jbyteArray publicKey, jbyteArray peerId_out) {
    ForetiasPubKey32 pub;
    ForetiasPeerID id;
    jbyte_to_buf(env, publicKey, pub.bytes, 32);

    ForetiasResult rc = foretias_ed25519_derive_peer_id(&pub, &id);
    if (rc == FORETIAS_OK) {
        buf_to_jbyte(env, peerId_out, id.bytes, 32);
    }
    return (jint)rc;
}

/* ── SHA-256 ─────────────────────────────────────────────────── */

JNIEXPORT jint JNICALL
Java_foretias_Crypto_hashSha256(JNIEnv *env, jobject this_obj,
                                 jbyteArray data, jbyteArray out) {
    jsize len = (*env)->GetArrayLength(env, data);
    uint8_t *d = (uint8_t *)malloc((size_t)len);
    jbyte_to_buf(env, data, d, (size_t)len);

    ForetiasHash32 hash;
    ForetiasResult rc = foretias_hash_sha256(d, (size_t)len, &hash);
    free(d);

    if (rc == FORETIAS_OK) {
        buf_to_jbyte(env, out, hash.bytes, 32);
    }
    return (jint)rc;
}

/* ── Blake3 ──────────────────────────────────────────────────── */

JNIEXPORT jint JNICALL
Java_foretias_Crypto_hashBlake3(JNIEnv *env, jobject this_obj,
                                 jbyteArray data, jbyteArray out) {
    jsize len = (*env)->GetArrayLength(env, data);
    uint8_t *d = (uint8_t *)malloc((size_t)len);
    jbyte_to_buf(env, data, d, (size_t)len);

    ForetiasHash32 hash;
    ForetiasResult rc = foretias_hash_blake3(d, (size_t)len, &hash);
    free(d);

    if (rc == FORETIAS_OK) {
        buf_to_jbyte(env, out, hash.bytes, 32);
    }
    return (jint)rc;
}

/* ── RNG ─────────────────────────────────────────────────────── */

JNIEXPORT jint JNICALL
Java_foretias_Crypto_rngBytes(JNIEnv *env, jobject this_obj, jbyteArray buf) {
    jsize len = (*env)->GetArrayLength(env, buf);
    uint8_t *b = (uint8_t *)malloc((size_t)len);
    ForetiasResult rc = foretias_rng_bytes(b, (size_t)len);
    if (rc == FORETIAS_OK) {
        buf_to_jbyte(env, buf, b, (size_t)len);
    }
    free(b);
    return (jint)rc;
}
