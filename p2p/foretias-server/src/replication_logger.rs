//! Replication logger — per-peer log files with ERROR: prefix for grep-ability.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Log entry for replication events.
#[derive(Debug)]
pub struct ReplicationLogEntry {
    pub level: LogLevel,
    pub event: String,
    pub tbid: Option<String>,
    pub peer: Option<String>,
    pub details: Vec<(String, String)>,
}

#[non_exhaustive]
#[derive(Debug, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// ReplicationLogger — writes to per-peer log files.
pub struct ReplicationLogger {
    base_dir: PathBuf,
}

impl ReplicationLogger {
    #[must_use = "creating a replication logger may fail (e.g. I/O error creating directory); the error must be handled"]
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self, std::io::Error> {
        let base_dir = base_dir.into();
        std::fs::create_dir_all(&base_dir)?;
        let global_path = base_dir.join("replication.log");
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&global_path);
        Ok(Self { base_dir })
    }

    /// Log an event for a specific peer.
    pub fn log_peer(&self, peer_id: &str, entry: &ReplicationLogEntry) {
        let peer_dir = self.base_dir.join(peer_id);
        if std::fs::create_dir_all(&peer_dir).is_err() {
            return;
        }
        let log_path = peer_dir.join("replication.log");
        let line = Self::format_entry(entry);
        let line_clone = line.clone();
        let log_path_clone = log_path.clone();
        std::thread::spawn(move || {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path_clone)
            {
                let _ = writeln!(file, "{}", line_clone);
                let _ = file.flush();
            }
        });
    }

    /// Log an event globally.
    pub fn log_global(&self, entry: &ReplicationLogEntry) {
        let global_path = self.base_dir.join("replication.log");
        let line = Self::format_entry(entry);
        let line_clone = line.clone();
        std::thread::spawn(move || {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(global_path)
            {
                let _ = writeln!(file, "{}", line_clone);
                let _ = file.flush();
            }
        });
    }

    fn format_entry(entry: &ReplicationLogEntry) -> String {
        let ts = chrono::Utc::now().to_rfc3339();
        let level_str = entry.level.to_string();
        let prefix = if entry.level == LogLevel::Error {
            format!("ERROR:{}", level_str)
        } else {
            level_str
        };
        let mut parts = vec![prefix];
        parts.push(format!("ts={}", ts));
        parts.push(format!("event={}", entry.event));
        if let Some(ref tbid) = entry.tbid {
            parts.push(format!("tbid={}", tbid));
        }
        if let Some(ref peer) = entry.peer {
            parts.push(format!("peer={}", peer));
        }
        for (k, v) in &entry.details {
            parts.push(format!("{}={}", k, v));
        }
        parts.join(" ")
    }

    /// Convenience: log an error event to both peer and global logs.
    pub fn error(&self, peer_id: Option<&str>, event: &str, details: Vec<(&str, &str)>) {
        let entry = ReplicationLogEntry {
            level: LogLevel::Error,
            event: event.to_string(),
            tbid: None,
            peer: None,
            details: details
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        if let Some(pid) = peer_id {
            self.log_peer(pid, &entry);
        }
        self.log_global(&entry);
    }
}

impl Clone for ReplicationLogger {
    fn clone(&self) -> Self {
        Self {
            base_dir: self.base_dir.clone(),
        }
    }
}
