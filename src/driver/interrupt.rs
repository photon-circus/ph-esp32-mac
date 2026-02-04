//! Interrupt status handling for the ESP32 EMAC.
//!
//! This module provides the [`InterruptStatus`] structure for parsing
//! and managing DMA interrupt flags.

use crate::internal::register::dma::{
    DMASTATUS_AIS, DMASTATUS_FBI, DMASTATUS_NIS, DMASTATUS_OVF, DMASTATUS_RI, DMASTATUS_RPS,
    DMASTATUS_RU, DMASTATUS_TI, DMASTATUS_TPS, DMASTATUS_TU, DMASTATUS_UNF,
};

// =============================================================================
// Interrupt Status
// =============================================================================

/// Interrupt status flags parsed from the DMA status register.
///
/// This structure provides a convenient way to check which interrupts
/// have occurred without manually parsing the raw register bits.
///
/// # Example
///
/// ```ignore
/// let status = emac.interrupt_status();
/// if status.rx_complete {
///     // Handle received frame
/// }
/// if status.has_error() {
///     // Handle error condition
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct InterruptStatus {
    /// TX complete - frame transmitted successfully
    pub tx_complete: bool,
    /// TX stopped - TX DMA stopped
    pub tx_stopped: bool,
    /// TX buffer unavailable - no TX descriptors available
    pub tx_buf_unavailable: bool,
    /// TX underflow - TX FIFO underflow
    pub tx_underflow: bool,
    /// RX complete - frame received
    pub rx_complete: bool,
    /// RX stopped - RX DMA stopped
    pub rx_stopped: bool,
    /// RX buffer unavailable - no RX descriptors available
    pub rx_buf_unavailable: bool,
    /// RX overflow - RX FIFO overflow
    pub rx_overflow: bool,
    /// Fatal bus error - unrecoverable DMA error
    pub fatal_bus_error: bool,
    /// Normal interrupt summary
    pub normal_summary: bool,
    /// Abnormal interrupt summary
    pub abnormal_summary: bool,
}

impl InterruptStatus {
    /// Create from raw DMA status register value
    #[inline]
    pub fn from_raw(status: u32) -> Self {
        Self {
            tx_complete: (status & DMASTATUS_TI) != 0,
            tx_stopped: (status & DMASTATUS_TPS) != 0,
            tx_buf_unavailable: (status & DMASTATUS_TU) != 0,
            tx_underflow: (status & DMASTATUS_UNF) != 0,
            rx_complete: (status & DMASTATUS_RI) != 0,
            rx_stopped: (status & DMASTATUS_RPS) != 0,
            rx_buf_unavailable: (status & DMASTATUS_RU) != 0,
            rx_overflow: (status & DMASTATUS_OVF) != 0,
            fatal_bus_error: (status & DMASTATUS_FBI) != 0,
            normal_summary: (status & DMASTATUS_NIS) != 0,
            abnormal_summary: (status & DMASTATUS_AIS) != 0,
        }
    }

    /// Convert to raw value for clearing (write-1-to-clear)
    #[inline]
    pub fn to_raw(&self) -> u32 {
        let mut val = 0u32;
        if self.tx_complete {
            val |= DMASTATUS_TI;
        }
        if self.tx_stopped {
            val |= DMASTATUS_TPS;
        }
        if self.tx_buf_unavailable {
            val |= DMASTATUS_TU;
        }
        if self.tx_underflow {
            val |= DMASTATUS_UNF;
        }
        if self.rx_complete {
            val |= DMASTATUS_RI;
        }
        if self.rx_stopped {
            val |= DMASTATUS_RPS;
        }
        if self.rx_buf_unavailable {
            val |= DMASTATUS_RU;
        }
        if self.rx_overflow {
            val |= DMASTATUS_OVF;
        }
        if self.fatal_bus_error {
            val |= DMASTATUS_FBI;
        }
        if self.normal_summary {
            val |= DMASTATUS_NIS;
        }
        if self.abnormal_summary {
            val |= DMASTATUS_AIS;
        }
        val
    }

    /// Check if any interrupt occurred (excluding summary bits)
    #[inline]
    pub fn any(&self) -> bool {
        self.tx_complete
            || self.tx_stopped
            || self.tx_buf_unavailable
            || self.tx_underflow
            || self.rx_complete
            || self.rx_stopped
            || self.rx_buf_unavailable
            || self.rx_overflow
            || self.fatal_bus_error
    }

    /// Check if any error occurred
    #[inline]
    pub fn has_error(&self) -> bool {
        self.tx_underflow || self.rx_overflow || self.fatal_bus_error
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupt_status_from_raw_zero() {
        let status = InterruptStatus::from_raw(0);

        assert!(!status.tx_complete);
        assert!(!status.tx_stopped);
        assert!(!status.tx_buf_unavailable);
        assert!(!status.tx_underflow);
        assert!(!status.rx_complete);
        assert!(!status.rx_stopped);
        assert!(!status.rx_buf_unavailable);
        assert!(!status.rx_overflow);
        assert!(!status.fatal_bus_error);
        assert!(!status.normal_summary);
        assert!(!status.abnormal_summary);
    }

    #[test]
    fn interrupt_status_from_raw_tx_complete() {
        let status = InterruptStatus::from_raw(DMASTATUS_TI);

        assert!(status.tx_complete);
        assert!(!status.rx_complete);
        assert!(!status.fatal_bus_error);
    }

    #[test]
    fn interrupt_status_from_raw_rx_complete() {
        let status = InterruptStatus::from_raw(DMASTATUS_RI);

        assert!(status.rx_complete);
        assert!(!status.tx_complete);
        assert!(!status.fatal_bus_error);
    }

    #[test]
    fn interrupt_status_from_raw_tx_stopped() {
        let status = InterruptStatus::from_raw(DMASTATUS_TPS);

        assert!(status.tx_stopped);
        assert!(!status.tx_complete);
    }

    #[test]
    fn interrupt_status_from_raw_rx_stopped() {
        let status = InterruptStatus::from_raw(DMASTATUS_RPS);

        assert!(status.rx_stopped);
        assert!(!status.rx_complete);
    }

    #[test]
    fn interrupt_status_from_raw_tx_buf_unavailable() {
        let status = InterruptStatus::from_raw(DMASTATUS_TU);

        assert!(status.tx_buf_unavailable);
    }

    #[test]
    fn interrupt_status_from_raw_rx_buf_unavailable() {
        let status = InterruptStatus::from_raw(DMASTATUS_RU);

        assert!(status.rx_buf_unavailable);
    }

    #[test]
    fn interrupt_status_from_raw_tx_underflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_UNF);

        assert!(status.tx_underflow);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_from_raw_rx_overflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_OVF);

        assert!(status.rx_overflow);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_from_raw_fatal_bus_error() {
        let status = InterruptStatus::from_raw(DMASTATUS_FBI);

        assert!(status.fatal_bus_error);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_from_raw_normal_summary() {
        let status = InterruptStatus::from_raw(DMASTATUS_NIS);

        assert!(status.normal_summary);
        assert!(!status.abnormal_summary);
    }

    #[test]
    fn interrupt_status_from_raw_abnormal_summary() {
        let status = InterruptStatus::from_raw(DMASTATUS_AIS);

        assert!(status.abnormal_summary);
        assert!(!status.normal_summary);
    }

    #[test]
    fn interrupt_status_from_raw_all_bits() {
        let all_bits = DMASTATUS_TI
            | DMASTATUS_TPS
            | DMASTATUS_TU
            | DMASTATUS_UNF
            | DMASTATUS_RI
            | DMASTATUS_RPS
            | DMASTATUS_RU
            | DMASTATUS_OVF
            | DMASTATUS_FBI
            | DMASTATUS_NIS
            | DMASTATUS_AIS;

        let status = InterruptStatus::from_raw(all_bits);

        assert!(status.tx_complete);
        assert!(status.tx_stopped);
        assert!(status.tx_buf_unavailable);
        assert!(status.tx_underflow);
        assert!(status.rx_complete);
        assert!(status.rx_stopped);
        assert!(status.rx_buf_unavailable);
        assert!(status.rx_overflow);
        assert!(status.fatal_bus_error);
        assert!(status.normal_summary);
        assert!(status.abnormal_summary);
    }

    #[test]
    fn interrupt_status_to_raw_roundtrip() {
        let original = DMASTATUS_TI | DMASTATUS_RI | DMASTATUS_NIS;
        let status = InterruptStatus::from_raw(original);
        let roundtrip = status.to_raw();

        assert_eq!(roundtrip, original);
    }

    #[test]
    fn interrupt_status_to_raw_roundtrip_all() {
        let all_bits = DMASTATUS_TI
            | DMASTATUS_TPS
            | DMASTATUS_TU
            | DMASTATUS_UNF
            | DMASTATUS_RI
            | DMASTATUS_RPS
            | DMASTATUS_RU
            | DMASTATUS_OVF
            | DMASTATUS_FBI
            | DMASTATUS_NIS
            | DMASTATUS_AIS;

        let status = InterruptStatus::from_raw(all_bits);
        let roundtrip = status.to_raw();

        assert_eq!(roundtrip, all_bits);
    }

    #[test]
    fn interrupt_status_to_raw_zero() {
        let status = InterruptStatus::default();
        let raw = status.to_raw();

        assert_eq!(raw, 0);
    }

    #[test]
    fn interrupt_status_any_false_when_zero() {
        let status = InterruptStatus::from_raw(0);
        assert!(!status.any());
    }

    #[test]
    fn interrupt_status_any_true_for_tx() {
        let status = InterruptStatus::from_raw(DMASTATUS_TI);
        assert!(status.any());
    }

    #[test]
    fn interrupt_status_any_true_for_rx() {
        let status = InterruptStatus::from_raw(DMASTATUS_RI);
        assert!(status.any());
    }

    #[test]
    fn interrupt_status_any_true_for_error() {
        let status = InterruptStatus::from_raw(DMASTATUS_FBI);
        assert!(status.any());
    }

    #[test]
    fn interrupt_status_any_ignores_summary_bits() {
        let status = InterruptStatus::from_raw(DMASTATUS_NIS | DMASTATUS_AIS);
        assert!(!status.any());
    }

    #[test]
    fn interrupt_status_has_error_false_when_zero() {
        let status = InterruptStatus::from_raw(0);
        assert!(!status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_false_for_normal() {
        let status = InterruptStatus::from_raw(DMASTATUS_TI | DMASTATUS_RI | DMASTATUS_NIS);
        assert!(!status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_underflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_UNF);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_overflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_OVF);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_fatal_bus() {
        let status = InterruptStatus::from_raw(DMASTATUS_FBI);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_multiple_errors() {
        let status = InterruptStatus::from_raw(DMASTATUS_UNF | DMASTATUS_OVF | DMASTATUS_FBI);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_default_is_zero() {
        let status = InterruptStatus::default();

        assert!(!status.any());
        assert!(!status.has_error());
        assert_eq!(status.to_raw(), 0);
    }
}
