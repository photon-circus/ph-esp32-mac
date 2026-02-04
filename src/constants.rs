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
