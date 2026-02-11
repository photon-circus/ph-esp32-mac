//! IEEE 802.3 flow control for the ESP32 EMAC.
//!
//! This module extends [`Emac`] with PAUSE frame-based flow control,
//! allowing the MAC to signal the link partner to temporarily stop
//! transmitting when receive buffers are filling up.
//!
//! # Overview
//!
//! Flow control uses IEEE 802.3x PAUSE frames to prevent buffer overflows:
//!
//! 1. When RX descriptors drop below `low_water_mark`, send PAUSE frame
//! 2. Link partner stops transmitting for `pause_time` quanta
//! 3. When RX descriptors rise above `high_water_mark`, resume normal operation
//!
//! # Example
//!
//! ```ignore
//! // After PHY auto-negotiation completes
//! if link_partner_supports_pause {
//!     emac.set_peer_pause_ability(true);
//!     emac.enable_flow_control(true);
//! }
//!
//! // In your RX processing loop
//! emac.check_flow_control();
//! ```
//!
//! # Testing Notes
//!
//! Flow control is an advanced feature and has limited hardware validation so far.
//! Treat it as best-effort until broader testing confirms behavior.

use super::config::FlowControlConfig;
use super::emac::Emac;
use crate::internal::register::mac::MacRegs;

// =============================================================================
// Flow Control Implementation
// =============================================================================

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    fn apply_flow_control(&mut self, enable: bool) {
        if enable {
            let fc = &self.config.flow_control;
            MacRegs::configure_flow_control(
                fc.pause_time,
                fc.pause_low_threshold as u8,
                fc.unicast_pause_detect,
                true, // TX flow control
                true, // RX flow control
            );
        } else {
            MacRegs::configure_flow_control(0, 0, false, false, false);
            self.flow_control_active = false;
        }
    }

    /// Enable or disable flow control
    ///
    /// This configures the MAC for IEEE 802.3 PAUSE frame-based flow control.
    /// Must be called after init() and before or after start().
    ///
    /// Flow control is only actually enabled if:
    /// 1. The `enable` parameter is `true`
    /// 2. The peer has advertised PAUSE capability (via [`set_peer_pause_ability`])
    ///
    /// [`set_peer_pause_ability`]: Self::set_peer_pause_ability
    pub fn enable_flow_control(&mut self, enable: bool) {
        self.config.flow_control.enabled = enable;
        self.apply_flow_control(enable && self.peer_pause_ability);
    }

    /// Set peer PAUSE frame ability
    ///
    /// This should be called after PHY auto-negotiation completes to indicate
    /// whether the link partner supports PAUSE frames. Flow control will only
    /// be enabled if both the user configuration requests it AND the peer
    /// supports it.
    ///
    /// # Arguments
    /// * `ability` - true if peer advertised PAUSE capability
    ///
    /// # Example
    /// ```ignore
    /// // After reading link partner abilities from PHY ANLPAR register
    /// let supports_pause = (anlpar & ANLPAR_PAUSE) != 0;
    /// emac.set_peer_pause_ability(supports_pause);
    /// ```
    pub fn set_peer_pause_ability(&mut self, ability: bool) {
        self.peer_pause_ability = ability;

        // Re-configure flow control based on new peer ability
        self.apply_flow_control(self.config.flow_control.enabled && ability);
    }

    /// Check if flow control action is needed and send PAUSE frame if necessary
    ///
    /// This implements software flow control logic based on RX descriptor
    /// availability. Call this periodically (e.g., from RX interrupt handler)
    /// to manage PAUSE frame transmission.
    ///
    /// # Returns
    /// `true` if flow control state changed (PAUSE sent or resumed)
    ///
    /// # Example
    /// ```ignore
    /// // In your interrupt handler or main loop
    /// if emac.check_flow_control() {
    ///     log::debug!("Flow control state changed");
    /// }
    /// ```
    pub fn check_flow_control(&mut self) -> bool {
        // Only do flow control if enabled and peer supports it
        if !self.config.flow_control.enabled || !self.peer_pause_ability {
            return false;
        }

        let fc = &self.config.flow_control;
        let free_descriptors = self.dma.rx_free_count();
        let frames_remain = self.rx_frames_waiting() > 0;

        // Check if we need to activate flow control (send PAUSE)
        if !self.flow_control_active && free_descriptors < fc.low_water_mark && frames_remain {
            MacRegs::send_pause_frame(true);
            self.flow_control_active = true;
            return true;
        }

        // Check if we can deactivate flow control (resume)
        if self.flow_control_active && (free_descriptors > fc.high_water_mark || !frames_remain) {
            MacRegs::send_pause_frame(false);
            self.flow_control_active = false;
            return true;
        }

        false
    }

    /// Get current flow control state
    ///
    /// Returns `true` if PAUSE has been sent and we're waiting for buffers
    /// to free up.
    #[inline(always)]
    pub fn is_flow_control_active(&self) -> bool {
        self.flow_control_active
    }

    /// Get flow control configuration
    #[inline(always)]
    pub fn flow_control_config(&self) -> &FlowControlConfig {
        &self.config.flow_control
    }

    /// Get peer PAUSE ability
    ///
    /// Returns `true` if the link partner supports PAUSE frames.
    #[inline(always)]
    pub fn peer_pause_ability(&self) -> bool {
        self.peer_pause_ability
    }
}
