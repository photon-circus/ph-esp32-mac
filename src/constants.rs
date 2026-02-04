//! Centralized Constants
//!
//! This module provides a single source of truth for all magic numbers and
//! configuration constants used throughout the EMAC driver.
//!
//! # Organization
//!
//! Constants are grouped by category:
//! - **Frame/Buffer sizes**: Ethernet frame dimensions
//! - **Timing**: Timeouts, delays, and polling intervals
//! - **Clock frequencies**: RMII/MII clock speeds, MDC, CPU
//! - **Flow control**: IEEE 802.3 PAUSE frame defaults
//! - **Default configurations**: Default buffer counts, water marks
//!
//! # Note
//!
//! Hardware register bit definitions remain in their respective modules
//! (`register/dma.rs`, `register/mac.rs`, etc.) as they are specific to
//! those hardware blocks.

// =============================================================================
// Frame and Buffer Sizes
// =============================================================================

/// Maximum Ethernet frame size including VLAN tag (1500 + 14 header + 4 CRC + 4 VLAN)
pub const MAX_FRAME_SIZE: usize = 1522;

/// Standard Ethernet MTU (Maximum Transmission Unit)
pub const MTU: usize = 1500;

/// Ethernet header size (dst MAC + src MAC + EtherType)
pub const ETH_HEADER_SIZE: usize = 14;

/// CRC/FCS size at end of frame
pub const CRC_SIZE: usize = 4;

/// VLAN tag size
pub const VLAN_TAG_SIZE: usize = 4;

/// Default DMA buffer size (supports jumbo frames)
pub const DEFAULT_BUFFER_SIZE: usize = 1600;

/// Minimum Ethernet frame size (excluding CRC)
pub const MIN_FRAME_SIZE: usize = 60;

// =============================================================================
// Default Buffer Counts
// =============================================================================

/// Default number of receive descriptors/buffers
pub const DEFAULT_RX_BUFFERS: usize = 10;

/// Default number of transmit descriptors/buffers
pub const DEFAULT_TX_BUFFERS: usize = 10;

// =============================================================================
// Timing Constants
// =============================================================================

/// Default soft reset timeout in milliseconds
pub const SOFT_RESET_TIMEOUT_MS: u32 = 100;

/// Reset poll interval in microseconds
pub const RESET_POLL_INTERVAL_US: u32 = 100;

/// Maximum iterations waiting for MII/MDIO operation
pub const MII_BUSY_TIMEOUT: u32 = 100_000;

/// Maximum iterations waiting for TX FIFO flush
pub const FLUSH_TIMEOUT: u32 = 100_000;

// =============================================================================
// Clock Frequencies
// =============================================================================

/// RMII reference clock frequency in Hz (always 50 MHz)
pub const RMII_CLK_HZ: u32 = 50_000_000;

/// MII TX/RX clock frequency for 100 Mbps (25 MHz)
pub const MII_100M_CLK_HZ: u32 = 25_000_000;

/// MII TX/RX clock frequency for 10 Mbps (2.5 MHz)
pub const MII_10M_CLK_HZ: u32 = 2_500_000;

/// Maximum MDC clock frequency per IEEE 802.3 (2.5 MHz)
pub const MDC_MAX_FREQ_HZ: u32 = 2_500_000;

// =============================================================================
// Flow Control (IEEE 802.3 PAUSE)
// =============================================================================

/// Maximum PAUSE time value (~33ms at 100Mbps)
/// Each unit = 512 bit times (slot time)
pub const PAUSE_TIME_MAX: u16 = 0xFFFF;

/// Default low water mark for flow control (as fraction of 10 buffers)
pub const DEFAULT_FLOW_LOW_WATER: usize = 3;

/// Default high water mark for flow control (as fraction of 10 buffers)
pub const DEFAULT_FLOW_HIGH_WATER: usize = 6;

// =============================================================================
// MDIO/MDC (IEEE 802.3 Clause 22)
// =============================================================================

/// CSR clock divider for 60-100 MHz APB clock (div 42 = ~1.9 MHz MDC)
/// This is the register value for GMACMIIADDR_CR field
pub const CSR_CLOCK_DIV_42: u32 = 0;

/// CSR clock divider for 100-150 MHz APB clock (div 62)
pub const CSR_CLOCK_DIV_62: u32 = 1;

/// CSR clock divider for 20-35 MHz APB clock (div 16)
pub const CSR_CLOCK_DIV_16: u32 = 2;

/// CSR clock divider for 35-60 MHz APB clock (div 26)
pub const CSR_CLOCK_DIV_26: u32 = 3;

/// CSR clock divider for 150-250 MHz APB clock (div 102)
pub const CSR_CLOCK_DIV_102: u32 = 4;

/// CSR clock divider for 250-300 MHz APB clock (div 124)
pub const CSR_CLOCK_DIV_124: u32 = 5;

// =============================================================================
// MAC Address
// =============================================================================

/// Default locally-administered MAC address
/// Bit 1 of first byte = 1 indicates locally administered
/// Bit 0 of first byte = 0 indicates unicast
pub const DEFAULT_MAC_ADDR: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

/// MAC address length in bytes
pub const MAC_ADDR_LEN: usize = 6;

// =============================================================================
// DMA State Machine (Debug)
// =============================================================================

/// TX DMA state shift in status register
pub const TX_DMA_STATE_SHIFT: u32 = 20;

/// TX DMA state mask (3 bits)
pub const TX_DMA_STATE_MASK: u32 = 0x7;

/// RX DMA state shift in status register
pub const RX_DMA_STATE_SHIFT: u32 = 17;

/// RX DMA state mask (3 bits)
pub const RX_DMA_STATE_MASK: u32 = 0x7;

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Frame Size Validation
    // =========================================================================

    #[test]
    fn max_frame_size_includes_all_headers() {
        // MAX_FRAME_SIZE = MTU + ETH_HEADER + CRC + VLAN
        assert_eq!(MAX_FRAME_SIZE, MTU + ETH_HEADER_SIZE + CRC_SIZE + VLAN_TAG_SIZE);
    }

    #[test]
    fn max_frame_size_is_1522() {
        // Standard Ethernet with VLAN: 1500 + 14 + 4 + 4 = 1522
        assert_eq!(MAX_FRAME_SIZE, 1522);
    }

    #[test]
    fn mtu_is_standard_ethernet() {
        assert_eq!(MTU, 1500);
    }

    #[test]
    fn eth_header_size_is_14_bytes() {
        // 6 bytes dst + 6 bytes src + 2 bytes EtherType = 14
        assert_eq!(ETH_HEADER_SIZE, 14);
    }

    #[test]
    fn crc_size_is_4_bytes() {
        assert_eq!(CRC_SIZE, 4);
    }

    #[test]
    fn min_frame_size_is_60_bytes() {
        // Minimum Ethernet frame excluding CRC
        assert_eq!(MIN_FRAME_SIZE, 60);
    }

    #[test]
    fn default_buffer_size_exceeds_max_frame() {
        assert!(DEFAULT_BUFFER_SIZE >= MAX_FRAME_SIZE);
    }

    // =========================================================================
    // Buffer Count Validation
    // =========================================================================

    #[test]
    fn default_buffer_counts_are_reasonable() {
        // Should have at least 2 buffers for double-buffering
        assert!(DEFAULT_RX_BUFFERS >= 2);
        assert!(DEFAULT_TX_BUFFERS >= 2);
        // But not too many to waste memory
        assert!(DEFAULT_RX_BUFFERS <= 32);
        assert!(DEFAULT_TX_BUFFERS <= 32);
    }

    // =========================================================================
    // Timing Validation
    // =========================================================================

    #[test]
    fn soft_reset_timeout_is_reasonable() {
        // Should be at least 10ms, no more than 1 second
        assert!(SOFT_RESET_TIMEOUT_MS >= 10);
        assert!(SOFT_RESET_TIMEOUT_MS <= 1000);
    }

    #[test]
    fn mii_busy_timeout_is_positive() {
        assert!(MII_BUSY_TIMEOUT > 0);
    }

    #[test]
    fn flush_timeout_is_positive() {
        assert!(FLUSH_TIMEOUT > 0);
    }

    // =========================================================================
    // Clock Frequency Validation
    // =========================================================================

    #[test]
    fn rmii_clock_is_50mhz() {
        assert_eq!(RMII_CLK_HZ, 50_000_000);
    }

    #[test]
    fn mii_100m_clock_is_25mhz() {
        assert_eq!(MII_100M_CLK_HZ, 25_000_000);
    }

    #[test]
    fn mii_10m_clock_is_2_5mhz() {
        assert_eq!(MII_10M_CLK_HZ, 2_500_000);
    }

    #[test]
    fn mdc_max_freq_per_ieee_802_3() {
        // IEEE 802.3 specifies max MDC frequency of 2.5 MHz
        assert_eq!(MDC_MAX_FREQ_HZ, 2_500_000);
    }

    #[test]
    fn mii_10m_equals_mdc_max() {
        // 10 Mbps MII clock equals max MDC frequency
        assert_eq!(MII_10M_CLK_HZ, MDC_MAX_FREQ_HZ);
    }

    // =========================================================================
    // Flow Control Validation
    // =========================================================================

    #[test]
    fn pause_time_max_is_16_bits() {
        assert_eq!(PAUSE_TIME_MAX, 0xFFFF);
    }

    #[test]
    fn flow_control_water_marks_ordered() {
        assert!(DEFAULT_FLOW_LOW_WATER < DEFAULT_FLOW_HIGH_WATER);
    }

    #[test]
    fn flow_control_water_marks_fit_buffers() {
        // Water marks should be <= default buffer count
        assert!(DEFAULT_FLOW_HIGH_WATER <= DEFAULT_RX_BUFFERS);
    }

    // =========================================================================
    // CSR Clock Divider Validation
    // =========================================================================

    #[test]
    fn csr_clock_dividers_are_sequential() {
        assert_eq!(CSR_CLOCK_DIV_42, 0);
        assert_eq!(CSR_CLOCK_DIV_62, 1);
        assert_eq!(CSR_CLOCK_DIV_16, 2);
        assert_eq!(CSR_CLOCK_DIV_26, 3);
        assert_eq!(CSR_CLOCK_DIV_102, 4);
        assert_eq!(CSR_CLOCK_DIV_124, 5);
    }

    #[test]
    fn csr_clock_dividers_fit_in_3_bits() {
        // The CR field in GMACMIIADDR is 3 bits wide
        assert!(CSR_CLOCK_DIV_42 < 8);
        assert!(CSR_CLOCK_DIV_62 < 8);
        assert!(CSR_CLOCK_DIV_16 < 8);
        assert!(CSR_CLOCK_DIV_26 < 8);
        assert!(CSR_CLOCK_DIV_102 < 8);
        assert!(CSR_CLOCK_DIV_124 < 8);
    }

    // =========================================================================
    // MAC Address Validation
    // =========================================================================

    #[test]
    fn mac_addr_len_is_6() {
        assert_eq!(MAC_ADDR_LEN, 6);
    }

    #[test]
    fn default_mac_is_locally_administered() {
        // Bit 1 of first byte = 1 indicates locally administered
        assert_eq!(DEFAULT_MAC_ADDR[0] & 0x02, 0x02);
    }

    #[test]
    fn default_mac_is_unicast() {
        // Bit 0 of first byte = 0 indicates unicast
        assert_eq!(DEFAULT_MAC_ADDR[0] & 0x01, 0x00);
    }

    #[test]
    fn default_mac_has_correct_length() {
        assert_eq!(DEFAULT_MAC_ADDR.len(), MAC_ADDR_LEN);
    }

    // =========================================================================
    // DMA State Machine Masks
    // =========================================================================

    #[test]
    fn dma_state_masks_are_3_bits() {
        assert_eq!(TX_DMA_STATE_MASK, 0x7);
        assert_eq!(RX_DMA_STATE_MASK, 0x7);
    }

    #[test]
    fn tx_dma_state_shift_position() {
        // TX DMA state is at bits 22:20 in status register
        assert_eq!(TX_DMA_STATE_SHIFT, 20);
    }

    #[test]
    fn rx_dma_state_shift_position() {
        // RX DMA state is at bits 19:17 in status register
        assert_eq!(RX_DMA_STATE_SHIFT, 17);
    }

    #[test]
    fn dma_state_fields_dont_overlap() {
        let tx_bits = TX_DMA_STATE_MASK << TX_DMA_STATE_SHIFT;
        let rx_bits = RX_DMA_STATE_MASK << RX_DMA_STATE_SHIFT;
        assert_eq!(tx_bits & rx_bits, 0, "TX and RX DMA state fields overlap");
    }
}
