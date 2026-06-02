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
use foretias_core::clock::{Clock, SystemClock};
use foretias_core::core::merkle::{merkle_leaf, merkle_root_from_leaves};
use foretias_core::crypto_server::{CryptoServer, SealedBlob};
use foretias_core::error::NodeError;
use foretias_core::foretias::clean_auth::{CleanAuthenticated, Externalized};
use foretias_core::foretias::ChrononRecord;
use serde::{Deserialize, Serialize};

/// A single block stored in the encrypted JSONL file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarBlock {
    /// Monotonically increasing block identifier.
    pub block_id: u64,
    /// Wall-clock nanoseconds when the block was written.
    pub written_at_ns: u64,
    /// Tick records contained in this block.
    pub ticks: Vec<Externalized<ChrononRecord>>,
    /// Merkle root over the tick records in this block.
    ///
    /// Computed by hashing each tick's canonical JSON serialization as a
    /// Merkle leaf (SHA-256(0x00 || json_bytes)), then calling
    /// [`merkle_root_from_leaves`].
    ///
    /// Defaults to `[0; 32]` for blocks written before this field was added.
    #[serde(default)]
    pub merkle_root: [u8; 32],
}

impl CalendarBlock {
    /// Compute the Merkle root over this block's tick records.
    ///
    /// Each tick is serialized to canonical JSON bytes, hashed as a Merkle
    /// leaf (SHA-256(0x00 || json_bytes)), and the resulting leaf hashes
    /// are combined into a Merkle root via the C11 FFI.
    ///
    /// Returns `[0; 32]` if the block has no ticks.
    pub fn compute_merkle_root(&self) -> Result<[u8; 32], NodeError> {
        if self.ticks.is_empty() {
            return Ok([0u8; 32]);
        }

        let mut leaves = Vec::with_capacity(self.ticks.len());
        for tick in &self.ticks {
            let canonical = serde_json::to_vec(tick.inner())
                .map_err(|e| NodeError::Internal(format!("tick serialize error: {e}")))?;
            let leaf = merkle_leaf(&canonical)?;
            leaves.push(leaf);
        }

        let root = merkle_root_from_leaves(&leaves)?;
        Ok(root.bytes)
    }
}

/// Encrypted append-only calendar store backed by a JSONL file.
///
/// Each line in the file is a base64-encoded CBOR-encoded sealed blob
/// containing a [`CalendarBlock`].
pub struct EncryptedJsonlCalendarStore {
    path: PathBuf,
    server: Arc<dyn CryptoServer>,
    clock: Arc<dyn Clock>,
    next_block_id: AtomicU64,
}

impl EncryptedJsonlCalendarStore {
    /// Creates a new store backed by the file at `path`.
    ///
    /// If the file already exists, the next block ID is computed from
    /// the highest block ID found in the file.
    pub fn new(path: PathBuf, server: Arc<dyn CryptoServer>) -> Self {
        Self::with_clock(path, server, Arc::new(SystemClock))
    }

    /// Creates a store with an injected clock (for testing).
    pub fn with_clock(path: PathBuf, server: Arc<dyn CryptoServer>, clock: Arc<dyn Clock>) -> Self {
        let next_block_id = Self::compute_next_block_id(&path, &server).unwrap_or(0);
        Self {
            path,
            server,
            clock,
            next_block_id: AtomicU64::new(next_block_id),
        }
    }

    /// Appends a block of ticks to the store.
    ///
    /// The ticks are wrapped in a [`CalendarBlock`], serialized to JSON,
    /// sealed with the node's seal key, CBOR-encoded, base64-encoded,
    /// and appended as a newline-terminated line to the backing file.
    pub fn append_block(&self, ticks: Vec<Externalized<ChrononRecord>>) -> Result<(), NodeError> {
        let block_id = self.next_block_id.fetch_add(1, Ordering::SeqCst);
        let written_at_ns = self
            .clock
            .now_ns()
            .map_err(|e| NodeError::Internal(format!("clock error: {}", e)))?;

        let mut block = CalendarBlock {
            block_id,
            written_at_ns,
            ticks,
            merkle_root: [0u8; 32],
        };
        block.merkle_root = block.compute_merkle_root()?;

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
        ticks: Vec<ChrononRecord>,
    }

    let cal: PlaintextCalendar = serde_json::from_str(&contents)?;
    let count = cal.ticks.len();

    if !cal.ticks.is_empty() {
        let externalized: Vec<Externalized<ChrononRecord>> = cal
            .ticks
            .into_iter()
            .map(|r| CleanAuthenticated::<ChrononRecord>::from_trusted(r).externalize())
            .collect();
        store.append_block(externalized)?;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::{clock::FixedClock, crypto_server, foretias::Tbid};

    fn make_server() -> Arc<dyn CryptoServer> {
        let server: Box<dyn CryptoServer> =
            crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519)
                .expect("failed to create software crypto server");
        Arc::from(server)
    }

    fn make_tick(chronon_number: u64) -> Externalized<ChrononRecord> {
        let record = ChrononRecord {
            chronon_number,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
        };
        CleanAuthenticated::<ChrononRecord>::from_trusted(record).externalize()
    }

    #[test]
    fn encrypted_jsonl_append_read() {
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-ejl-test-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        let server = make_server();
        const FIXED_NS: u64 = 1_700_000_000_000_000_000;
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(FIXED_NS));
        let store = EncryptedJsonlCalendarStore::with_clock(path.clone(), server.clone(), clock);

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
            assert_eq!(block.ticks[0].inner().chronon_number, i as u64);
            assert_eq!(block.ticks[1].inner().chronon_number, (i as u64 + 100));
            assert_eq!(block.written_at_ns, FIXED_NS);
        }

        // Cleanup
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn plaintext_migration() {
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-migration-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let plaintext_path = tmp_dir.join("calendar.json");
        let encrypted_path = tmp_dir.join("calendar.jsonl");

        let pk_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(vec![0u8; 32]);
        let nonce_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(vec![0u8; 16]);
        let tbid_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(vec![
            1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        ]);
        let cal_json = serde_json::json!({
            "tbid": tbid_b64,
            "tbn": "legacy-cal",
            "stamp_tbid": tbid_b64,
            "ticks": [
                {
                    "tick_number": 1,
                    "public_key": pk_b64,
                    "forward_foretis": "",
                    "backward_foretis": "",
                    "aa_nonce": nonce_b64,
                    "chronon_stamp_count": 0,
                    "external_attestations": []
                },
                {
                    "tick_number": 2,
                    "public_key": pk_b64,
                    "forward_foretis": "",
                    "backward_foretis": "",
                    "aa_nonce": nonce_b64,
                    "chronon_stamp_count": 0,
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
        assert_eq!(blocks[0].ticks[0].inner().chronon_number, 1);
        assert_eq!(blocks[0].ticks[1].inner().chronon_number, 2);

        // Cleanup
        std::fs::remove_file(&plaintext_path).ok();
        std::fs::remove_file(&encrypted_path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn read_all_empty_file_returns_empty() {
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-empty-test-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        let server = make_server();
        let store = EncryptedJsonlCalendarStore::new(path.clone(), server);

        let blocks = store.read_all().unwrap();
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_externalized_roundtrip() {
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-ext-roundtrip-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        let server = make_server();
        let store = EncryptedJsonlCalendarStore::new(path.clone(), server.clone());

        // Append Externalized<ChrononRecord> ticks
        let ticks = vec![make_tick(42), make_tick(43)];
        store.append_block(ticks).unwrap();

        // Read back
        let blocks = store.read_all().unwrap();
        assert_eq!(blocks.len(), 1);
        let block = &blocks[0];

        // Verify fields preserved through serialization roundtrip
        assert_eq!(block.ticks.len(), 2);
        assert_eq!(block.ticks[0].inner().chronon_number, 42);
        assert_eq!(block.ticks[1].inner().chronon_number, 43);
        assert_eq!(block.ticks[0].inner().signature_algorithm, "Ed25519");
        assert_eq!(block.ticks[0].inner().public_key.as_slice(), &[0u8; 32]);
        assert_eq!(&*block.ticks[0].inner().aa_nonce, &[0u8; 16]);
        assert!(block.ticks[0].inner().forward_foretis.is_empty());
        assert!(block.ticks[0].inner().backward_foretis.is_empty());
        assert!(block.ticks[0].inner().external_attestations.is_empty());

        // Cleanup
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn looks_like_plaintext_json_detects_json() {
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-detect-test-{}", std::process::id()));
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

    #[test]
    fn merkle_root_single_tick() {
        let server = make_server();
        let tmp_dir = std::env::temp_dir().join(format!("foretias-merkle1-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        const FIXED_NS: u64 = 1_700_000_000_000_000_000;
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(FIXED_NS));
        let store = EncryptedJsonlCalendarStore::with_clock(path.clone(), server, clock);

        let ticks = vec![make_tick(1)];
        store.append_block(ticks).unwrap();

        let blocks = store.read_all().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_ne!(blocks[0].merkle_root, [0u8; 32]);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn merkle_root_two_ticks() {
        let server = make_server();
        let tmp_dir = std::env::temp_dir().join(format!("foretias-merkle2-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        const FIXED_NS: u64 = 1_700_000_000_000_000_000;
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(FIXED_NS));
        let store = EncryptedJsonlCalendarStore::with_clock(path.clone(), server, clock);

        let ticks = vec![make_tick(1), make_tick(2)];
        store.append_block(ticks).unwrap();

        let blocks = store.read_all().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_ne!(blocks[0].merkle_root, [0u8; 32]);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn merkle_root_sixty_four_ticks() {
        let server = make_server();
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-merkle64-{}", std::process::id()));
        let path = tmp_dir.join("calendar.jsonl");
        const FIXED_NS: u64 = 1_700_000_000_000_000_000;
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(FIXED_NS));
        let store = EncryptedJsonlCalendarStore::with_clock(path.clone(), server, clock);

        let ticks: Vec<Externalized<ChrononRecord>> = (1..=64u64).map(make_tick).collect();
        store.append_block(ticks).unwrap();

        let blocks = store.read_all().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_ne!(blocks[0].merkle_root, [0u8; 32]);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn merkle_root_deterministic() {
        let server = make_server();
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-merkle-det-{}", std::process::id()));
        let path1 = tmp_dir.join("cal1.jsonl");
        let path2 = tmp_dir.join("cal2.jsonl");
        const FIXED_NS: u64 = 1_700_000_000_000_000_000;
        let clock1: Arc<dyn Clock> = Arc::new(FixedClock::new(FIXED_NS));
        let clock2: Arc<dyn Clock> = Arc::new(FixedClock::new(FIXED_NS));
        let store1 = EncryptedJsonlCalendarStore::with_clock(path1.clone(), server.clone(), clock1);
        let store2 = EncryptedJsonlCalendarStore::with_clock(path2.clone(), server, clock2);

        let ticks = vec![make_tick(1), make_tick(2), make_tick(3)];
        store1.append_block(ticks.clone()).unwrap();
        store2.append_block(ticks).unwrap();

        let blocks1 = store1.read_all().unwrap();
        let blocks2 = store2.read_all().unwrap();
        assert_eq!(blocks1[0].merkle_root, blocks2[0].merkle_root);

        std::fs::remove_file(&path1).ok();
        std::fs::remove_file(&path2).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }

    #[test]
    fn empty_block_defaults_merkle_root_to_zero() {
        let tmp_dir =
            std::env::temp_dir().join(format!("foretias-merkle-empty-{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let path = tmp_dir.join("calendar.jsonl");

        let server = make_server();
        let store = EncryptedJsonlCalendarStore::new(path.clone(), server);

        store.append_block(vec![]).unwrap();

        let blocks = store.read_all().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].merkle_root, [0u8; 32]);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp_dir).ok();
    }
}
