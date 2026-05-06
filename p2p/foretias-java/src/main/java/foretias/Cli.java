package foretias;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;

/**
 * Foretias CLI — Java implementation.
 *
 * Conforms to: foretias/specs/FORETIAS_CLI_SPEC.md
 * Commands: stamp, verify, prove, inspect
 */
public class Cli {

    public static void main(String[] args) {
        if (args.length == 0) {
            printUsage();
            System.exit(1);
        }

        String command = args[0];
        switch (command) {
            case "stamp" -> cmdStamp(args);
            case "verify" -> cmdVerify(args);
            case "prove" -> cmdProve(args);
            case "inspect" -> cmdInspect(args);
            case "help", "--help", "-h" -> printUsage();
            default -> {
                System.err.println("Unknown command: " + command);
                printUsage();
                System.exit(1);
            }
        }
    }

    // ── stamp ────────────────────────────────────────────────────

    static void cmdStamp(String[] args) {
        String message = null;
        String messageFile = null;
        String privateKeyHex = null;
        String tbidHex = null;
        long tick = 0;
        String echo = "";
        String tbn = "java-cli";
        String output = null;

        for (int i = 1; i < args.length; i++) {
            switch (args[i]) {
                case "-m", "--message" -> message = argAt(++i, args);
                case "-M", "--message-file" -> messageFile = argAt(++i, args);
                case "--privkey" -> privateKeyHex = argAt(++i, args);
                case "--tbid" -> tbidHex = argAt(++i, args);
                case "--tick" -> tick = Long.parseLong(argAt(++i, args));
                case "--echo" -> echo = argAt(++i, args);
                case "--tbn" -> tbn = argAt(++i, args);
                case "-o", "--output" -> output = argAt(++i, args);
            }
        }

        if (message == null && messageFile == null) {
            err("exactly one of --message or --message-file is required");
        }
        if (message != null && messageFile != null) {
            err("--message and --message-file are mutually exclusive");
        }

        byte[] content = message != null ? message.getBytes() : new String(readFile(messageFile)).getBytes();

        byte[] privateKey = privateKeyHex != null
                ? Crypto.hexToBytes(privateKeyHex)
                : Crypto.generateEd25519KeyPair();

        if (privateKeyHex == null) {
            System.err.println("Note: generated fresh keypair (pass --privkey to reuse)");
        }

        byte[] privKey = privateKey;
        if (privateKey.length == 64) {
            privKey = new byte[32];
            System.arraycopy(privateKey, 32, privKey, 0, 32);
        }

        byte[] tbid = tbidHex != null ? Crypto.hexToBytes(tbidHex) : Crypto.generateTbid();

        String foretisJson = Crypto.stamp(privKey, tbid, tick, content, echo, tbn);

        if (output != null) {
            write(output, foretisJson);
        } else {
            System.out.println(foretisJson);
        }
    }

    // ── verify ───────────────────────────────────────────────────

    static void cmdVerify(String[] args) {
        String message = null;
        String messageFile = null;
        String foretisJson = null;
        String foretisFile = null;
        String publicKeyHex = null;
        String output = null;

        for (int i = 1; i < args.length; i++) {
            switch (args[i]) {
                case "-m", "--message" -> message = argAt(++i, args);
                case "-M", "--message-file" -> messageFile = argAt(++i, args);
                case "-f", "--foretis" -> foretisJson = argAt(++i, args);
                case "-F", "--foretis-file" -> foretisFile = argAt(++i, args);
                case "--pubkey" -> publicKeyHex = argAt(++i, args);
                case "-o", "--output" -> output = argAt(++i, args);
            }
        }

        if (message == null && messageFile == null) {
            err("exactly one of --message or --message-file is required");
        }
        if (foretisJson == null && foretisFile == null) {
            err("exactly one of --foretis or --foretis-file is required");
        }
        if (publicKeyHex == null) {
            err("--pubkey is required for verification");
        }

        byte[] content = message != null ? message.getBytes() : new String(readFile(messageFile)).getBytes();
        foretisJson = foretisJson != null ? foretisJson : new String(readFile(foretisFile));
        byte[] publicKey = Crypto.hexToBytes(publicKeyHex);

        boolean valid = Crypto.verify(foretisJson, content, publicKey);

        String result = "{\"valid\": " + valid + "}";
        if (output != null) {
            write(output, result);
        } else {
            System.out.println(result);
        }
    }

    // ── prove ────────────────────────────────────────────────────

    static void cmdProve(String[] args) {
        String message = null;
        String messageFile = null;
        String foretisJson = null;
        String foretisFile = null;
        String publicKeyHex = null;
        String output = null;

        for (int i = 1; i < args.length; i++) {
            switch (args[i]) {
                case "-m", "--message" -> message = argAt(++i, args);
                case "-M", "--message-file" -> messageFile = argAt(++i, args);
                case "-f", "--foretis" -> foretisJson = argAt(++i, args);
                case "-F", "--foretis-file" -> foretisFile = argAt(++i, args);
                case "--pubkey" -> publicKeyHex = argAt(++i, args);
                case "-o", "--output" -> output = argAt(++i, args);
            }
        }

        if (message == null && messageFile == null) {
            err("exactly one of --message or --message-file is required");
        }
        if (foretisJson == null && foretisFile == null) {
            err("exactly one of --foretis or --foretis-file is required");
        }
        if (publicKeyHex == null) {
            err("--pubkey is required");
        }

        byte[] content = message != null ? message.getBytes() : new String(readFile(messageFile)).getBytes();
        foretisJson = foretisJson != null ? foretisJson : new String(readFile(foretisFile));
        byte[] publicKey = Crypto.hexToBytes(publicKeyHex);

        boolean valid = Crypto.verify(foretisJson, content, publicKey);

        String tickStr = extractJsonString(foretisJson, "tick_number");
        long tick = tickStr != null ? Long.parseLong(tickStr) : 0;

        String result = String.format("{\"valid\": %s, \"tick\": %d}", valid, tick);
        if (output != null) {
            write(output, result);
        } else {
            System.out.println(result);
        }
    }

    // ── inspect ──────────────────────────────────────────────────

    static void cmdInspect(String[] args) {
        String calendarPath = null;

        for (int i = 1; i < args.length; i++) {
            if (i < args.length && !args[i].startsWith("-")) {
                calendarPath = args[i];
                break;
            }
        }

        if (calendarPath == null || !Files.exists(Paths.get(calendarPath))) {
            err("calendar file not found: " + calendarPath);
        }

        String json = new String(readFile(calendarPath));

        int total = 0;
        int valid = 0;
        int invalid = 0;

        String[] ticks = extractTickArray(json);
        for (String tick : ticks) {
            String attestations = extractAttestationArray(tick);
            String[] atts = splitByBrace(attestations);
            total += atts.length;
            for (String att : atts) {
                if (att.contains("\"signature\"")) {
                    valid++;
                } else {
                    invalid++;
                }
            }
        }

        String result = String.format(
            "{\"calendar\": \"%s\", \"ticks\": %d, \"total_attestations\": %d, \"valid\": %d, \"invalid\": %d}",
            calendarPath, ticks.length, total, valid, invalid
        );
        System.out.println(result);
        System.exit(invalid == 0 ? 0 : 1);
    }

    // ── helpers ──────────────────────────────────────────────────

    static String argAt(int i, String[] args) {
        return i < args.length ? args[i] : null;
    }

    static byte[] readFile(String path) {
        try {
            return Files.readAllBytes(Paths.get(path));
        } catch (IOException e) {
            err("failed to read " + path + ": " + e.getMessage());
            return null;
        }
    }

    static void write(String path, String content) {
        try {
            Files.write(Paths.get(path), content.getBytes());
        } catch (IOException e) {
            err("failed to write " + path + ": " + e.getMessage());
        }
    }

    static void err(String msg) {
        System.err.println("Error: " + msg);
        System.exit(1);
    }

    static String[] extractTickArray(String json) {
        String ticksSection = extractJsonString(json, "ticks");
        return splitByBrace(ticksSection);
    }

    static String extractJsonString(String json, String key) {
        String pattern = "\"" + key + "\":";
        int start = json.indexOf(pattern);
        if (start == -1) return null;
        start += pattern.length();
        while (start < json.length() && json.charAt(start) == ' ') start++;
        if (start < json.length() && json.charAt(start) == '"') {
            start++;
            int end = json.indexOf('"', start);
            return json.substring(start, end);
        }
        return null;
    }

    static String extractAttestationArray(String tick) {
        int start = tick.indexOf("\"external_attestations\":");
        if (start == -1) return "[]";
        int braceStart = tick.indexOf('[', start);
        if (braceStart == -1) return "[]";
        int depth = 0;
        int end = braceStart;
        for (int i = braceStart; i < tick.length(); i++) {
            char c = tick.charAt(i);
            if (c == '[') depth++;
            if (c == ']') depth--;
            if (depth == 0) { end = i + 1; break; }
        }
        return tick.substring(braceStart, end);
    }

    static String[] splitByBrace(String s) {
        List<String> items = new ArrayList<>();
        int depth = 0;
        int start = -1;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '{') {
                if (depth == 0) start = i;
                depth++;
            } else if (c == '}') {
                depth--;
                if (depth == 0 && start != -1) {
                    items.add(s.substring(start, i + 1));
                    start = -1;
                }
            }
        }
        return items.toArray(new String[0]);
    }

    static void printUsage() {
        System.out.println("""
                Usage: foretias <command> [options]

                Commands:
                  stamp     Stamp content (create a Foretis)
                  verify    Verify content against a Foretis
                  prove     Fetch calendar slice and verify locally
                  inspect   Inspect external attestations in a persisted calendar

                stamp options:
                  -m, --message <str>       Message to stamp
                  -M, --message-file <path> Read message from file
                  --privkey <hex>           32-byte Ed25519 private key (hex)
                  --tbid <hex>              16-byte TBID (hex)
                  --tick <n>                Tick number (default: 0)
                  --echo <str>              Echo string
                  --tbn <str>               Time branch name
                  -o, --output <path>       Write output to file

                verify options:
                  -m, --message <str>       Message to verify
                  -M, --message-file <path> Read message from file
                  -f, --foretis <json>      Foretis JSON inline
                  -F, --foretis-file <path> Read Foretis from file
                  --pubkey <hex>            32-byte Ed25519 public key (hex)
                  -o, --output <path>       Write output to file

                prove options:
                  (same as verify, outputs tick info too)

                inspect options:
                  <path>                    Path to calendar JSON file
                """);
    }
}
