//! Host-side controller for deterministic ESP32 Ethernet QA.
//!
//! The library keeps parsing, validation, scenario selection, and evidence
//! generation independent from serial and packet-capture implementations.
//! Native Windows hardware support is opt-in through the
//! `windows-hardware` feature so ordinary host tests need no Npcap SDK.

pub mod adapters;
pub mod cli;
pub mod config;
pub mod evidence;
pub mod orchestrator;
pub mod protocol;
pub mod session;
pub mod suite;

pub use cli::run_cli;
