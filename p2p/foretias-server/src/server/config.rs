//! ForetiasServer configuration.
//!
//! Extends the client-side containment configs (`StandaloneConfig` < `PtpConfig` < `P2pConfig`)
//! with server-specific parameters: listen address, P2P enablement gate, and P2P listener details.
//!
//! When `p2p_enabled: false`, the server operates as a Level 2 node (PtP only, backed by `CommunerdServer`).
//! When `p2p_enabled: true`, the server operates as a Level 3 node (full mesh, backed by `CommunerdP2P`).

use std::path::PathBuf;

use foretias_client::P2pConfig;

/// Configuration for a Foretias server node.
///
/// Contains a `P2pConfig` (which itself contains `PtpConfig` → `StandaloneConfig`)
/// to represent the full capability stack, plus server-specific settings.
pub struct ForetiasServerConfig {
    /// Full P2P config (contains PtP and Standalone configs via containment).
    pub p2p: P2pConfig,
    /// TCP listen address for the JSON-RPC server (e.g. "0.0.0.0:3000").
    pub listen_addr: String,
    /// Gate: when `false`, P2P stack is not initialized — server operates as Level 2 (PtP only).
    pub p2p_enabled: bool,
    /// Optional libp2p listen multiaddr (e.g. "/ip4/0.0.0.0/tcp/0").
    /// When `None` and `p2p_enabled: true`, defaults to "/ip4/0.0.0.0/tcp/0".
    pub p2p_listen: Option<String>,
    /// Port range for P2P listener allocation (reserved for future use).
    pub p2p_port_range: [u16; 2],
    /// Optional path to persist calendar state.
    pub persist_path: Option<PathBuf>,
}

impl ForetiasServerConfig {
    /// Create a minimal server config. P2P disabled by default.
    pub fn new(tbn: impl Into<String>, listen_addr: impl Into<String>) -> Self {
        let tbn = tbn.into();
        Self {
            p2p: P2pConfig::new(&tbn, "mainnet"),
            listen_addr: listen_addr.into(),
            p2p_enabled: false,
            p2p_listen: None,
            p2p_port_range: [0, 0],
            persist_path: None,
        }
    }

    /// Enable or disable the P2P stack.
    pub fn with_p2p_enabled(mut self, enabled: bool) -> Self {
        self.p2p_enabled = enabled;
        self
    }

    /// Set the DHT namespace for peer discovery.
    pub fn with_dht_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.p2p.dht_namespace = namespace.into();
        self
    }

    /// Set known server addresses for bootstrap.
    pub fn with_known_servers(mut self, servers: Vec<String>) -> Self {
        self.p2p.known_servers = servers;
        self
    }

    /// Set peer addresses for PtP connections.
    pub fn with_peers(mut self, peers: Vec<String>) -> Self {
        self.p2p.ptp.peers = peers;
        self
    }

    /// Set the chronon period (nanoseconds).
    pub fn with_chronon(mut self, chronon_ns: u64) -> Self {
        self.p2p.ptp.standalone.chronon_ns = chronon_ns;
        self
    }

    /// Set the persistence path for calendar state.
    pub fn with_persist_path(mut self, path: PathBuf) -> Self {
        self.persist_path = Some(path.clone());
        self.p2p.ptp.standalone.persist_path = Some(path);
        self
    }

    /// Set the libp2p listen multiaddr.
    pub fn with_p2p_listen(mut self, addr: impl Into<String>) -> Self {
        self.p2p_listen = Some(addr.into());
        self
    }

    /// Extract the TBN from the contained config.
    pub fn tbn(&self) -> &str {
        &self.p2p.ptp.standalone.tbn
    }

    /// Extract the chronon period from the contained config.
    pub fn chronon_ns(&self) -> u64 {
        self.p2p.ptp.standalone.chronon_ns
    }

    /// Extract the persist path (from server config, falls back to contained config).
    pub fn persist_path(&self) -> Option<&PathBuf> {
        self.persist_path
            .as_ref()
            .or_else(|| self.p2p.ptp.standalone.persist_path.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_config_defaults() {
        let cfg = ForetiasServerConfig::new("test", "0.0.0.0:3000");
        assert_eq!(cfg.tbn(), "test");
        assert_eq!(cfg.listen_addr, "0.0.0.0:3000");
        assert!(!cfg.p2p_enabled);
        assert!(cfg.p2p_listen.is_none());
        assert!(cfg.persist_path.is_none());
    }

    #[test]
    fn server_config_with_p2p() {
        let cfg = ForetiasServerConfig::new("test", "0.0.0.0:3000")
            .with_p2p_enabled(true)
            .with_dht_namespace("testnet")
            .with_known_servers(vec!["127.0.0.1:4001".into()]);
        assert!(cfg.p2p_enabled);
        assert_eq!(cfg.p2p.dht_namespace, "testnet");
        assert_eq!(cfg.p2p.known_servers.len(), 1);
    }

    #[test]
    fn server_config_containment_chain() {
        let cfg = ForetiasServerConfig::new("deep", "0.0.0.0:3000")
            .with_chronon(1_000_000_000)
            .with_peers(vec!["peer1".into()])
            .with_persist_path(PathBuf::from("/tmp/test"));
        assert_eq!(cfg.p2p.ptp.standalone.tbn, "deep");
        assert_eq!(cfg.p2p.ptp.standalone.chronon_ns, 1_000_000_000);
        assert_eq!(cfg.p2p.ptp.peers.len(), 1);
        assert!(cfg.persist_path.is_some());
    }

    #[test]
    fn server_config_dispatch_methods() {
        let cfg = ForetiasServerConfig::new("dispatch", "0.0.0.0:3000")
            .with_chronon(5_000_000_000);
        assert_eq!(cfg.chronon_ns(), 5_000_000_000);
        assert_eq!(cfg.tbn(), "dispatch");
    }
}
