//! Clock Configuration HAL
//!
//! This module provides higher-level abstractions for configuring the EMAC clocks.
//! It wraps the low-level register access from `register::ext` with a more
//! user-friendly API.

use crate::config::{PhyInterface, RmiiClockMode};
use crate::error::{ConfigError, Result};
use crate::internal::register::ext::ExtRegs;

/// Clock configuration state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClockState {
    /// Clock not configured
    #[default]
    Unconfigured,
    /// Clock enabled
    Enabled,
    /// Clock disabled
    Disabled,
}

/// Clock controller for EMAC peripheral
///
/// Provides methods to configure and control the various clocks
/// required for EMAC operation.
#[derive(Debug)]
pub struct ClockController {
    state: ClockState,
    interface: PhyInterface,
}

impl ClockController {
    /// Create a new clock controller
    pub const fn new() -> Self {
        Self {
            state: ClockState::Unconfigured,
            interface: PhyInterface::Rmii,
        }
    }

    /// Configure clocks for the specified PHY interface
    ///
    /// This sets up the appropriate clock sources and routing for either
    /// MII or RMII mode.
    pub fn configure(&mut self, interface: PhyInterface, rmii_clock: RmiiClockMode) -> Result<()> {
        self.interface = interface;

        match interface {
            PhyInterface::Rmii => self.configure_rmii(rmii_clock),
            PhyInterface::Mii => self.configure_mii(),
        }
    }

    /// Configure clocks for RMII mode
    fn configure_rmii(&mut self, clock_mode: RmiiClockMode) -> Result<()> {
        // Set RMII mode (phy_intf_sel = 4)
        ExtRegs::set_rmii_mode();

        match clock_mode {
            RmiiClockMode::ExternalInput { gpio } => {
                // External 50 MHz clock input
                // GPIO0 is the standard input for external RMII clock
                if gpio != 0 {
                    // Only GPIO0 is valid for external clock input on ESP32
                    #[cfg(feature = "esp32")]
                    return Err(ConfigError::InvalidConfig.into());
                }

                // Configure GPIO0 for clock input, then set external clock mode
                ExtRegs::configure_gpio0_rmii_clock_input();
                ExtRegs::set_rmii_clock_external();
            }
            RmiiClockMode::InternalOutput { gpio } => {
                // Internal 50 MHz clock output (requires APLL)
                #[cfg(feature = "esp32")]
                if gpio != 0 && gpio != 16 && gpio != 17 {
                    return Err(ConfigError::InvalidConfig.into());
                }

                ExtRegs::set_rmii_clock_internal();
            }
        }

        Ok(())
    }

    /// Configure clocks for MII mode
    fn configure_mii(&mut self) -> Result<()> {
        // Set MII mode (phy_intf_sel = 0)
        ExtRegs::set_mii_mode();
        Ok(())
    }

    /// Enable all EMAC clocks
    ///
    /// This enables the peripheral clocks required for EMAC operation.
    /// Should be called after configuration and before starting the MAC.
    pub fn enable(&mut self) {
        ExtRegs::enable_clocks();
        ExtRegs::power_up_ram();
        self.state = ClockState::Enabled;
    }

    /// Disable all EMAC clocks
    ///
    /// This disables the peripheral clocks to save power.
    /// Should be called after stopping the MAC.
    pub fn disable(&mut self) {
        ExtRegs::disable_clocks();
        self.state = ClockState::Disabled;
    }

    /// Check if clocks are enabled
    pub fn is_enabled(&self) -> bool {
        self.state == ClockState::Enabled
    }

    /// Get current clock state
    pub fn state(&self) -> ClockState {
        self.state
    }

    /// Set clock inversion
    ///
    /// Some PHYs may require clock inversion to meet timing requirements.
    ///
    /// NOTE: This feature is not currently implemented for ESP32.
    /// The ESP32 EMAC extension registers don't appear to have a clock
    /// inversion bit in the standard register layout.
    #[allow(unused_variables)]
    pub fn set_clock_inversion(&self, invert: bool) {
        // Clock inversion not available in ESP32 EMAC extension registers
        // based on emac_ext_struct.h from ESP-IDF
        #[cfg(feature = "defmt")]
        if invert {
            defmt::warn!("Clock inversion requested but not available on ESP32 EMAC");
        }
    }

    /// Read clock control register (for debugging)
    pub fn read_clock_control(&self) -> u32 {
        ExtRegs::clk_ctrl()
    }

    /// Read PHY interface config register (for debugging)
    pub fn read_phy_interface_config(&self) -> u32 {
        ExtRegs::phy_inf_conf()
    }
}

impl Default for ClockController {
    fn default() -> Self {
        Self::new()
    }
}
