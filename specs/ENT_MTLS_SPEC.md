# ENT_MTLS_SPEC.md

**Spec: Enterprise mTLS for Mirror-Only Peers**
**Status: PROPOSED**
**Date: 2026-06-05**
**Classification: Enterprise Feature**

---

## 1. Overview

Foretias is an **open network** — any time being can join, stamp, and verify. No authentication beyond TBID stamps is required for general participation.

However, **enterprise deployments** may need to restrict mirror sources to a known set of peers. For example, a corporate time being may only accept calendar mirrors from internal peers, not from the open network.

This spec defines an **optional mTLS layer** for mirror-only connections. When enabled, the mirror server requires a valid client certificate before accepting mirror data. This is an enterprise feature — not needed for the open network.

### When to Use

| Scenario | mTLS Needed? |
|----------|-------------|
| Open network — any time being can mirror | ❌ No |
| Corporate internal — only approved mirrors | ✅ Yes |
| Hybrid — open + selective mirrors | ✅ Yes (on mirror endpoint only) |
| Development/testing | ❌ No |

### Current Transport (No TLS)

All transport encryption is via **Noise protocol** (C11 Noise_XX or libp2p-noise). There is **zero TLS/mTLS infrastructure** in the codebase:

| Transport | Encryption | Authentication |
|-----------|-----------|---------------|
| JSON-RPC (primary) | Noise_XX (ChaCha20-Poly1305) | TBID stamps |
| libp2p P2P mesh | libp2p-noise | PeerId |
| HTTP/axum (alternative) | **NONE** | TBID stamps |

---

## 2. Design

### 2.1 Architecture

```
┌─────────────────────────────────────────────┐
│  Mirror Server (enterprise mode)            │
│                                             │
│  ┌─────────┐  ┌──────────────────────────┐  │
│  │ Noise_XX│  │ mTLS (rustls)            │  │
│  │ (open)  │  │ (enterprise mirrors)     │  │
│  └────┬────┘  └────────────┬─────────────┘  │
│       │                    │                │
│       └────────┬───────────┘                │
│                │                            │
│       ┌────────▼────────┐                   │
│       │  JSON-RPC Handler│                   │
│       │  (same handlers) │                   │
│       └─────────────────┘                   │
└─────────────────────────────────────────────┘
```

When mTLS is enabled for a mirror endpoint:
1. Client connects via TCP
2. `rustls` performs TLS handshake with client certificate verification
3. Server extracts client certificate and maps to a TBID
4. If TBID is in the approved mirror list → accept
5. If TBID is not approved → reject with error

### 2.2 Configuration

```rust
/// Enterprise mTLS configuration (optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MtlsConfig {
    /// Enable mTLS for mirror connections.
    pub enabled: bool,

    /// Path to server certificate (PEM).
    pub server_cert: Option<PathBuf>,

    /// Path to server private key (PEM).
    pub server_key: Option<PathBuf>,

    /// Path to CA certificate for verifying client certs (PEM).
    pub client_ca_cert: Option<PathBuf>,

    /// Path to CRL file for certificate revocation (PEM, optional).
    pub crl_path: Option<PathBuf>,

    /// Approved mirror TBIDs (hex-encoded). If empty, all certificate-bearing peers accepted.
    pub approved_mirror_tbids: Vec<String>,

    /// Certificate-to-TBID mapping strategy.
    pub tbid_mapping: TbidMappingStrategy,
}

/// How to map a client certificate to a TBID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TbidMappingStrategy {
    /// Extract TBID from certificate Subject CN (must be hex-encoded TBID).
    SubjectCn,

    /// Extract TBID from certificate SAN (Subject Alternative Name).
    San,

    /// Use a lookup table: certificate serial number → TBID.
    LookupTable(HashMap<String, String>),
}
```

### 2.3 Certificate Generation

For enterprise deployments, the CA is internal (not a public CA). Use `rcgen` to generate:

```rust
fn generate_enterprise_pki(org_name: &str) -> EnterprisePki {
    // 1. Generate CA (self-signed)
    // 2. Generate server cert (signed by CA)
    // 3. Generate client certs (one per approved mirror, signed by CA)
    // 4. Write PEM files
}
```

The CA certificate is distributed to all approved mirrors. Each mirror gets its own client certificate.

### 2.4 Integration with Existing Transport

The mTLS layer is **additive** — it does not replace Noise_XX. When mTLS is enabled:

1. The mirror endpoint listens on a separate port (e.g., `:4002` for mTLS, `:4001` for Noise_XX)
2. Or: the same port accepts both Noise_XX and mTLS connections (detection by first byte)
3. The JSON-RPC handler is the same — the only difference is the transport layer

**Detection strategy:** Noise_XX connections start with a Noise handshake (specific byte pattern). TLS connections start with a TLS ClientHello (different byte pattern). The server can peek at the first byte to determine which transport to use.

### 2.5 Certificate Revocation

Three strategies:

| Strategy | Implementation | When to Use |
|----------|---------------|-------------|
| **CRL (Certificate Revocation Lists)** | `WebPkiClientVerifier::builder().with_crls(...)` | Internal PKI, short-lived CRLs |
| **Short-lived certs** | `rcgen` with 24-hour `not_after` | Replace revocation entirely |
| **Approved TBID list** | `approved_mirror_tbids` in config | Application-level revocation |

---

## 3. Dependencies

```toml
[dependencies]
tokio-rustls = "0.26"
rustls = "0.23"
rustls-pemfile = "2"
rcgen = "0.13"
```

---

## 4. Scope

### In Scope

- `MtlsConfig` struct and deserialization
- Certificate loading and validation
- Client certificate verification with TBID mapping
- CRL support (optional)
- Approved mirror TBID list
- Integration with existing mirror endpoints

### Out of Scope

- Public CA integration (enterprise uses internal CA only)
- OCSP stapling (not needed for internal PKI)
- Certificate rotation automation (manual for now)
- mTLS for non-mirror endpoints (stamp, verify, etc.)

---

## 5. Implementation TODO

- [ ] Write `MtlsConfig` struct with serde support
- [ ] Write `generate_enterprise_pki()` using `rcgen`
- [ ] Implement `TlsAcceptor` integration with `tokio-rustls`
- [ ] Implement TBID mapping from client certificates
- [ ] Implement approved mirror TBID list check
- [ ] Add CRL support (optional)
- [ ] Add integration test: mTLS mirror connection
- [ ] Add integration test: rejected unapproved mirror
- [ ] Document enterprise deployment guide
