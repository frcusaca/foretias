//! Fortias domain types: TickRecord, Fortis, and stamp/verify operations.

use serde::{Deserialize, Serialize};

use crate::crypto_server::CryptoServer;
use crate::core::bindings::{FortiasPubKey32, FortiasSig64};
use crate::error::NodeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickRecord {
    pub tick_number: u64,
    pub public_key: Vec<u8>,
    pub forward_fortis: Vec<u8>,
    pub backward_fortis: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fortis {
    pub tick_number: u64,
    pub content_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub tbid: [u8; 16],
    pub echo: String,
    pub tbn: String,
}

pub trait CalendarLookup: Send + Sync {
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError>;
    fn latest(&self) -> Option<u64>;
}

/// Stamp content under the current tick's key.
pub fn stamp(
    server: &dyn CryptoServer,
    tbid: &[u8; 16],
    tick_number: u64,
    content: &[u8],
    echo: &str,
    tbn: &str,
) -> Result<Fortis, NodeError> {
    let mut sig_input = Vec::with_capacity(16 + 8 + content.len());
    sig_input.extend_from_slice(tbid);
    sig_input.extend_from_slice(&tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    let signature = server.sign(&sig_input)?;
    let content_hash = server.sha256(content)?;

    Ok(Fortis {
        tick_number,
        content_hash: content_hash.bytes,
        signature: signature.bytes.to_vec(),
        tbid: *tbid,
        echo: echo.to_string(),
        tbn: tbn.to_string(),
    })
}

/// Verify a Fortis against content and calendar.
pub fn verify(
    server: &dyn CryptoServer,
    fortis: &Fortis,
    content: &[u8],
    calendar: &dyn CalendarLookup,
) -> Result<bool, NodeError> {
    let recomputed = server.sha256(content)?;
    if recomputed.bytes != fortis.content_hash {
        return Ok(false);
    }

    let records = calendar.get(fortis.tick_number, 1)?;
    let rec = records.first().ok_or(NodeError::NotFound("tick"))?;

    let mut sig_input = Vec::new();
    sig_input.extend_from_slice(&fortis.tbid);
    sig_input.extend_from_slice(&fortis.tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    let pub_key_bytes: [u8; 32] = rec.public_key[..32].try_into()
        .map_err(|_| NodeError::BadFormat("public_key"))?;
    let pub_key = FortiasPubKey32 { bytes: pub_key_bytes };

    let sig_bytes: [u8; 64] = fortis.signature[..].try_into()
        .map_err(|_| NodeError::BadFormat("signature"))?;
    let sig = FortiasSig64 { bytes: sig_bytes };

    Ok(server.verify_ed25519(&pub_key, &sig_input, &sig)?)
}
