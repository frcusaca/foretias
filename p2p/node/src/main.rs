//! Fortias CLI — command-line interface for TimeFamilyServer.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use fortias_p2p::server::TimeFamilyServer;

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
    },
    /// Stamp content via TimeFamilyServer
    Stamp {
        /// Content to stamp (raw string)
        content: String,
        /// Server address
        #[arg(short, long, default_value = "127.0.0.1:4001")]
        server: String,
    },
    /// Verify content against a Fortis
    Verify {
        /// Content to verify (raw string)
        content: String,
        /// Fortis JSON (from stamp response)
        fortis_json: String,
        /// Server address
        #[arg(short, long, default_value = "127.0.0.1:4001")]
        server: String,
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

// ── Subcommands ─────────────────────────────────────────────────────────────

async fn cmd_serve(addr: String, chronon_ns: u64) -> Result<(), Box<dyn std::error::Error>> {
    // Load config file; CLI args override config values
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

    let server = TimeFamilyServer::new(&addr, chronon_ns)?;
    let server = Arc::new(server);

    // Print server info before starting
    println!("Fortias TimeFamilyServer starting...");
    println!("  Listen : {}", addr);
    println!("  TBN    : {}", server.get_tbn());
    println!("  TBID   : {}", hex::encode(server.get_tbid()));
    println!("  Chronon: {}", humanize_nanoseconds(chronon_ns));

    let handle = server.start()?;

    // Wait for Ctrl+C
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down...");
        }
        _ = handle => {}
    }

    Ok(())
}

async fn cmd_stamp(content: String, server_addr: String) -> Result<(), Box<dyn std::error::Error>> {
    let content_hex = hex::encode(content.as_bytes());
    let result = json_rpc_call(
        &server_addr,
        "stamp",
        serde_json::json!({"content": content_hex}),
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn cmd_verify(
    content: String,
    fortis_json: String,
    server_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let content_hex = hex::encode(content.as_bytes());
    let fortis_value: serde_json::Value = serde_json::from_str(&fortis_json)?;

    let result = json_rpc_call(
        &server_addr,
        "verify",
        serde_json::json!({"content": content_hex, "fortis": fortis_value}),
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

// ── JSON-RPC client ─────────────────────────────────────────────────────────

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

// ── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { addr, chronon_ns } => cmd_serve(addr, chronon_ns).await,
        Commands::Stamp { content, server } => cmd_stamp(content, server).await,
        Commands::Verify { content, fortis_json, server } => {
            cmd_verify(content, fortis_json, server).await
        }
    }
}
