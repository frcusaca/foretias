//! Encrypted JSONL calendar store.
//!
//! Each block of tick records is serialized to JSON, sealed via the node's
//! crypto server, CBOR-encoded, base64-encoded, and appended as a single line
//! to a JSONL file.
//!
//! On read, each line is base64-decoded, CBOR-decoded, unsealed, and
//! deserialized back into a [`CalendarBlock`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use foretias_core::crypto_server::{CryptoServer, SealedBlob};
use foretias_core::foretias::TickRecord;
use foretias_core::error::NodeError;
use serde::{Deserialize, Serialize};

/// A single block stored in the encrypted JSONL file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarBlock {
    /// Monotonically increasing block identifier.
    pub block_id: u64,
    /// Wall-clock nanoseconds when the block was written.
    pub written_at_ns: u64,
    /// Tick records contained in this block.
    pub ticks: Vec<TickRecord>,
}

/// Encrypted append-only calendar store backed by a JSONL file.
///
/// Each line in the file is a base64-encoded CBOR-encoded sealed blob
/// containing a [`CalendarBlock`].
pub struct EncryptedJsonlCalendarStore {
    path: PathBuf,
    server: Arc<dyn CryptoServer>,
    next_block_id: AtomicU64,
}

impl EncryptedJsonlCalendarStore {
    /// Creates a new store backed by the file at `path`.
    ///
    /// If the file already exists, the next block ID is computed from
    /// the highest block ID found in the file.
    pub fn new(path: PathBuf, server: Arc<dyn CryptoServer>) -> Self {
        let next_block_id = Self::compute_next_block_id(&path, &server).unwrap_or(0);
        Self {
            path,
            server,
            next_block_id: AtomicU64::new(next_block_id),
        }
    }

    /// Appends a block of ticks to the store.
    ///
    /// The ticks are wrapped in a [`CalendarBlock`], serialized to JSON,
    /// sealed with the node's seal key, CBOR-encoded, base64-encoded,
    /// and appended as a newline-terminated line to the backing file.
    pub fn append_block(&self, ticks: Vec<TickRecord>) -> Result<(), NodeError> {
        let block_id = self.next_block_id.fetch_add(1, Ordering::SeqCst);
        let written_at_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| NodeError::Internal(format!("SystemTime before UNIX_EPOCH: {}", e)))?
            .as_nanos() as u64;

        let block = CalendarBlock {
            block_id,
            written_at_ns,
            ticks,
        };

        // Serialize block to JSON
        let json_bytes = serde_json::to_vec(&block)?;

        // Seal with crypto server
        let sealed = self.server.seal_for_self(&json_bytes)?;

        // CBOR encode the sealed blob
        let cbor_bytes = serde_cbor::to_vec(&sealed)
            .map_err(|e| NodeError::Internal(format!("CBOR encode error: {}", e)))?;

        // Base64 encode
        let line = STANDARD.encode(&cbor_bytes);

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Append line to file
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        use std::io::Write;
        writeln!(file, "{}", line)?;

        Ok(())
    }

    /// Reads all blocks from the store in order.
    ///
    /// Each line is base64-decoded, CBOR-decoded to a [`SealedBlob`],
    /// unsealed, and JSON-deserialized into a [`CalendarBlock`].
    pub fn read_all(&self) -> Result<Vec<CalendarBlock>, NodeError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let contents = std::fs::read_to_string(&self.path)?;
        let mut blocks = Vec::new();

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Base64 decode
            let cbor_bytes = STANDARD
                .decode(line)
                .map_err(|e| NodeError::Internal(format!("base64 decode error: {}", e)))?;

            // CBOR decode to SealedBlob
            let sealed: SealedBlob = serde_cbor::from_slice(&cbor_bytes)
                .map_err(|e| NodeError::Internal(format!("CBOR decode error: {}", e)))?;

            // Unseal
            let json_bytes = self.server.unseal_for_self(&sealed)?;

            // JSON deserialize
            let block: CalendarBlock = serde_json::from_slice(&json_bytes)?;
            blocks.push(block);
        }

        Ok(blocks)
    }

    /// Returns the path to the backing file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Computes the next block ID from an existing file.
    fn compute_next_block_id(path: &Path, server: &Arc<dyn CryptoServer>) -> Option<u64> {
        if !path.exists() {
            return Some(0);
        }
        let contents = std::fs::read_to_string(path).ok()?;
        let mut max_id: u64 = 0;
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let cbor_bytes = STANDARD.decode(line).ok()?;
            let sealed: SealedBlob = serde_cbor::from_slice(&cbor_bytes).ok()?;
            let json_bytes = server.unseal_for_self(&sealed).ok()?;
            let block: CalendarBlock = serde_json::from_slice(&json_bytes).ok()?;
            if block.block_id >= max_id {
                max_id = block.block_id + 1;
            }
        }
        Some(max_id)
    }
}

/// Returns `true` if the file at `path` appears to be plaintext JSON
/// (i.e. a v0.1-format calendar file that starts with `{`).
pub fn looks_like_plaintext_json(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.trim_start().starts_with('{'))
        .unwrap_or(false)
}

/// Migrates a plaintext JSON calendar file to the encrypted JSONL format.
///
/// The plaintext file is expected to be a JSON object with a `"ticks"` field
/// (matching the v0.1 `Calendar` format). Each tick is written as a single
/// block in the encrypted store. The original file is left untouched.
pub fn migrate_plaintext(
    plaintext_path: &Path,
    store: &EncryptedJsonlCalendarStore,
) -> Result<usize, NodeError> {
    let contents = std::fs::read_to_string(plaintext_path)?;

    // Parse as the v0.1 Calendar format (tbid, tbn, ticks)
    #[derive(Deserialize)]
    struct PlaintextCalendar {
        ticks: Vec<TickRecord>,
    }

    let cal: PlaintextCalendar = serde_json::from_str(&contents)?;
    let count = cal.ticks.len();

    if !cal.ticks.is_empty() {
        store.append_block(cal.ticks)?;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::crypto_server;

    fn make_server() -> Arc<dyn CryptoServer> {
        let server: Box<dyn CryptoServer> = crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519)
            .expect("failed to create software crypto server");
        Arc::from(server)
    }

    fn make_tick(tick_number: u64) -> TickRecord {
        TickRecord {
            tick_number,
            public_key: vec![0u8; 32],
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![],
            backward_foretis: vec![],
            aa_nonce: [0u8; 16],
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: Vec::new(),
            tb_version: 0,
        }
    }

    #[test]
    fn encrypted_jsonl_append_read() {
        let tmp_dir = std::env::temp_dir().join(format!("foretias-ejl-test-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        let server = make_server();
        let store = EncryptedJsonlCalendarStore::new(path.clone(), server.clone());

        // Append 5 blocks
        for i in 0..5u64 {
            let ticks = vec![make_tick(i), make_tick(i + 100)];
            store.append_block(ticks).unwrap();
        }

        // Read all back
        let blocks = store.read_all().unwrap();
        assert_eq!(blocks.len(), 5);

        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block.block_id, i as u64);
            assert_eq!(block.ticks.len(), 2);
            assert_eq!(block.ticks[0].tick_number, i as u64);
            assert_eq!(block.ticks[1].tick_number, (i as u64 + 100));
            assert!(block.written_at_ns > 0);
        }

        // Cleanup
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn plaintext_migration() {
        let tmp_dir = std::env::temp_dir().join(format!("foretias-migration-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let plaintext_path = tmp_dir.join("calendar.json");
        let encrypted_path = tmp_dir.join("calendar.jsonl");

        // Write a v0.1-format plaintext calendar
        let pk: Vec<u8> = vec![0u8; 32];
        let nonce: Vec<u8> = vec![0u8; 16];
        let cal_json = serde_json::json!({
            "tbid": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            "tbn": "legacy-cal",
            "stamp_tbid": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            "ticks": [
                {
                    "tick_number": 1,
                    "public_key": pk.clone(),
                    "forward_foretis": [],
                    "backward_foretis": [],
                    "aa_nonce": nonce.clone(),
                    "external_attestations": []
                },
                {
                    "tick_number": 2,
                    "public_key": pk,
                    "forward_foretis": [],
                    "backward_foretis": [],
                    "aa_nonce": nonce,
                    "external_attestations": []
                }
            ]
        });
        std::fs::write(&plaintext_path, cal_json.to_string()).unwrap();

        // Verify it looks like plaintext
        assert!(looks_like_plaintext_json(&plaintext_path));

        // Migrate
        let server = make_server();
        let store = EncryptedJsonlCalendarStore::new(encrypted_path.clone(), server.clone());
        let migrated = migrate_plaintext(&plaintext_path, &store).unwrap();
        assert_eq!(migrated, 2);

        // Read back from encrypted store
        let blocks = store.read_all().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].ticks.len(), 2);
        assert_eq!(blocks[0].ticks[0].tick_number, 1);
        assert_eq!(blocks[0].ticks[1].tick_number, 2);

        // Cleanup
        std::fs::remove_file(&plaintext_path).ok();
        std::fs::remove_file(&encrypted_path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn read_all_empty_file_returns_empty() {
        let tmp_dir = std::env::temp_dir().join(format!("foretias-empty-test-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        let server = make_server();
        let store = EncryptedJsonlCalendarStore::new(path.clone(), server);

        let blocks = store.read_all().unwrap();
        assert!(blocks.is_empty());
    }

    #[test]
    fn looks_like_plaintext_json_detects_json() {
        let tmp_dir = std::env::temp_dir().join(format!("foretias-detect-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let json_path = tmp_dir.join("data.json");
        std::fs::write(&json_path, "  \n  {\"key\": \"value\"}").unwrap();
        assert!(looks_like_plaintext_json(&json_path));

        let bin_path = tmp_dir.join("data.bin");
        std::fs::write(&bin_path, "dGVzdA==\n").unwrap();
        assert!(!looks_like_plaintext_json(&bin_path));

        let missing_path = tmp_dir.join("missing.json");
        assert!(!looks_like_plaintext_json(&missing_path));

        std::fs::remove_file(&json_path).ok();
        std::fs::remove_file(&bin_path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }
}
