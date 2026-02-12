//! Reset Controller HAL
//!
//! This module provides abstractions for resetting the EMAC peripheral.
//! It handles both the DMA soft reset and the full peripheral reset.

use embedded_hal::delay::DelayNs;

use crate::driver::error::{IoError, Result};
use crate::internal::constants::{RESET_POLL_INTERVAL_US, SOFT_RESET_TIMEOUT_MS};
use crate::internal::register::dma::{DMABUSMODE_SW_RST, DmaRegs};
use crate::internal::register::ext::ExtRegs;

// =============================================================================
// Reset Controller
// =============================================================================

/// Reset controller for the EMAC peripheral
///
/// Provides methods to perform soft resets and check reset status.
#[derive(Debug)]
pub struct ResetController<D: DelayNs> {
    /// Delay provider
    delay: D,
    /// Reset timeout in milliseconds
    timeout_ms: u32,
}

impl<D: DelayNs> ResetController<D> {
    /// Create a new reset controller
    pub fn new(delay: D) -> Self {
        Self {
            delay,
            timeout_ms: SOFT_RESET_TIMEOUT_MS,
        }
    }

    /// Create a new reset controller with custom timeout
    pub fn with_timeout(delay: D, timeout_ms: u32) -> Self {
        Self { delay, timeout_ms }
    }

    /// Perform a DMA soft reset
    ///
    /// This resets the DMA engine and MAC logic to their default states.
    /// All register values are reset and DMA transfers are stopped.
    ///
    /// Returns `Ok(())` on success, or `Err(Error::Timeout)` if the reset
    /// doesn't complete within the configured timeout.
    pub fn soft_reset(&mut self) -> Result<()> {
        // Set the software reset bit
        let bus_mode = DmaRegs::bus_mode();
        DmaRegs::set_bus_mode(bus_mode | DMABUSMODE_SW_RST);

        // Wait for reset to complete (SWR bit clears automatically)
        let max_iterations = (self.timeout_ms * 1000) / RESET_POLL_INTERVAL_US;
        for _ in 0..max_iterations {
            if !self.is_reset_in_progress() {
                return Ok(());
            }
            self.delay.delay_us(RESET_POLL_INTERVAL_US);
        }

        Err(IoError::Timeout.into())
    }

    /// Check if a reset is currently in progress
    pub fn is_reset_in_progress(&self) -> bool {
        (DmaRegs::bus_mode() & DMABUSMODE_SW_RST) != 0
    }

    /// Check if reset is complete (inverse of is_reset_in_progress)
    pub fn is_reset_done(&self) -> bool {
        !self.is_reset_in_progress()
    }

    /// Power up the EMAC RAM
    ///
    /// This must be done before the EMAC can be used.
    pub fn power_up(&self) {
        ExtRegs::power_up_ram();
    }

    /// Power down the EMAC RAM
    ///
    /// This puts the EMAC RAM in a low-power state.
    /// The EMAC cannot be used while powered down.
    pub fn power_down(&self) {
        ExtRegs::power_down_ram();
    }

    /// Get the current timeout setting
    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }
}

// =============================================================================
// Reset State Machine
// =============================================================================

/// Reset state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ResetState {
    /// Not reset (normal operation)
    #[default]
    Normal,
    /// Reset in progress
    Resetting,
    /// Reset complete, waiting for initialization
    ResetComplete,
}

/// Stateful reset manager
///
/// Tracks reset state and provides state machine for reset sequences.
#[derive(Debug)]
pub struct ResetManager<D: DelayNs> {
    controller: ResetController<D>,
    state: ResetState,
}

impl<D: DelayNs> ResetManager<D> {
    /// Create a new reset manager
    pub fn new(delay: D) -> Self {
        Self {
            controller: ResetController::new(delay),
            state: ResetState::Normal,
        }
    }

    /// Get current reset state
    pub fn state(&self) -> ResetState {
        self.state
    }

    /// Start a reset sequence
    pub fn start_reset(&mut self) {
        // Set the software reset bit
        let bus_mode = DmaRegs::bus_mode();
        DmaRegs::set_bus_mode(bus_mode | DMABUSMODE_SW_RST);
        self.state = ResetState::Resetting;
    }

    /// Poll reset status (non-blocking)
    ///
    /// Returns true if reset is complete.
    pub fn poll_reset(&mut self) -> bool {
        if self.state == ResetState::Resetting && self.controller.is_reset_done() {
            self.state = ResetState::ResetComplete;
            true
        } else {
            self.state == ResetState::ResetComplete
        }
    }

    /// Perform blocking reset with timeout
    pub fn reset(&mut self) -> Result<()> {
        self.start_reset();
        let result = self.controller.soft_reset();
        if result.is_ok() {
            self.state = ResetState::ResetComplete;
        } else {
            self.state = ResetState::Normal; // Reset failed, return to normal state
        }
        result
    }

    /// Mark reset sequence as complete
    pub fn complete(&mut self) {
        self.state = ResetState::Normal;
    }

    /// Access the underlying controller
    pub fn controller(&self) -> &ResetController<D> {
        &self.controller
    }

    /// Access the underlying controller mutably
    pub fn controller_mut(&mut self) -> &mut ResetController<D> {
        &mut self.controller
    }
}

// =============================================================================
// Full Reset Sequence
// =============================================================================

/// Perform a full EMAC reset sequence
///
/// This function:
/// 1. Powers up the EMAC RAM
/// 2. Performs a soft reset
/// 3. Waits for reset to complete
/// 4. Returns the EMAC to a known state
pub fn full_reset<D: DelayNs>(mut delay: D, timeout_ms: u32) -> Result<()> {
    // Power up RAM first
    ExtRegs::power_up_ram();

    // Small delay to let RAM power up
    delay.delay_us(10);

    // Create reset controller and perform reset
    let mut reset_ctrl = ResetController::with_timeout(delay, timeout_ms);
    reset_ctrl.soft_reset()
}
