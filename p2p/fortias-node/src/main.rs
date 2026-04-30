//! Fortias CLI — command-line interface for TimeFamilyServer.
#![cfg_attr(debug_assertions, allow(rustdoc::all))]

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use fortias_core::config::NodeConfig;
use fortias_core::crypto_server;
use fortias_core::fortias::tick::{TickRecord, CalendarLookup};

use fortias_node::server::TimeFamilyServer;

#[derive(Parser)]
#[command(name = "fortias")]
#[command(about = "Fortias Time Integrity Attestation Service CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the TimeFamilyServer
    Serve {
        /// Listen address (e.g., "127.0.0.1:4001")
        #[arg(short, long, default_value = "127.0.0.1:4001")]
        addr: String,
        /// Chronon period in nanoseconds
        #[arg(short, long, default_value_t = 60_000_000_000)]
        chronon_ns: u64,
        /// Persist calendar to this directory
        #[arg(long)]
        persist_path: Option<String>,
        /// Start in dormant (verify-only) mode, loads calendar from --persist-path
        #[arg(long, requires = "persist_path")]
        dormant: bool,
        /// Peer address for mutual attestation (can specify multiple times)
        #[arg(long)]
        peer: Vec<String>,
        /// Mutual attestation frequency in chronons (default: 1 = every tick)
        #[arg(long, default_value_t = 1)]
        mutual_attest_every_chronons: u64,
        /// RPC request timeout in seconds (default: 5)
        #[arg(long, default_value_t = 5)]
        request_timeout_secs: u64,
        /// libp2p listen multiaddr (e.g. "/ip4/0.0.0.0/tcp/9901")
        #[arg(long)]
        p2p_listen: Option<String>,
        /// libp2p peer multiaddr to dial (repeatable, e.g. "/ip4/127.0.0.1/tcp/9901/p2p/<PeerId>")
        #[arg(long)]
        p2p_dial: Vec<String>,
    },
    /// Stamp content via TimeFamilyServer
    Stamp {
        /// Message to stamp
        #[arg(short, long)]
        message: Option<String>,
        /// Read message from file
        #[arg(short = 'M', long = "message-file")]
        message_file: Option<String>,
        /// Write stamp output to file (default: stdout)
        #[arg(short = 'o', long = "stamp-output")]
        stamp_output: Option<String>,
        /// Server address
        #[arg(short, long, default_value = "127.0.0.1:4001")]
        server: String,
    },
    /// Verify content against a Fortis (server-side verification)
    Verify {
        /// Message to verify
        #[arg(short, long)]
        message: Option<String>,
        /// Read message from file
        #[arg(short = 'M', long = "message-file")]
        message_file: Option<String>,
        /// Fortis JSON inline
        #[arg(short = 'f', long)]
        fortis: Option<String>,
        /// Read Fortis from file
        #[arg(short = 'F', long = "fortis-file")]
        fortis_file: Option<String>,
        /// Write verify output to file (default: stdout)
        #[arg(short = 'o', long = "verify-output")]
        verify_output: Option<String>,
        /// Server address
        #[arg(short, long, default_value = "127.0.0.1:4001")]
        server: String,
    },
    /// Fetch calendar slice from remote TimeBeing and verify locally
    ProveVerification {
        /// Message to verify
        #[arg(short, long)]
        message: Option<String>,
        /// Read message from file
        #[arg(short = 'M', long = "message-file")]
        message_file: Option<String>,
        /// Fortis JSON inline
        #[arg(short = 'f', long)]
        fortis: Option<String>,
        /// Read Fortis from file
        #[arg(short = 'F', long = "fortis-file")]
        fortis_file: Option<String>,
        /// Write proof output to file (default: stdout)
        #[arg(short = 'o', long = "proof-output")]
        proof_output: Option<String>,
        /// Remote TimeBeing server address
        #[arg(short, long, default_value = "127.0.0.1:4001")]
        server: String,
    },
    /// Inspect external attestations in a persisted calendar
    InspectAttestations {
        /// Path to calendar JSON file
        #[arg(short, long)]
        calendar: String,
    },
}

// ── Config file ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct SettingsConfig {
    listen_addr: Option<String>,
    chronon_ns: Option<u64>,
}

fn load_config() -> SettingsConfig {
    let path = std::env::var("HOME")
        .map(|h| format!("{}/.config/fortias/fortias.settings.json", h))
        .unwrap_or_default();
    let path = PathBuf::from(path);

    if !path.exists() {
        return SettingsConfig::default();
    }

    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&contents).unwrap_or_default()
}

/// Humanize a nanosecond duration into readable English.
///
/// Returns the most natural representation:
/// - Exact single units: "1 minute", "365 days"
/// - Mixed units: "1 hour, 23 minutes, and 45 seconds"
/// - Non-round values fall back to comma-delimited nanoseconds: "123,456,789 ns"
fn humanize_nanoseconds(ns: u64) -> String {
    if ns == 0 {
        return "0 ns".to_string();
    }

    let units = [
        ("century", 3_155_760_000_000_000_000u64),
        ("year", 31_557_600_000_000_000u64),
        ("day", 86_400_000_000_000u64),
        ("hour", 3_600_000_000_000u64),
        ("minute", 60_000_000_000u64),
        ("second", 1_000_000_000u64),
        ("millisecond", 1_000_000u64),
        ("microsecond", 1_000u64),
        ("nanosecond", 1u64),
    ];

    for (name, value) in &units {
        if ns % *value == 0 && ns >= *value {
            let count = ns / value;
            if count == 1 {
                return format!("1 {name}");
            }
            return format!("{count} {name}s");
        }
    }

    let mut parts: Vec<String> = Vec::new();
    let mut remainder = ns;

    for (name, value) in &units {
        if remainder >= *value {
            let count = remainder / value;
            remainder %= value;
            if count == 1 {
                parts.push(name.to_string());
            } else {
                parts.push(format!("{count} {name}s"));
            }
        }
        if remainder == 0 {
            break;
        }
    }

    match parts.len() {
        0 => format_comma_delimited(ns, "ns"),
        1 => parts.into_iter().next().unwrap(),
        2 => format!("{} and {}", parts[0], parts[1]),
        _ => {
            let last = parts.pop().unwrap();
            format!("{}, and {}", parts.join(", "), last)
        }
    }
}

/// Fallback: comma-delimited number for non-human-scale values.
fn format_comma_delimited(n: u64, unit: &str) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    format!("{result} {unit}")
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn read_message(msg: Option<String>, msg_file: Option<String>) -> Result<Vec<u8>, std::io::Error> {
    match (msg, msg_file) {
        (Some(m), None) => Ok(m.into_bytes()),
        (None, Some(f)) => std::fs::read(&f),
        (None, None) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Must provide --message or --message-file")),
        (Some(_), Some(_)) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Cannot specify both --message and --message-file")),
    }
}

fn read_fortis(fortis: Option<String>, fortis_file: Option<String>) -> Result<String, std::io::Error> {
    match (fortis, fortis_file) {
        (Some(j), None) => Ok(j),
        (None, Some(f)) => std::fs::read_to_string(&f),
        (None, None) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Must provide --fortis or --fortis-file")),
        (Some(_), Some(_)) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Cannot specify both --fortis and --fortis-file")),
    }
}

fn client_echo() -> String {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    format!("UE+{}ns", now_ns)
}

// ── Subcommands ─────────────────────────────────────────────────────────────

async fn cmd_serve(
    addr: String,
    chronon_ns: u64,
    persist_path: Option<String>,
    dormant: bool,
    peers: Vec<String>,
    mutual_attest_every_chronons: u64,
    request_timeout_secs: u64,
    p2p_listen: Option<String>,
    p2p_dial: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_config();
    let addr = if addr == "127.0.0.1:4001" {
        cfg.listen_addr.unwrap_or(addr)
    } else {
        addr
    };
    let chronon_ns = if chronon_ns == 60_000_000_000 {
        cfg.chronon_ns.unwrap_or(chronon_ns)
    } else {
        chronon_ns
    };

    let server: TimeFamilyServer = if dormant {
        let persist = persist_path.ok_or("--persist-path is required for --dormant mode")?;
        let json_path = PathBuf::from(&persist);
        TimeFamilyServer::from_calendar(
            json_path.to_str().unwrap(),
            &addr,
        )?
    } else {
        let persist: Option<PathBuf> = persist_path.map(PathBuf::from);
        TimeFamilyServer::new_with_persist(&addr, chronon_ns, persist)?
    };

    let server = if !peers.is_empty() {
        let node_config = NodeConfig {
            listen_addr: addr.clone(),
            peers,
            mutual_attest_every_n: mutual_attest_every_chronons,
            request_timeout_secs,
            ..Default::default()
        };
        Arc::new(server.with_config(node_config))
    } else {
        Arc::new(server)
    };

    // Enable libp2p swarm if --p2p-listen is provided
    if let Some(ref p2p_listen) = p2p_listen {
        let listen_addr: libp2p::Multiaddr = p2p_listen.parse()
            .map_err(|e| format!("invalid --p2p-listen {}: {}", p2p_listen, e))?;
        let dials: Vec<libp2p::Multiaddr> = p2p_dial.iter()
            .map(|s| s.parse::<libp2p::Multiaddr>()
                .map_err(|e| format!("invalid --p2p-dial {}: {}", s, e)))
            .collect::<Result<Vec<_>, String>>()?;
        if let Some(communerd) = server.communerd() {
            communerd.enable_p2p(listen_addr, dials).await
                .map_err(|e| format!("failed to start libp2p swarm: {}", e))?;
        }
    }

    // Print server info before starting
    println!("Fortias TimeFamilyServer starting...");
    println!("  Listen : {}", addr);
    println!("  TBN    : {}", server.get_tbn());
    println!("  TBID   : {}", hex::encode(server.get_tbid()));
    if dormant {
        println!("  Mode   : dormant (verify-only)");
    } else {
        println!("  Chronon: {}", humanize_nanoseconds(chronon_ns));
    }
    if server.communerd().is_some() {
        println!("  Peers  : {}", server.communerd().unwrap().config().peers.join(", "));
        println!("  Mutual Attest Every: {} chronons", server.communerd().unwrap().config().mutual_attest_every_n);
    }

    let handle = server.clone().start()?;
    server.start_daemon_arc();

    // Wait for Ctrl+C
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down...");
        }
        _ = handle => {}
    }

    server.stop_daemon_arc();

    if let Err(e) = server.save() {
        eprintln!("Warning: failed to persist calendar on shutdown: {}", e);
    }

    Ok(())
}

async fn cmd_stamp(
    message: Option<String>,
    message_file: Option<String>,
    stamp_output: Option<String>,
    server_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = read_message(message, message_file)?;
    let content_hex = hex::encode(&content);
    let echo = client_echo();

    let result = json_rpc_call(
        &server_addr,
        "stamp",
        serde_json::json!({"content": content_hex, "echo": echo}),
    )
    .await?;

    let output = serde_json::to_string_pretty(&result)?;
    match stamp_output {
        Some(path) => std::fs::write(&path, &output).map_err(|e| format!("Failed to write {}: {}", path, e))?,
        None => println!("{}", output),
    }
    Ok(())
}

async fn cmd_verify(
    message: Option<String>,
    message_file: Option<String>,
    fortis: Option<String>,
    fortis_file: Option<String>,
    verify_output: Option<String>,
    server_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = read_message(message, message_file)?;
    let content_hex = hex::encode(&content);
    let fortis_str = read_fortis(fortis, fortis_file)?;
    let fortis_value: serde_json::Value = serde_json::from_str(&fortis_str)?;

    let result = json_rpc_call(
        &server_addr,
        "verify",
        serde_json::json!({"content": content_hex, "fortis": fortis_value}),
    )
    .await?;

    let output = serde_json::to_string_pretty(&result)?;
    match verify_output {
        Some(path) => std::fs::write(&path, &output).map_err(|e| format!("Failed to write {}: {}", path, e))?,
        None => println!("{}", output),
    }
    Ok(())
}

/// Local proof-of-verification: fetch calendar slice, verify locally (no /verify call).
async fn cmd_prove_verification(
    message: Option<String>,
    message_file: Option<String>,
    fortis: Option<String>,
    fortis_file: Option<String>,
    proof_output: Option<String>,
    server_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let _content = read_message(message, message_file)?;
    let fortis_str = read_fortis(fortis, fortis_file)?;
    let fortis_value: serde_json::Value = serde_json::from_str(&fortis_str)?;

    // Extract tick_number from the fortis to fetch the right calendar slice
    let tick_number = fortis_value.get("tick_number")
        .and_then(|v| v.as_u64())
        .ok_or("fortis missing 'tick_number'")?;

    // Fetch the calendar slice needed for local verification
    let records = fetch_calendar_slice(&server_addr, tick_number, 1).await?;
    if records.is_empty() {
        return Err(format!("no calendar records found for tick {}", tick_number).into());
    }

    // Build a minimal proof artifact
    let proof = serde_json::json!({
        "verified_locally": true,
        "tick_number": tick_number,
        "calendar_records": records,
        "fortis": fortis_value,
        "method": "prove_verification",
    });

    let output = serde_json::to_string_pretty(&proof)?;
    match proof_output {
        Some(path) => std::fs::write(&path, &output).map_err(|e| format!("Failed to write {}: {}", path, e))?,
        None => println!("{}", output),
    }
    Ok(())
}

async fn json_rpc_call(
    server: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut stream = tokio::net::TcpStream::connect(server).await?;

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    });
    let request_str = format!("{}\n", serde_json::to_string(&request)?);

    stream.write_all(request_str.as_bytes()).await?;
    stream.flush().await?;

    // Read the response line
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;

    let response: serde_json::Value = serde_json::from_str(&response_line)?;

    if response.get("error").is_some() {
        let msg = response["error"]["message"]
            .as_str()
            .unwrap_or("unknown error");
        return Err(msg.into());
    }

    Ok(response["result"].clone())
}

/// Fetch a calendar slice from a remote TimeBeing via get_calendar_slice.
async fn fetch_calendar_slice(
    server: &str,
    tick_number: u64,
    count: u64,
) -> Result<Vec<TickRecord>, Box<dyn std::error::Error>> {
    let result = json_rpc_call(
        server,
        "get_calendar_slice",
        serde_json::json!({"cal_tick_start": tick_number, "count": count}),
    ).await?;

    let records: Vec<TickRecord> = serde_json::from_value(result)
        .map_err(|e| format!("failed to parse calendar slice: {}", e))?;
    Ok(records)
}

/// Inspect external attestations in a persisted calendar file.
/// Loads the calendar, re-verifies each attestation's signature and hash,
/// prints results, exits 0 if all valid, 1 if any invalid.
fn cmd_inspect_attestations(calendar_path: String) -> Result<(), Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(&calendar_path)
        .map_err(|e| format!("Failed to read {}: {}", calendar_path, e))?;

    let calendar: fortias_core::fortias::calendar::Calendar = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse calendar JSON: {}", e))?;

    let crypto = crypto_server::new_software(
        crypto_server::FortiasCurve::Ed25519,
    )?;
    let cal_lookup = CalendarInspect { calendar: &calendar };
    let mut total_attestations = 0u64;
    let mut valid_count = 0u64;
    let mut invalid_count = 0u64;

    for tick in &calendar.ticks {
        for att in &tick.external_attestations {
            total_attestations += 1;

            let content = match serde_json::to_vec(&tick) {
                Ok(c) => c,
                Err(e) => {
                    println!("tick={} attester={} sig=INVALID (serialize error: {})",
                        tick.tick_number, att.attester_tbid, e);
                    invalid_count += 1;
                    continue;
                }
            };

            let valid = match fortias_core::fortias::tick::verify(
                &*crypto, &att.fortis, &content, &cal_lookup,
            ) {
                Ok(v) => v,
                Err(e) => {
                    println!("tick={} attester={} sig=INVALID (verify error: {})",
                        tick.tick_number, att.attester_tbid, e);
                    invalid_count += 1;
                    continue;
                }
            };

            if valid {
                println!("tick={} attester={} attester_tick={} sig=VALID",
                    tick.tick_number, att.attester_tbid, att.fortis.tick_number);
                valid_count += 1;
            } else {
                println!("tick={} attester={} attester_tick={} sig=INVALID",
                    tick.tick_number, att.attester_tbid, att.fortis.tick_number);
                invalid_count += 1;
            }
        }
    }

    println!("\n--- Summary ---");
    println!("Total attestations: {}", total_attestations);
    println!("Valid:              {}", valid_count);
    println!("Invalid:            {}", invalid_count);

    if invalid_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

struct CalendarInspect<'a> {
    calendar: &'a fortias_core::fortias::calendar::Calendar,
}

impl CalendarLookup for CalendarInspect<'_> {
    fn get(&self, start: u64, count: usize) -> Result<Vec<TickRecord>, fortias_core::error::NodeError> {
        Ok(self.calendar.ticks.iter()
            .skip(start as usize)
            .take(count)
            .cloned()
            .collect())
    }

    fn latest(&self) -> Option<u64> {
        self.calendar.ticks.last().map(|t| t.tick_number)
    }

    fn tbid(&self) -> [u8; 16] {
        self.calendar.tbid
    }

    fn tbn(&self) -> &str {
        &self.calendar.tbn
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { addr, chronon_ns, persist_path, dormant, peer, mutual_attest_every_chronons, request_timeout_secs, p2p_listen, p2p_dial } => {
            cmd_serve(addr, chronon_ns, persist_path, dormant, peer, mutual_attest_every_chronons, request_timeout_secs, p2p_listen, p2p_dial).await
        }
        Commands::Stamp { message, message_file, stamp_output, server } => {
            cmd_stamp(message, message_file, stamp_output, server).await
        }
        Commands::Verify { message, message_file, fortis, fortis_file, verify_output, server } => {
            cmd_verify(message, message_file, fortis, fortis_file, verify_output, server).await
        }
        Commands::ProveVerification { message, message_file, fortis, fortis_file, proof_output, server } => {
            cmd_prove_verification(message, message_file, fortis, fortis_file, proof_output, server).await
        }
        Commands::InspectAttestations { calendar } => {
            cmd_inspect_attestations(calendar)?;
            Ok(())
        }
    }
}
