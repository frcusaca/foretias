# Foretias — Configuration Hierarchy Plan

**Version:** v0.5.240
**Status:** Draft — design phase
**Prerequisites:** Phase 2–5 (P2P/CLI/Integration) complete
**Goal:** Organize scattered configuration and constants into a clean, JSON-serializable hierarchy: `TimeFamily → Chronomatter → Calendar`

---

## 1. Problem Statement

Configuration is currently scattered across:

| Location | What it holds | Problem |
|----------|---------------|---------|
| `NodeConfig` (config.rs) | P2P, collision, crypto algo, chronon_ns, peers | Monolithic — mixes P2P networking with domain config |
| `TimeFamilyServer` (server/mod.rs) | chronomatter, calendar, communerd, listen_addr, persist_path | Config passed positionally, no structured TimeFamily config |
| `Chronomatter` (chronomatter/mod.rs) | chronon_ns, TBID, TBN, dormant, keypair rotation | Config set via constructor args, no config struct |
| `Calendar` (calendar.rs) | tbid, tbn, stamp_tbid, ticks | No config — created with raw values |
| `main.rs` | SettingsConfig, CLI flags | CLI→NodeConfig mapping is manual and partial |
| `CollisionConfig` | heartbeat, nonce_window, liege_wait | Nested inside NodeConfig but domain-specific |

**Problems:**
1. No hierarchy — one flat `NodeConfig` for everything
2. Domain configs (Chronomatter, Calendar) have no struct — values passed raw
3. Adding a new TimeFamily member requires touching multiple files
4. No way to serialize a complete TimeFamily config to disk
5. Calendar persists data but not its own configuration

---

## 2. Design Principles

1. **TBID is not configurable** — it's derived from the Chronomatter's cryptographic identity. Calendar stores TBID as a data field, not a config field.
2. **Hierarchy matches runtime structure** — `TimeFamily → Chronomatter → Calendar`, one-to-one for now, designed for N-to-N later.
3. **Each layer serializes independently** — a Calendar can be loaded without knowing about Chronomatter or P2P.
4. **Config is separate from state** — Calendar JSON contains ticks (state) + a `config` section (immutable settings).
5. **JSON is the wire format** — all configs serialize/deserialize via `serde_json`.

---

## 3. Configuration Hierarchy

### 3.1 Top-level: `TimeFamilyConfig`

Represents the complete configuration for a TimeFamily (one Chronomatter, one or more Calendars, and P2P settings).

```json
{
  "version": "0.5.240",
  "chronomatter": { ... },
  "calendars": [ { ... } ],
  "p2p": { ... }
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeFamilyConfig {
    /// Config file version for migration compatibility
    pub version: String,
    /// Chronomatter (ticking engine) configuration
    #[serde(default)]
    pub chronomatter: ChronomatterConfig,
    /// Calendar configurations (one per TimeBeing member)
    #[serde(default = "default_calendars")]
    pub calendars: Vec<CalendarConfig>,
    /// P2P / networking configuration
    #[serde(default)]
    pub p2p: P2PConfig,
}
```

### 3.2 `ChronomatterConfig`

Configuration for the ticking engine. Controls time progression, key rotation, and attestation behavior.

```json
{
  "chronon_ns": 60000000000,
  "tbn": "MainFamily",
  "dormant": false,
  "signature_algorithm": "SPHINCS+-SHA2-128s-simple",
  "kem_algorithm": "Noise-XX",
  "mutual_attest": {
    "every_n_chronons": 1,
    "request_timeout_secs": 5,
    "peers": []
  },
  "key_rotation": {
    "enabled": false,
    "interval_chronons": 1000
  }
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronomatterConfig {
    /// Tick period in nanoseconds (default: 60s)
    #[serde(default = "default_chronon_ns")]
    pub chronon_ns: u64,
    /// TimeBeing Name (human-readable family identifier)
    #[serde(default = "default_tbn")]
    pub tbn: String,
    /// Start in dormant (verify-only) mode
    #[serde(default)]
    pub dormant: bool,
    /// Signature algorithm for stamping
    #[serde(default = "default_signature_algorithm")]
    pub signature_algorithm: SignatureAlgorithm,
    /// KEM algorithm for key exchange
    #[serde(default = "default_kem_algorithm")]
    pub kem_algorithm: KemAlgorithm,
    /// Mutual attestation settings
    #[serde(default)]
    pub mutual_attest: MutualAttestConfig,
    /// Key rotation settings
    #[serde(default)]
    pub key_rotation: KeyRotationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutualAttestConfig {
    /// Attest every N chronons (default: 1)
    #[serde(default = "default_mutual_attest_every_n")]
    pub every_n_chronons: u64,
    /// RPC request timeout in seconds (default: 5)
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    /// Static peer addresses for attestation
    #[serde(default)]
    pub peers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationConfig {
    /// Enable automatic key rotation
    #[serde(default)]
    pub enabled: bool,
    /// Rotate keys every N chronons (default: 1000)
    #[serde(default = "default_key_rotation_interval")]
    pub interval_chronons: u64,
}
```

### 3.3 `CalendarConfig`

Configuration for a single Calendar. The Calendar persists its config alongside its data.

```json
{
  "tbn": "MainFamily",
  "persist_path": ".foretias/calendars",
  "encryption": {
    "enabled": false,
    "algorithm": "none"
  }
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarConfig {
    /// TimeBeing Name — links this calendar to its Chronomatter
    pub tbn: String,
    /// Directory for persisting calendar data
    #[serde(default = "default_calendar_path")]
    pub persist_path: PathBuf,
    /// Encryption settings for persisted data
    #[serde(default)]
    pub encryption: EncryptionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Enable encryption for persisted calendar data
    #[serde(default)]
    pub enabled: bool,
    /// Encryption algorithm (default: "none")
    #[serde(default = "default_encryption_algorithm")]
    pub algorithm: String,
}
```

### 3.4 `P2PConfig`

Extracted from the monolithic `NodeConfig`. Contains all libp2p and networking settings.

```json
{
  "listen_addr": "127.0.0.1:4001",
  "p2p_listen": null,
  "p2p_port_range": [9900, 9999],
  "p2p_dial": [],
  "known_servers": [],
  "max_discovered_peers": 13,
  "dht": {
    "namespace": "mainnet",
    "bootstrap": []
  },
  "collision": {
    "heartbeat_interval_secs": 30,
    "nonce_window": 10,
    "liege_wait_secs": 30
  }
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PConfig {
    /// JSON-RPC listen address (host:port)
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    /// libp2p listen multiaddr (overrides p2p_port_range if set)
    #[serde(default)]
    pub p2p_listen: Option<String>,
    /// Port range for auto-selection
    #[serde(default = "default_p2p_port_range")]
    pub p2p_port_range: [u16; 2],
    /// libp2p peers to dial at startup
    #[serde(default)]
    pub p2p_dial: Vec<String>,
    /// Known servers for self-registration
    #[serde(default)]
    pub known_servers: Vec<String>,
    /// Max peers to auto-discover from DHT
    #[serde(default = "default_max_discovered_peers")]
    pub max_discovered_peers: usize,
    /// DHT configuration
    #[serde(default)]
    pub dht: DHTConfig,
    /// Collision detection configuration
    #[serde(default)]
    pub collision: CollisionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DHTConfig {
    /// DHT namespace for protocol isolation
    #[serde(default = "default_dht_namespace")]
    pub namespace: String,
    /// Bootstrap peer multiaddrs
    #[serde(default)]
    pub bootstrap: Vec<String>,
}
```

### 3.5 Current `NodeConfig` becomes a compatibility wrapper

After migration, `NodeConfig` is a thin alias that maps to `TimeFamilyConfig`:

```rust
// DEPRECATED — use TimeFamilyConfig
pub type NodeConfig = TimeFamilyConfig;
```

Or if backward compatibility requires the old struct:

```rust
impl From<LegacyNodeConfig> for TimeFamilyConfig {
    fn from(legacy: LegacyNodeConfig) -> Self {
        Self {
            version: legacy.version,
            chronomatter: ChronomatterConfig {
                chronon_ns: legacy.chronon_ns,
                tbn: "Default".into(),
                dormant: false,
                signature_algorithm: legacy.signature_algorithm,
                kem_algorithm: legacy.kem_algorithm,
                mutual_attest: MutualAttestConfig {
                    every_n_chronons: legacy.auto_attest_every_n,
                    request_timeout_secs: legacy.request_timeout_secs,
                    peers: legacy.peers,
                },
                key_rotation: KeyRotationConfig::default(),
            },
            calendars: vec![CalendarConfig {
                tbn: "Default".into(),
                persist_path: legacy.calendar_path,
                encryption: EncryptionConfig::default(),
            }],
            p2p: P2PConfig {
                listen_addr: legacy.listen_addr,
                p2p_listen: legacy.p2p_listen,
                p2p_port_range: legacy.p2p_port_range,
                p2p_dial: legacy.p2p_dial,
                known_servers: legacy.known_servers,
                max_discovered_peers: legacy.max_discovered_peers,
                dht: DHTConfig {
                    namespace: legacy.dht_namespace,
                    bootstrap: legacy.dht_bootstrap,
                },
                collision: legacy.collision,
            },
        }
    }
}
```

---

## 4. Calendar On-Disk Format

When Calendar saves to disk, the JSON includes a `config` section as a non-repeating metadata field:

```json
{
  "config": {
    "tbid": "a1b2c3d4e5f6...",
    "tbn": "MainFamily",
    "stamp_tbid": "a1b2c3d4e5f6...",
    "persisted_by": "foretias-v0.5.240",
    "calendar_config": {
      "tbn": "MainFamily",
      "encryption": {
        "enabled": false,
        "algorithm": "none"
      }
    }
  },
  "ticks": [
    { "tick_number": 0, ... },
    { "tick_number": 1, ... }
  ]
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedCalendar {
    /// Non-repeating config metadata
    pub config: CalendarMetadata,
    /// Append-only tick records
    pub ticks: Vec<TickRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarMetadata {
    /// TimeBeing ID (derived, not configurable)
    pub tbid: String,
    /// TimeBeing Name
    pub tbn: String,
    /// Stamp TimeBeing ID
    pub stamp_tbid: String,
    /// Software version that last persisted this calendar
    pub persisted_by: String,
    /// Calendar configuration snapshot
    pub calendar_config: CalendarConfig,
}
```

**Migration path:** The current `Calendar` struct serializes directly. After migration, `Calendar::save()` wraps itself in `PersistedCalendar`. `Calendar::load()` handles both formats:
- If `config` field exists → new format
- If `tbid` field exists at top level → legacy format (migrate on load)

---

## 5. File Locations

| Config struct | File | Description |
|--------------|------|-------------|
| `TimeFamilyConfig` | `core-engine/src/config/time_family.rs` | Top-level config |
| `ChronomatterConfig` | `core-engine/src/config/chronomatter.rs` | Ticking engine config |
| `CalendarConfig` | `core-engine/src/config/calendar.rs` | Calendar persistence config |
| `P2PConfig` | `core-engine/src/config/p2p.rs` | Networking config (extracted from NodeConfig) |
| `DHTConfig` | `core-engine/src/config/p2p.rs` | DHT sub-config |
| `MutualAttestConfig` | `core-engine/src/config/p2p.rs` | Attestation sub-config |
| `KeyRotationConfig` | `core-engine/src/config/chronomatter.rs` | Key rotation sub-config |
| `EncryptionConfig` | `core-engine/src/config/calendar.rs` | Encryption sub-config |
| `CollisionConfig` | `core-engine/src/config/p2p.rs` | Stays in P2P domain |
| `PersistedCalendar` | `core-engine/src/foretias/calendar.rs` | On-disk calendar wrapper |
| `CalendarMetadata` | `core-engine/src/foretias/calendar.rs` | Calendar metadata header |

---

## 6. Default Config File on Disk

```
~/.config/foretias/foretias.json
```

```json
{
  "version": "0.5.240",
  "chronomatter": {
    "chronon_ns": 60000000000,
    "tbn": "Default",
    "dormant": false,
    "signature_algorithm": "SPHINCS+-SHA2-128s-simple",
    "kem_algorithm": "Noise-XX",
                "mutual_attest": {
      "every_n_chronons": 1,
      "request_timeout_secs": 5,
      "peers": []
    },
    "key_rotation": {
      "enabled": false,
      "interval_chronons": 1000
    }
  },
  "calendars": [
    {
      "tbn": "Default",
      "persist_path": ".foretias/calendars",
      "encryption": {
        "enabled": false,
        "algorithm": "none"
      }
    }
  ],
  "p2p": {
    "listen_addr": "127.0.0.1:4001",
    "p2p_port_range": [9900, 9999],
    "known_servers": [],
    "max_discovered_peers": 13,
    "dht": {
      "namespace": "mainnet",
      "bootstrap": []
    },
    "collision": {
      "heartbeat_interval_secs": 30,
      "nonce_window": 10,
      "liege_wait_secs": 30
    }
  }
}
```

---

## 7. CLI Integration

CLI flags map to the hierarchy:

| CLI flag | Config path |
|----------|-------------|
| `--addr` | `p2p.listen_addr` |
| `--p2p-listen` | `p2p.p2p_listen` |
| `--p2p-port-range` | `p2p.p2p_port_range` |
| `--p2p-dial` | `p2p.p2p_dial` |
| `--known-servers` | `p2p.known_servers` |
| `--max-discovered-peers` | `p2p.max_discovered_peers` |
| `--dht-namespace` | `p2p.dht.namespace` |
| `--dht-bootstrap` | `p2p.dht.bootstrap` |
| `--chronon-ns` | `chronomatter.chronon_ns` |
| `--peer` | `communerd.mutual_attest.peers` |
| `--mutually-attest-every-chronons` | `communerd.mutual_attest.every_n_chronons` |
| `--request-timeout-secs` | `communerd.mutual_attest.request_timeout_secs` |
| `--persist-path` | `calendars[0].persist_path` |
| `--dormant` | `chronomatter.dormant` |

---

## 8. Migration Steps

### Step 1: Create new config structs
Create `ChronomatterConfig`, `CalendarConfig`, `P2PConfig`, `DHTConfig`, `MutualAttestConfig`, `KeyRotationConfig`, `EncryptionConfig` in `core-engine/src/config/`.

### Step 2: Create `TimeFamilyConfig`
Create the top-level config struct with the three sub-configs.

### Step 3: Add `From<NodeConfig>` migration impl
Implement `From<LegacyNodeConfig>` for `TimeFamilyConfig` to map old flat fields to new hierarchy.

### Step 4: Update `TimeFamilyServer` constructor
Accept `TimeFamilyConfig` instead of individual parameters. Extract sub-configs internally.

### Step 5: Update `main.rs`
Build `TimeFamilyConfig` from CLI flags + config file. Use `TimeFamilyConfig` to construct `TimeFamilyServer`.

### Step 6: Update Calendar persistence
Wrap `Calendar` in `PersistedCalendar` with `CalendarMetadata` header on save. Handle legacy format on load.

### Step 7: Deprecate `NodeConfig`
Rename current `NodeConfig` to `LegacyNodeConfig`, add deprecation attribute. Keep `From` impl for backward compat.

---

## 9. Out of Scope

- Multi-Chronomatter per TimeFamily (designed for it, not implemented)
- Calendar encryption implementation (config exists, encryption is deferred)
- Config file hot-reload (file is read at startup only)
- Config version migration beyond v0.5.x (migration framework exists, future versions TBD)

---

## 10. References

- `core-engine/src/config.rs` — current monolithic NodeConfig
- `server/mod.rs` — TimeFamilyServer constructor (current parameter passing)
- `foretias/calendar.rs` — Calendar struct and persistence
- `chronomatter/mod.rs` — Chronomatter constructor
- `CLI_SPECIFIED.md` — CLI flag→config mapping reference
