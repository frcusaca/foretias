package foretias;

/**
 * Integration test: Java stamp/verify roundtrip via Rust JNI.
 *
 * Run:
 *   LD_LIBRARY_PATH=../../target/release \
 *   java --enable-native-access=ALL-UNNAMED \
 *     -cp target/classes:~/.m2/repository/com/google/code/gson/gson/2.10.1/gson-2.10.1.jar \
 *     foretias.IntegrationTest
 */
public class IntegrationTest {
    static int passed = 0;
    static int failed = 0;

    public static void main(String[] args) {
        System.out.println("=== Foretias Java JNI Integration Tests ===\n");

        testKeyGeneration();
        testSignVerify();
        testSha256();
        testBlake3();
        testRng();
        testDerivePeerId();
        testStampVerify();

        System.out.println("\n=== Results: " + passed + " passed, " + failed + " failed ===");
        if (failed > 0) {
            System.exit(1);
        }
    }

    static void assertEq(String name, Object expected, Object actual) {
        if (expected.equals(actual)) {
            System.out.println("  PASS: " + name);
            passed++;
        } else {
            System.out.println("  FAIL: " + name + " expected=" + expected + " actual=" + actual);
            failed++;
        }
    }

    static void assertTrue(String name, boolean condition) {
        if (condition) {
            System.out.println("  PASS: " + name);
            passed++;
        } else {
            System.out.println("  FAIL: " + name);
            failed++;
        }
    }

    static void testKeyGeneration() {
        System.out.println("[1] Key Generation");
        byte[] keypair = Crypto.generateEd25519KeyPair();
        assertTrue("keypair length is 64", keypair.length == 64);

        byte[] pubOnly = Crypto.generateEd25519PublicKey();
        assertTrue("generateEd25519PublicKey returns 32 bytes", pubOnly.length == 32);
    }

    static void testSignVerify() {
        System.out.println("[2] Sign / Verify");
        byte[] keypair = Crypto.generateEd25519KeyPair();
        byte[] pub = new byte[32];
        byte[] priv = new byte[32];
        System.arraycopy(keypair, 0, pub, 0, 32);
        System.arraycopy(keypair, 32, priv, 0, 32);

        byte[] msg = "integration test".getBytes();
        byte[] sig = Crypto.ed25519Sign(priv, msg);
        assertTrue("signature length is 64", sig.length == 64);

        boolean valid = Crypto.ed25519Verify(pub, msg, sig);
        assertTrue("valid signature verifies", valid);

        byte[] badMsg = "tampered".getBytes();
        boolean invalid = Crypto.ed25519Verify(pub, badMsg, sig);
        assertTrue("tampered message fails", !invalid);
    }

    static void testSha256() {
        System.out.println("[3] SHA-256");
        byte[] hash = Crypto.sha256("".getBytes());
        String hex = Crypto.hex(hash);
        assertEq("SHA-256 of empty string",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", hex);
    }

    static void testBlake3() {
        System.out.println("[4] Blake3");
        byte[] hash = Crypto.blake3("test".getBytes());
        assertTrue("Blake3 hash is 32 bytes", hash.length == 32);
        String hex = Crypto.hex(hash);
        String known = "4878ca0425c739fa427f7eda20fe845f6b2e46ba5fe2a14df5b1e32f50603215";
        assertEq("Blake3 of 'test'", known, hex);
    }

    static void testRng() {
        System.out.println("[5] RNG");
        byte[] a = Crypto.randomBytes(32);
        byte[] b = Crypto.randomBytes(32);
        assertTrue("RNG produces different outputs", !Crypto.hex(a).equals(Crypto.hex(b)));
    }

    static void testDerivePeerId() {
        System.out.println("[6] Peer ID Derivation");
        byte[] keypair = Crypto.generateEd25519KeyPair();
        byte[] pub = new byte[32];
        System.arraycopy(keypair, 0, pub, 0, 32);

        byte[] peerId = Crypto.derivePeerId(pub);
        assertTrue("peer ID is 32 bytes", peerId.length == 32);

        byte[] peerId2 = Crypto.derivePeerId(pub);
        assertTrue("same pub yields same peer ID",
            Crypto.hex(peerId).equals(Crypto.hex(peerId2)));
    }

    static void testStampVerify() {
        System.out.println("[7] Stamp / Verify (full roundtrip)");
        byte[] keypair = Crypto.generateEd25519KeyPair();
        byte[] pub = new byte[32];
        byte[] priv = new byte[32];
        System.arraycopy(keypair, 0, pub, 0, 32);
        System.arraycopy(keypair, 32, priv, 0, 32);

        byte[] tbid = Crypto.generateTbid();
        String content = "foretias integration test";
        String stamp = Crypto.stamp(priv, tbid, 42, content.getBytes(), "echo1", "tbn1");

        assertTrue("stamp contains tick_number", stamp.contains("\"tick_number\":42"));
        assertTrue("stamp contains content_hash", stamp.contains("\"content_hash\":\""));
        assertTrue("stamp contains signature", stamp.contains("\"signature\":\""));
        assertTrue("stamp contains tbid", stamp.contains("\"tbid\":\""));

        boolean valid = Crypto.verify(stamp, content.getBytes(), pub);
        assertTrue("stamp verifies correctly", valid);

        String tampered = "different content";
        boolean invalid = Crypto.verify(stamp, tampered.getBytes(), pub);
        assertTrue("tampered content fails verification", !invalid);
    }
}
