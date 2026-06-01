//! Probity module — reputation tracking via signed gossip.

pub mod aggregator;
pub mod clean_auth;
pub mod gossip_handler;
pub mod report;
pub mod store;

pub use aggregator::{aggregate, u_shape_weight, UShapeConfig};
pub use clean_auth::{pub_key_from_tbid_hex, CleanAuthError};
pub use gossip_handler::{handle_gossip_message, DefaultReporterKeyResolver, ReporterKeyResolver};
pub use report::ProbityReport;
pub use store::ProbityStore;
