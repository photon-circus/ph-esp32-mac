//! Shared support for the unpublished ESP32 hardware QA firmware.
//!
//! This crate is intentionally allocation-free. It provides the serial protocol,
//! board state, interrupt observations, and register snapshots shared by the
//! exploratory smoke runner and the dedicated release suites.

#![no_std]
#![deny(missing_docs)]

pub mod board;
pub mod frames;
pub mod interrupts;
pub mod protocol;
pub mod rx_harness;
pub mod snapshots;

pub use board::{EMAC, TestContext, enable_wt32_oscillator, reset_reason_token};
pub use protocol::{Reporter, RunMode, RunResult, TestResult, TestStats};

/// Run identifier embedded by the host controller.
///
/// Local builds use `0`; release-grade orchestration replaces this with one to
/// sixteen lowercase hexadecimal digits through the `PHQA_RUN_ID` compile-time
/// environment variable.
pub const BUILD_RUN_ID: &str = match option_env!("PHQA_RUN_ID") {
    Some(value) => value,
    None => "0",
};

/// Git commit embedded by the host controller.
///
/// Local builds use a valid all-zero commit; release-grade orchestration
/// replaces this through the `PHQA_GIT_SHA` compile-time environment variable.
pub const BUILD_GIT_COMMIT: &str = match option_env!("PHQA_GIT_SHA") {
    Some(value) => value,
    None => "0000000000000000000000000000000000000000",
};

/// Emit a failing release-suite scaffold and enter a terminal idle state.
///
/// Dedicated suites use this until their hardware scenario is implemented.
/// Failing closed prevents an empty firmware image from being accepted as
/// release evidence.
pub fn run_blocked_suite(suite: &'static str, test_id: &'static str) -> ! {
    let mut reporter = Reporter::new(
        BUILD_RUN_ID,
        suite,
        BUILD_GIT_COMMIT,
        RunMode::Release,
        reset_reason_token(),
    );
    reporter.start();
    reporter.record_with_reason(test_id, TestResult::Blocked, true, "not-implemented");
    let _ = reporter.finish();
    terminal_idle()
}

/// Disable the EMAC interrupt and wait indefinitely after a terminal record.
///
/// Unlike `qa-smoke`, release suites perform no monitoring after `RUN_END`.
pub fn terminal_idle() -> ! {
    esp_hal::interrupt::disable(
        esp_hal::system::Cpu::current(),
        esp_hal::peripherals::Interrupt::ETH_MAC,
    );

    loop {
        esp_hal::interrupt::wait_for_interrupt();
    }
}
