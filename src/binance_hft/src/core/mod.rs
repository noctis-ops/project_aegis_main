//! Core types and constants for the HFT system
//!
//! Constants stay under `core::constants` (their single canonical path);
//! re-globbing them here as well would create two import paths for the
//! same items and triggers unused-import warnings in binary targets.

pub mod types;
pub mod constants;
pub mod error;

pub use types::*;
pub use error::*;