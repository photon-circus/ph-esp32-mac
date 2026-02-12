//! DMA descriptor bit field constants.
//!
//! Based on ESP32 TRM Chapter 10 and IEEE 802.3.

#![allow(dead_code)]

// =============================================================================
// RDES0 (RX Descriptor Word 0) - Status
// =============================================================================

/// RX Descriptor Word 0 bit field constants
pub mod rdes0 {
    /// Extended Status Available - extended status in RDES4 is valid
    pub const EXT_STATUS: u32 = 1 << 0;
    /// CRC Error - frame has CRC error
    pub const CRC_ERR: u32 = 1 << 1;
    /// Dribble Bit Error - frame contains non-integer multiple of 8 bits
    pub const DRIBBLE_ERR: u32 = 1 << 2;
    /// Receive Error - error reported by PHY (RX_ER signal)
    pub const RX_ERR: u32 = 1 << 3;
    /// Receive Watchdog Timeout - frame truncated due to watchdog
    pub const RX_WATCHDOG: u32 = 1 << 4;
    /// Frame Type - 1 = Ethernet frame (length/type > 0x600)
    pub const FRAME_TYPE: u32 = 1 << 5;
    /// Late Collision - collision detected after 64 bytes
    pub const LATE_COLLISION: u32 = 1 << 6;
    /// Timestamp Available - IEEE 1588 timestamp captured in RDES6/RDES7
    pub const TIMESTAMP_AVAIL: u32 = 1 << 7;
    /// Last Descriptor - this is the last descriptor for the frame
    pub const LAST_DESC: u32 = 1 << 8;
    /// First Descriptor - this is the first descriptor for the frame
    pub const FIRST_DESC: u32 = 1 << 9;
    /// VLAN Tag - frame is VLAN tagged
    pub const VLAN_TAG: u32 = 1 << 10;
    /// Overflow Error - DMA buffer overflow
    pub const OVERFLOW_ERR: u32 = 1 << 11;
    /// Length Error - actual length doesn't match length/type field
    pub const LENGTH_ERR: u32 = 1 << 12;
    /// Source Address Filter Fail - frame failed SA filter
    pub const SA_FILTER_FAIL: u32 = 1 << 13;
    /// Descriptor Error - descriptor not available or bus error
    pub const DESC_ERR: u32 = 1 << 14;
    /// Error Summary - logical OR of error bits
    pub const ERR_SUMMARY: u32 = 1 << 15;
    /// Frame Length shift (14 bits)
    pub const FRAME_LEN_SHIFT: u32 = 16;
    /// Frame Length mask
    pub const FRAME_LEN_MASK: u32 = 0x3FFF << 16;
    /// Destination Address Filter Fail - frame failed DA filter
    pub const DA_FILTER_FAIL: u32 = 1 << 30;
    /// OWN - when set, descriptor owned by DMA; when clear, owned by CPU
    pub const OWN: u32 = 1 << 31;

    /// All possible RX error bits
    pub const ALL_ERRORS: u32 = CRC_ERR
        | DRIBBLE_ERR
        | RX_ERR
        | RX_WATCHDOG
        | LATE_COLLISION
        | OVERFLOW_ERR
        | LENGTH_ERR
        | DESC_ERR;
}

// =============================================================================
// RDES1 (RX Descriptor Word 1) - Control
// =============================================================================

/// RX Descriptor Word 1 bit field constants
pub mod rdes1 {
    /// RX Buffer 1 Size mask (13 bits)
    pub const BUFFER1_SIZE_MASK: u32 = 0x1FFF;
    /// RX Buffer 1 Size shift
    pub const BUFFER1_SIZE_SHIFT: u32 = 0;
    /// Second Address Chained - buffer2 contains next descriptor address
    pub const SECOND_ADDR_CHAINED: u32 = 1 << 14;
    /// Receive End of Ring - this is the last descriptor in the ring
    pub const RX_END_OF_RING: u32 = 1 << 15;
    /// RX Buffer 2 Size mask (13 bits)
    pub const BUFFER2_SIZE_MASK: u32 = 0x1FFF << 16;
    /// RX Buffer 2 Size shift
    pub const BUFFER2_SIZE_SHIFT: u32 = 16;
    /// Disable Interrupt on Completion
    pub const DISABLE_IRQ: u32 = 1 << 31;
}

// =============================================================================
// RDES4 (Extended Status) - when RDES0.EXT_STATUS is set
// =============================================================================

/// RX Descriptor Word 4 (Extended Status) bit field constants
pub mod rdes4 {
    /// IP Payload Type shift (3 bits)
    pub const IP_PAYLOAD_TYPE_SHIFT: u32 = 0;
    /// IP Payload Type mask
    pub const IP_PAYLOAD_TYPE_MASK: u32 = 0x7;
    /// IP Header Error
    pub const IP_HEADER_ERR: u32 = 1 << 3;
    /// IP Payload Error
    pub const IP_PAYLOAD_ERR: u32 = 1 << 4;
    /// IP Checksum Bypassed
    pub const IP_CHECKSUM_BYPASS: u32 = 1 << 5;
    /// IPv4 Packet Received
    pub const IPV4_PKT: u32 = 1 << 6;
    /// IPv6 Packet Received
    pub const IPV6_PKT: u32 = 1 << 7;
    /// PTP Message Type shift (4 bits)
    pub const PTP_MSG_TYPE_SHIFT: u32 = 8;
    /// PTP Message Type mask
    pub const PTP_MSG_TYPE_MASK: u32 = 0xF << 8;
    /// PTP Frame Type (1 = PTPv2, 0 = PTPv1)
    pub const PTP_FRAME_TYPE: u32 = 1 << 12;
    /// PTP Version (within PTPv2)
    pub const PTP_VERSION: u32 = 1 << 13;
    /// Timestamp Dropped
    pub const TIMESTAMP_DROPPED: u32 = 1 << 14;
    /// AV Tagged Packet
    pub const AV_TAGGED: u32 = 1 << 16;
    /// AV Tagged Packet control/data
    pub const AV_CTRL_DATA: u32 = 1 << 17;
    /// Layer 3 Filter Match
    pub const L3_FILTER_MATCH: u32 = 1 << 24;
    /// Layer 4 Filter Match
    pub const L4_FILTER_MATCH: u32 = 1 << 25;
    /// Layer 3/4 Filter Number Matched shift
    pub const L3_L4_FILTER_NUM_SHIFT: u32 = 26;
    /// Layer 3/4 Filter Number Matched mask
    pub const L3_L4_FILTER_NUM_MASK: u32 = 0x3 << 26;
}

// =============================================================================
// TDES0 (TX Descriptor Word 0) - Status/Control
// =============================================================================

/// TX Descriptor Word 0 bit field constants
pub mod tdes0 {
    /// Deferred Bit - set when frame transmission is deferred
    pub const DEFERRED: u32 = 1 << 0;
    /// Underflow Error - TX FIFO underflow during frame transmission
    pub const UNDERFLOW_ERR: u32 = 1 << 1;
    /// Excessive Deferral - frame deferred for more than 24288 bit times
    pub const EXCESSIVE_DEFERRAL: u32 = 1 << 2;
    /// Collision Count shift (4 bits)
    pub const COLLISION_COUNT_SHIFT: u32 = 3;
    /// Collision Count mask
    pub const COLLISION_COUNT_MASK: u32 = 0xF << 3;
    /// VLAN Frame - frame is a VLAN tagged frame
    pub const VLAN_FRAME: u32 = 1 << 7;
    /// Excessive Collision - more than 16 collisions
    pub const EXCESSIVE_COLLISION: u32 = 1 << 8;
    /// Late Collision - collision after 64 byte times
    pub const LATE_COLLISION: u32 = 1 << 9;
    /// No Carrier - carrier sense signal not asserted
    pub const NO_CARRIER: u32 = 1 << 10;
    /// Loss of Carrier - carrier lost during transmission
    pub const LOSS_OF_CARRIER: u32 = 1 << 11;
    /// IP Payload Error - checksum error in payload
    pub const IP_PAYLOAD_ERR: u32 = 1 << 12;
    /// Frame Flushed - frame flushed due to SW flush
    pub const FRAME_FLUSHED: u32 = 1 << 13;
    /// Jabber Timeout - transmission continued beyond 2048 bytes
    pub const JABBER_TIMEOUT: u32 = 1 << 14;
    /// Error Summary - logical OR of all error bits
    pub const ERR_SUMMARY: u32 = 1 << 15;
    /// IP Header Error - checksum error in IP header
    pub const IP_HEADER_ERR: u32 = 1 << 16;
    /// TX Timestamp Status - timestamp captured
    pub const TX_TIMESTAMP_STATUS: u32 = 1 << 17;
    /// VLAN Insertion Control shift (2 bits)
    pub const VLAN_INSERT_CTRL_SHIFT: u32 = 18;
    /// VLAN Insertion Control mask
    pub const VLAN_INSERT_CTRL_MASK: u32 = 0x3 << 18;
    /// Second Address Chained - buffer2 contains next descriptor address
    pub const SECOND_ADDR_CHAINED: u32 = 1 << 20;
    /// Transmit End of Ring - this is the last descriptor in the ring
    pub const TX_END_OF_RING: u32 = 1 << 21;
    /// Checksum Insertion Control shift (2 bits)
    pub const CHECKSUM_INSERT_SHIFT: u32 = 22;
    /// Checksum Insertion Control mask
    pub const CHECKSUM_INSERT_MASK: u32 = 0x3 << 22;
    /// CRC Replacement Control - replace CRC with calculated value
    pub const CRC_REPLACE: u32 = 1 << 24;
    /// Transmit Timestamp Enable - capture timestamp on transmission
    pub const TX_TIMESTAMP_EN: u32 = 1 << 25;
    /// Disable Pad - do not add padding to short frames
    pub const DISABLE_PAD: u32 = 1 << 26;
    /// Disable CRC - do not append CRC to frame
    pub const DISABLE_CRC: u32 = 1 << 27;
    /// First Segment - buffer contains first segment of frame
    pub const FIRST_SEGMENT: u32 = 1 << 28;
    /// Last Segment - buffer contains last segment of frame
    pub const LAST_SEGMENT: u32 = 1 << 29;
    /// Interrupt on Completion - generate interrupt when transmission complete
    pub const INTERRUPT_ON_COMPLETE: u32 = 1 << 30;
    /// OWN - when set, descriptor is owned by DMA; when clear, owned by CPU
    pub const OWN: u32 = 1 << 31;

    /// All possible TX error bits
    pub const ALL_ERRORS: u32 = UNDERFLOW_ERR
        | EXCESSIVE_DEFERRAL
        | EXCESSIVE_COLLISION
        | LATE_COLLISION
        | NO_CARRIER
        | LOSS_OF_CARRIER
        | IP_PAYLOAD_ERR
        | JABBER_TIMEOUT
        | IP_HEADER_ERR;

    /// Control flags that should be preserved on first segment
    pub const FS_CTRL_FLAGS: u32 =
        VLAN_INSERT_CTRL_MASK | TX_TIMESTAMP_EN | DISABLE_PAD | DISABLE_CRC;

    /// Control flags that should be preserved on last segment
    pub const LS_CTRL_FLAGS: u32 = CHECKSUM_INSERT_MASK | CRC_REPLACE | INTERRUPT_ON_COMPLETE;
}

// =============================================================================
// TDES1 (TX Descriptor Word 1) - Buffer Sizes
// =============================================================================

/// TX Descriptor Word 1 bit field constants
pub mod tdes1 {
    /// TX Buffer 1 Size mask (13 bits)
    pub const BUFFER1_SIZE_MASK: u32 = 0x1FFF;
    /// TX Buffer 1 Size shift
    pub const BUFFER1_SIZE_SHIFT: u32 = 0;
    /// TX Buffer 2 Size mask (13 bits)
    pub const BUFFER2_SIZE_MASK: u32 = 0x1FFF << 16;
    /// TX Buffer 2 Size shift
    pub const BUFFER2_SIZE_SHIFT: u32 = 16;
    /// Source Address Insertion/Replacement Control shift (3 bits)
    pub const SA_INSERT_CTRL_SHIFT: u32 = 29;
    /// Source Address Insertion/Replacement Control mask
    pub const SA_INSERT_CTRL_MASK: u32 = 0x7 << 29;
}

// =============================================================================
// Checksum Insertion Modes
// =============================================================================

/// TX checksum insertion mode constants
pub mod checksum_mode {
    /// Checksum insertion disabled
    pub const DISABLED: u32 = 0;
    /// Insert IP header checksum only
    pub const IP_ONLY: u32 = 1;
    /// Insert IP header and payload checksum (no pseudo-header)
    pub const IP_AND_PAYLOAD: u32 = 2;
    /// Insert IP header and payload checksum with pseudo-header
    pub const FULL: u32 = 3;
}
