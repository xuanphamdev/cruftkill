//! nodemoduleskiller — find and delete `node_modules` (and friends) recursively.
//!
//! Port of [voidcosmos/npkill](https://github.com/voidcosmos/npkill) into idiomatic
//! Rust with an async (tokio) core and a `ratatui` TUI.
//!
//! Public surface lives under [`core`]; the `nmk` binary is in `src/main.rs`.

pub mod cli;
pub mod core;
pub mod tui;

pub use crate::core::error::NpkillError;
pub use crate::core::types::{
    DeleteResult, FolderResult, RiskAnalysis, ScanFoundFolder, ScanOptions, SortBy, SortDirection,
};
