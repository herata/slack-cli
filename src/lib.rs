//! Internal library facade for `slack-cli`.
//!
//! Exposed only so integration tests under `tests/` can reach
//! the same modules the binary uses. Not a stable public API.

#![warn(missing_debug_implementations, rust_2018_idioms, unreachable_pub)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc
)]

pub mod cli;
pub mod cmd;
pub mod config;
pub mod error;
pub mod id;
pub mod logging;
pub mod output;
pub mod resolve;
pub mod secret;
pub mod slack;
