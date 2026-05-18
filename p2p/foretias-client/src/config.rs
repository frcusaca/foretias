//! Containment-style configuration for Foretias client instantiation.
//!
//! Each level contains its parent:
//! - `StandaloneConfig` — Level 1 (in-memory, no network)
//! - `PtpConfig` — Level 2 (outbound PtP only)
//! - `P2pConfig` — Level 3 (P2P mesh participation)
//!
//! `ForetiasConfig` enum dispatches to the appropriate instantiation path.

use std::path::PathBuf;

/// Level 1 — Standalone (in-memory, no network).
pub struct StandaloneConfig {
    pub tbn: String,
    pub chronon_ns: u64,
    pub persist_path: Option<PathBuf>,
}

impl StandaloneConfig {
    pub fn new(tbn: impl Into<String>) -> Self {
        Self {
            tbn: tbn.into(),
            chronon_ns: 60_000_000_000,
            persist_path: None,
        }
    }

    pub fn with_chronon(mut self, chronon_ns: u64) -> Self {
        self.chronon_ns = chronon_ns;
        self
    }

    pub fn with_persist_path(mut self, path: PathBuf) -> Self {
        self.persist_path = Some(path);
        self
    }
}

/// Level 2 — PtP Networked (outbound connections only).
/// Contains StandaloneConfig + peer connectivity.
pub struct PtpConfig {
    /// Level 1 capabilities (identity, crypto, calendar).
    pub standalone: StandaloneConfig,
    /// Known peer addresses to connect to.
    pub peers: Vec<String>,
    /// Default request timeout (seconds).
    pub timeout_secs: u64,
}

impl PtpConfig {
    pub fn new(tbn: impl Into<String>, peers: Vec<String>) -> Self {
        Self {
            standalone: StandaloneConfig::new(tbn),
            peers,
            timeout_secs: 30,
        }
    }

    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    pub fn with_chronon(mut self, chronon_ns: u64) -> Self {
        self.standalone.chronon_ns = chronon_ns;
        self
    }

    pub fn with_persist_path(mut self, path: PathBuf) -> Self {
        self.standalone.persist_path = Some(path);
        self
    }

    pub fn primary_peer(&self) -> Option<&str> {
        self.peers.first().map(|s| s.as_str())
    }
}

/// Level 3 — P2P Full (client-side mesh participation).
/// Contains PtpConfig + DHT namespace + known servers.
pub struct P2pConfig {
    /// Level 2 capabilities (identity, crypto, calendar, PtP).
    pub ptp: PtpConfig,
    /// DHT namespace for peer discovery.
    pub dht_namespace: String,
    /// Known server addresses for self-registration.
    pub known_servers: Vec<String>,
    /// Maximum peers to auto-discover.
    pub max_discovered_peers: usize,
}

impl P2pConfig {
    pub fn new(tbn: impl Into<String>, dht_namespace: impl Into<String>) -> Self {
        Self {
            ptp: PtpConfig::new(tbn, vec![]),
            dht_namespace: dht_namespace.into(),
            known_servers: vec![],
            max_discovered_peers: 13,
        }
    }

    pub fn with_peers(mut self, peers: Vec<String>) -> Self {
        self.ptp.peers = peers;
        self
    }

    pub fn with_known_servers(mut self, servers: Vec<String>) -> Self {
        self.known_servers = servers;
        self
    }

    pub fn with_max_discovered_peers(mut self, max: usize) -> Self {
        self.max_discovered_peers = max;
        self
    }

    pub fn with_chronon(mut self, chronon_ns: u64) -> Self {
        self.ptp.standalone.chronon_ns = chronon_ns;
        self
    }

    pub fn with_persist_path(mut self, path: PathBuf) -> Self {
        self.ptp.standalone.persist_path = Some(path);
        self
    }
}

/// Dispatch surface — select which level to instantiate.
pub enum ForetiasConfig {
    Standalone(StandaloneConfig),
    Ptp(PtpConfig),
    P2p(P2pConfig),
}

impl ForetiasConfig {
    /// Extract the TBN (TimeBeing Name) from any config variant.
    pub fn tbn(&self) -> &str {
        match self {
            ForetiasConfig::Standalone(cfg) => &cfg.tbn,
            ForetiasConfig::Ptp(cfg) => &cfg.standalone.tbn,
            ForetiasConfig::P2p(cfg) => &cfg.ptp.standalone.tbn,
        }
    }

    /// Extract the chronon period from any config variant.
    pub fn chronon_ns(&self) -> u64 {
        match self {
            ForetiasConfig::Standalone(cfg) => cfg.chronon_ns,
            ForetiasConfig::Ptp(cfg) => cfg.standalone.chronon_ns,
            ForetiasConfig::P2p(cfg) => cfg.ptp.standalone.chronon_ns,
        }
    }

    /// Extract the persist path from any config variant.
    pub fn persist_path(&self) -> Option<&PathBuf> {
        match self {
            ForetiasConfig::Standalone(cfg) => cfg.persist_path.as_ref(),
            ForetiasConfig::Ptp(cfg) => cfg.standalone.persist_path.as_ref(),
            ForetiasConfig::P2p(cfg) => cfg.ptp.standalone.persist_path.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_config_new() {
        let cfg = StandaloneConfig::new("test-tbn");
        assert_eq!(cfg.tbn, "test-tbn");
        assert_eq!(cfg.chronon_ns, 60_000_000_000);
        assert!(cfg.persist_path.is_none());
    }

    #[test]
    fn standalone_config_with_chronon() {
        let cfg = StandaloneConfig::new("test").with_chronon(1_000_000_000);
        assert_eq!(cfg.chronon_ns, 1_000_000_000);
    }

    #[test]
    fn standalone_config_with_persist_path() {
        let path = PathBuf::from("/tmp/test.json");
        let cfg = StandaloneConfig::new("test").with_persist_path(path.clone());
        assert_eq!(cfg.persist_path, Some(path));
    }

    #[test]
    fn ptp_config_contains_standalone() {
        let cfg = PtpConfig::new("test", vec!["127.0.0.1:4001".into()]);
        assert_eq!(cfg.standalone.tbn, "test");
        assert_eq!(cfg.peers.len(), 1);
        assert_eq!(cfg.timeout_secs, 30);
    }

    #[test]
    fn ptp_config_primary_peer() {
        let cfg = PtpConfig::new("test", vec!["peer1".into(), "peer2".into()]);
        assert_eq!(cfg.primary_peer(), Some("peer1"));
    }

    #[test]
    fn ptp_config_no_peers() {
        let cfg = PtpConfig::new("test", vec![]);
        assert!(cfg.primary_peer().is_none());
    }

    #[test]
    fn p2p_config_contains_ptp() {
        let cfg = P2pConfig::new("test", "mainnet");
        assert_eq!(cfg.ptp.standalone.tbn, "test");
        assert_eq!(cfg.dht_namespace, "mainnet");
        assert_eq!(cfg.max_discovered_peers, 13);
    }

    #[test]
    fn p2p_config_with_options() {
        let cfg = P2pConfig::new("test", "mainnet")
            .with_peers(vec!["peer1".into()])
            .with_known_servers(vec!["server1".into()])
            .with_max_discovered_peers(20);
        assert_eq!(cfg.ptp.peers.len(), 1);
        assert_eq!(cfg.known_servers.len(), 1);
        assert_eq!(cfg.max_discovered_peers, 20);
    }

    #[test]
    fn foretias_config_tbn_dispatch() {
        let s = ForetiasConfig::Standalone(StandaloneConfig::new("s"));
        let p = ForetiasConfig::Ptp(PtpConfig::new("p", vec![]));
        let d = ForetiasConfig::P2p(P2pConfig::new("d", "ns"));
        assert_eq!(s.tbn(), "s");
        assert_eq!(p.tbn(), "p");
        assert_eq!(d.tbn(), "d");
    }

    #[test]
    fn foretias_config_chronon_dispatch() {
        let s = ForetiasConfig::Standalone(StandaloneConfig::new("s").with_chronon(100));
        let p = ForetiasConfig::Ptp(PtpConfig::new("p", vec![]).with_chronon(200));
        let d = ForetiasConfig::P2p(P2pConfig::new("d", "ns").with_chronon(300));
        assert_eq!(s.chronon_ns(), 100);
        assert_eq!(p.chronon_ns(), 200);
        assert_eq!(d.chronon_ns(), 300);
    }
}
