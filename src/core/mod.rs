//! Core library — public, framework-agnostic API.
//!
//! Phase 02 added the scanner + GLOBAL_IGNORE.
//! Phase 03 adds folder size calculation.
//! Phase 04 adds risk analysis + safe-delete guard.
//! Subsequent phases add `delete`, `profiles`, `sort`, `filter`.

pub mod delete;
pub mod error;
pub mod filter;
pub mod ignore;
pub mod profiles;
pub mod risk;
pub mod safe_delete;
pub mod scanner;
pub mod size;
pub mod sort;
pub mod types;
