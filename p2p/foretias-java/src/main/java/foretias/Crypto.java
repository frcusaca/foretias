package foretias;

import java.nio.charset.StandardCharsets;
import java.util.Objects;

/**
 * JNI wrapper for foretias_core.h Ed25519 crypto operations.
 *
 * Native library: libforetias_java.so (compiled from jni_crypto.c)
 * Load via System.loadLibrary("foretias_java")
 */
public final class Crypto {

    private Crypto() {}

    static {
        try {
            System.loadLibrary("foretias_java");
        } catch (UnsatisfiedLinkError e) {
            throw new RuntimeException("Failed to load foretias_java native library", e);
        }
    }

    // ── Key generation ──────────────────────────────────────────

    public static byte[] generateEd25519PublicKey() {
        byte[] pub = new byte[32];
        byte[] priv = new byte[32];
        int rc = ed25519GenerateKeyPair(pub, priv);
        checkResult(rc);
        return pub;
    }

    public static byte[] generateEd25519KeyPair() {
        byte[] pub = new byte[32];
        byte[] priv = new byte[32];
        int rc = ed25519GenerateKeyPair(pub, priv);
        checkResult(rc);
        return concat(pub, priv);
    }

    // ── Sign ────────────────────────────────────────────────────

    public static byte[] ed25519Sign(byte[] privateKey, byte[] message) {
        Objects.requireNonNull(privateKey, "privateKey");
        Objects.requireNonNull(message, "message");
        if (privateKey.length != 32) throw new IllegalArgumentException("privateKey must be 32 bytes");
        byte[] sig = new byte[64];
        int rc = ed25519SignNative(privateKey, message, sig);
        checkResult(rc);
        return sig;
    }

    // ── Verify ──────────────────────────────────────────────────

    public static boolean ed25519Verify(byte[] publicKey, byte[] message, byte[] signature) {
        Objects.requireNonNull(publicKey, "publicKey");
        Objects.requireNonNull(message, "message");
        Objects.requireNonNull(signature, "signature");
        if (publicKey.length != 32) throw new IllegalArgumentException("publicKey must be 32 bytes");
        if (signature.length != 64) throw new IllegalArgumentException("signature must be 64 bytes");
        int rc = ed25519VerifyNative(publicKey, message, signature);
        return rc == 0;
    }

    // ── Peer ID derivation ──────────────────────────────────────

    public static byte[] derivePeerId(byte[] publicKey) {
        Objects.requireNonNull(publicKey);
        if (publicKey.length != 32) throw new IllegalArgumentException("publicKey must be 32 bytes");
        byte[] peerId = new byte[32];
        int rc = ed25519DerivePeerId(publicKey, peerId);
        checkResult(rc);
        return peerId;
    }

    // ── Hashing ─────────────────────────────────────────────────

    public static byte[] sha256(byte[] data) {
        byte[] out = new byte[32];
        int rc = hashSha256(data, out);
        checkResult(rc);
        return out;
    }

    public static byte[] blake3(byte[] data) {
        byte[] out = new byte[32];
        int rc = hashBlake3(data, out);
        checkResult(rc);
        return out;
    }

    // ── RNG ─────────────────────────────────────────────────────

    public static byte[] randomBytes(int len) {
        byte[] buf = new byte[len];
        int rc = rngBytes(buf);
        checkResult(rc);
        return buf;
    }

    // ── TBID generation ─────────────────────────────────────────

    public static byte[] generateTbid() {
        return randomBytes(16);
    }

    // ── Foretis stamp ───────────────────────────────────────────

    /**
     * Create a Foretis stamp: SHA-256(content) → sign with Ed25519.
     * Returns JSON string of the Foretis object.
     */
    public static String stamp(byte[] privateKey, byte[] tbid, long tickNumber, byte[] content, String echo, String tbn) {
        byte[] contentHash = sha256(content);
        byte[] signature = ed25519Sign(privateKey, contentHash);
        String ts = String.valueOf(System.currentTimeMillis());
        return String.format(
            "{\"tick_number\":%d,\"content_hash\":\"%s\",\"signature\":\"%s\",\"tbid\":\"%s\",\"echo\":\"%s\",\"tbn\":\"%s\",\"time_being_reference_time\":\"%s\"}",
            tickNumber, hex(contentHash), hex(signature), hex(tbid), escapeJson(echo), escapeJson(tbn), ts
        );
    }

    /**
     * Verify a Foretis stamp.
     * @param foretisJson JSON string of the Foretis
     * @param content original content bytes
     * @param publicKey verifier's public key
     * @return true if valid
     */
    public static boolean verify(String foretisJson, byte[] content, byte[] publicKey) {
        byte[] contentHash = sha256(content);
        String sigHex = extractJsonString(foretisJson, "signature");
        byte[] signature = hexToBytes(sigHex);
        return ed25519Verify(publicKey, contentHash, signature);
    }

    // ── Native methods ──────────────────────────────────────────

    private static native int ed25519GenerateKeyPair(byte[] pubOut, byte[] privOut);
    private static native int ed25519SignNative(byte[] privateKey, byte[] message, byte[] sigOut);
    private static native int ed25519VerifyNative(byte[] publicKey, byte[] message, byte[] signature);
    private static native int ed25519DerivePeerId(byte[] publicKey, byte[] peerIdOut);
    private static native int hashSha256(byte[] data, byte[] out);
    private static native int hashBlake3(byte[] data, byte[] out);
    private static native int rngBytes(byte[] buf);

    // ── Helpers ─────────────────────────────────────────────────

    private static void checkResult(int rc) {
        if (rc != 0) {
            throw new RuntimeException("foretias_core error: code " + rc);
        }
    }

    public static String hex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }

    public static byte[] hexToBytes(String hex) {
        byte[] bytes = new byte[hex.length() / 2];
        for (int i = 0; i < hex.length(); i += 2) {
            bytes[i / 2] = (byte) Integer.parseInt(hex.substring(i, i + 2), 16);
        }
        return bytes;
    }

    private static byte[] concat(byte[] a, byte[] b) {
        byte[] out = new byte[a.length + b.length];
        System.arraycopy(a, 0, out, 0, a.length);
        System.arraycopy(b, 0, out, a.length, b.length);
        return out;
    }

    private static String escapeJson(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"");
    }

    private static String extractJsonString(String json, String key) {
        String pattern = "\"" + key + "\":\"";
        int start = json.indexOf(pattern);
        if (start == -1) throw new IllegalArgumentException("key not found: " + key);
        start += pattern.length();
        int end = json.indexOf('"', start);
        return json.substring(start, end);
    }
}
