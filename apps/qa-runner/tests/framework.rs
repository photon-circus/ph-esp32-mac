//! Test framework compatibility re-exports.
//!
//! The shared implementation lives in the unpublished QA support library so
//! dedicated firmware suites can use the same state and protocol.

pub use ph_esp32_mac_qa_runner::board::{
    DMA_BASE, DPORT_WIFI_CLK_EN, EMAC, IO_MUX_BASE, MAC_BASE, TestContext,
};
pub use ph_esp32_mac_qa_runner::protocol::TestResult;
