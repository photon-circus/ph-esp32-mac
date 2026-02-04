//! Receive DMA Descriptor
//!
//! The RX descriptor controls frame reception and reports status after completion.

use super::VolatileCell;
use crate::internal::descriptor_bits::{rdes0, rdes1, rdes4};

// =============================================================================
// RDES0 (RX Descriptor Word 0) - Status
// =============================================================================

// Import constants from internal module for use within this module
use rdes0::ALL_ERRORS as RDES0_ALL_ERRORS_INT;
use rdes0::CRC_ERR as RDES0_CRC_ERR_INT;
use rdes0::DA_FILTER_FAIL as RDES0_DA_FILTER_FAIL_INT;
use rdes0::DESC_ERR as RDES0_DESC_ERR_INT;
use rdes0::DRIBBLE_ERR as RDES0_DRIBBLE_ERR_INT;
use rdes0::ERR_SUMMARY as RDES0_ERR_SUMMARY_INT;
use rdes0::EXT_STATUS as RDES0_EXT_STATUS_INT;
use rdes0::FIRST_DESC as RDES0_FIRST_DESC_INT;
use rdes0::FRAME_LEN_MASK as RDES0_FRAME_LEN_MASK_INT;
use rdes0::FRAME_LEN_SHIFT as RDES0_FRAME_LEN_SHIFT_INT;
use rdes0::FRAME_TYPE as RDES0_FRAME_TYPE_INT;
use rdes0::LAST_DESC as RDES0_LAST_DESC_INT;
use rdes0::LATE_COLLISION as RDES0_LATE_COLLISION_INT;
use rdes0::LENGTH_ERR as RDES0_LENGTH_ERR_INT;
use rdes0::OVERFLOW_ERR as RDES0_OVERFLOW_ERR_INT;
use rdes0::OWN as RDES0_OWN_INT;
use rdes0::RX_ERR as RDES0_RX_ERR_INT;
use rdes0::RX_WATCHDOG as RDES0_RX_WATCHDOG_INT;
use rdes0::SA_FILTER_FAIL as RDES0_SA_FILTER_FAIL_INT;
use rdes0::TIMESTAMP_AVAIL as RDES0_TIMESTAMP_AVAIL_INT;
use rdes0::VLAN_TAG as RDES0_VLAN_TAG_INT;

use rdes1::BUFFER1_SIZE_MASK as RDES1_BUFFER1_SIZE_MASK_INT;
use rdes1::BUFFER1_SIZE_SHIFT as RDES1_BUFFER1_SIZE_SHIFT_INT;
use rdes1::BUFFER2_SIZE_MASK as RDES1_BUFFER2_SIZE_MASK_INT;
use rdes1::BUFFER2_SIZE_SHIFT as RDES1_BUFFER2_SIZE_SHIFT_INT;
use rdes1::DISABLE_IRQ as RDES1_DISABLE_IRQ_INT;
use rdes1::RX_END_OF_RING as RDES1_RX_END_OF_RING_INT;
use rdes1::SECOND_ADDR_CHAINED as RDES1_SECOND_ADDR_CHAINED_INT;

// =============================================================================
// Deprecated Re-exports for Backward Compatibility
// =============================================================================
// These constants are deprecated. Use `crate::internal::descriptor_bits::rdes0::*` instead.

/// Extended Status Available (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::EXT_STATUS")]
pub const RDES0_EXT_STATUS: u32 = RDES0_EXT_STATUS_INT;
/// CRC Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::CRC_ERR")]
pub const RDES0_CRC_ERR: u32 = RDES0_CRC_ERR_INT;
/// Dribble Bit Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::DRIBBLE_ERR")]
pub const RDES0_DRIBBLE_ERR: u32 = RDES0_DRIBBLE_ERR_INT;
/// Receive Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::RX_ERR")]
pub const RDES0_RX_ERR: u32 = RDES0_RX_ERR_INT;
/// Receive Watchdog Timeout (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::RX_WATCHDOG")]
pub const RDES0_RX_WATCHDOG: u32 = RDES0_RX_WATCHDOG_INT;
/// Frame Type (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::FRAME_TYPE")]
pub const RDES0_FRAME_TYPE: u32 = RDES0_FRAME_TYPE_INT;
/// Late Collision (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::LATE_COLLISION")]
pub const RDES0_LATE_COLLISION: u32 = RDES0_LATE_COLLISION_INT;
/// Timestamp Available (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::TIMESTAMP_AVAIL")]
pub const RDES0_TIMESTAMP_AVAIL: u32 = RDES0_TIMESTAMP_AVAIL_INT;
/// Last Descriptor (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::LAST_DESC")]
pub const RDES0_LAST_DESC: u32 = RDES0_LAST_DESC_INT;
/// First Descriptor (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::FIRST_DESC")]
pub const RDES0_FIRST_DESC: u32 = RDES0_FIRST_DESC_INT;
/// VLAN Tag (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::VLAN_TAG")]
pub const RDES0_VLAN_TAG: u32 = RDES0_VLAN_TAG_INT;
/// Overflow Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::OVERFLOW_ERR")]
pub const RDES0_OVERFLOW_ERR: u32 = RDES0_OVERFLOW_ERR_INT;
/// Length Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::LENGTH_ERR")]
pub const RDES0_LENGTH_ERR: u32 = RDES0_LENGTH_ERR_INT;
/// Source Address Filter Fail (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::SA_FILTER_FAIL")]
pub const RDES0_SA_FILTER_FAIL: u32 = RDES0_SA_FILTER_FAIL_INT;
/// Descriptor Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::DESC_ERR")]
pub const RDES0_DESC_ERR: u32 = RDES0_DESC_ERR_INT;
/// Error Summary (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::ERR_SUMMARY")]
pub const RDES0_ERR_SUMMARY: u32 = RDES0_ERR_SUMMARY_INT;
/// Frame Length shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::FRAME_LEN_SHIFT")]
pub const RDES0_FRAME_LEN_SHIFT: u32 = RDES0_FRAME_LEN_SHIFT_INT;
/// Frame Length mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::FRAME_LEN_MASK")]
pub const RDES0_FRAME_LEN_MASK: u32 = RDES0_FRAME_LEN_MASK_INT;
/// Destination Address Filter Fail (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::DA_FILTER_FAIL")]
pub const RDES0_DA_FILTER_FAIL: u32 = RDES0_DA_FILTER_FAIL_INT;
/// OWN bit (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::OWN")]
pub const RDES0_OWN: u32 = RDES0_OWN_INT;
/// All error bits (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes0::ALL_ERRORS")]
pub const RDES0_ALL_ERRORS: u32 = RDES0_ALL_ERRORS_INT;

// =============================================================================
// RDES1 (RX Descriptor Word 1) - Control
// =============================================================================

/// Buffer 1 size mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes1::BUFFER1_SIZE_MASK")]
pub const RDES1_BUFFER1_SIZE_MASK: u32 = RDES1_BUFFER1_SIZE_MASK_INT;
/// Buffer 1 size shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes1::BUFFER1_SIZE_SHIFT")]
pub const RDES1_BUFFER1_SIZE_SHIFT: u32 = RDES1_BUFFER1_SIZE_SHIFT_INT;
/// Second address chained (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes1::SECOND_ADDR_CHAINED")]
pub const RDES1_SECOND_ADDR_CHAINED: u32 = RDES1_SECOND_ADDR_CHAINED_INT;
/// End of ring (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes1::RX_END_OF_RING")]
pub const RDES1_RX_END_OF_RING: u32 = RDES1_RX_END_OF_RING_INT;
/// Buffer 2 size mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes1::BUFFER2_SIZE_MASK")]
pub const RDES1_BUFFER2_SIZE_MASK: u32 = RDES1_BUFFER2_SIZE_MASK_INT;
/// Buffer 2 size shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes1::BUFFER2_SIZE_SHIFT")]
pub const RDES1_BUFFER2_SIZE_SHIFT: u32 = RDES1_BUFFER2_SIZE_SHIFT_INT;
/// Disable IRQ (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes1::DISABLE_IRQ")]
pub const RDES1_DISABLE_IRQ: u32 = RDES1_DISABLE_IRQ_INT;

// =============================================================================
// RDES4 (Extended Status) - when RDES0.EXT_STATUS is set
// =============================================================================

/// IP Payload Type shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::IP_PAYLOAD_TYPE_SHIFT")]
pub const RDES4_IP_PAYLOAD_TYPE_SHIFT: u32 = rdes4::IP_PAYLOAD_TYPE_SHIFT;
/// IP Payload Type mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::IP_PAYLOAD_TYPE_MASK")]
pub const RDES4_IP_PAYLOAD_TYPE_MASK: u32 = rdes4::IP_PAYLOAD_TYPE_MASK;
/// IP Header Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::IP_HEADER_ERR")]
pub const RDES4_IP_HEADER_ERR: u32 = rdes4::IP_HEADER_ERR;
/// IP Payload Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::IP_PAYLOAD_ERR")]
pub const RDES4_IP_PAYLOAD_ERR: u32 = rdes4::IP_PAYLOAD_ERR;
/// IP Checksum Bypass (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::IP_CHECKSUM_BYPASS")]
pub const RDES4_IP_CHECKSUM_BYPASS: u32 = rdes4::IP_CHECKSUM_BYPASS;
/// IPv4 Packet (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::IPV4_PKT")]
pub const RDES4_IPV4_PKT: u32 = rdes4::IPV4_PKT;
/// IPv6 Packet (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::IPV6_PKT")]
pub const RDES4_IPV6_PKT: u32 = rdes4::IPV6_PKT;
/// PTP Message Type shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::PTP_MSG_TYPE_SHIFT")]
pub const RDES4_PTP_MSG_TYPE_SHIFT: u32 = rdes4::PTP_MSG_TYPE_SHIFT;
/// PTP Message Type mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::PTP_MSG_TYPE_MASK")]
pub const RDES4_PTP_MSG_TYPE_MASK: u32 = rdes4::PTP_MSG_TYPE_MASK;
/// PTP Frame Type (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::PTP_FRAME_TYPE")]
pub const RDES4_PTP_FRAME_TYPE: u32 = rdes4::PTP_FRAME_TYPE;
/// PTP Version (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::PTP_VERSION")]
pub const RDES4_PTP_VERSION: u32 = rdes4::PTP_VERSION;
/// Timestamp Dropped (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::TIMESTAMP_DROPPED")]
pub const RDES4_TIMESTAMP_DROPPED: u32 = rdes4::TIMESTAMP_DROPPED;
/// AV Tagged (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::AV_TAGGED")]
pub const RDES4_AV_TAGGED: u32 = rdes4::AV_TAGGED;
/// AV Control/Data (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::AV_CTRL_DATA")]
pub const RDES4_AV_CTRL_DATA: u32 = rdes4::AV_CTRL_DATA;
/// L3 Filter Match (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::L3_FILTER_MATCH")]
pub const RDES4_L3_FILTER_MATCH: u32 = rdes4::L3_FILTER_MATCH;
/// L4 Filter Match (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::L4_FILTER_MATCH")]
pub const RDES4_L4_FILTER_MATCH: u32 = rdes4::L4_FILTER_MATCH;
/// L3/L4 Filter Number shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::L3_L4_FILTER_NUM_SHIFT")]
pub const RDES4_L3_L4_FILTER_NUM_SHIFT: u32 = rdes4::L3_L4_FILTER_NUM_SHIFT;
/// L3/L4 Filter Number mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::rdes4::L3_L4_FILTER_NUM_MASK")]
pub const RDES4_L3_L4_FILTER_NUM_MASK: u32 = rdes4::L3_L4_FILTER_NUM_MASK;

// =============================================================================
// RxDescriptor Structure
// =============================================================================

/// Receive DMA Descriptor
///
/// This structure must be aligned to 4 bytes for ESP32 or 64 bytes for ESP32-P4.
/// All fields are accessed through volatile operations.
#[repr(C)]
#[cfg_attr(not(feature = "esp32p4"), repr(align(4)))]
#[cfg_attr(feature = "esp32p4", repr(align(64)))]
pub struct RxDescriptor {
    /// RDES0: Status bits
    rdes0: VolatileCell<u32>,
    /// RDES1: Control and buffer sizes
    rdes1: VolatileCell<u32>,
    /// RDES2: Buffer 1 address
    buffer1_addr: VolatileCell<u32>,
    /// RDES3: Buffer 2 / Next descriptor address (in chained mode)
    buffer2_next_desc: VolatileCell<u32>,
    /// RDES4: Extended status (when enabled)
    extended_status: VolatileCell<u32>,
    /// Reserved
    _reserved: u32,
    /// RDES6: Timestamp low (when timestamping enabled)
    timestamp_low: VolatileCell<u32>,
    /// RDES7: Timestamp high (when timestamping enabled)
    timestamp_high: VolatileCell<u32>,
}

impl RxDescriptor {
    /// Size of the descriptor in bytes
    #[cfg(not(feature = "esp32p4"))]
    pub const SIZE: usize = 32;

    /// Size of the descriptor in bytes (ESP32-P4 with cache alignment)
    #[cfg(feature = "esp32p4")]
    pub const SIZE: usize = 64;

    /// Create a new uninitialized descriptor
    ///
    /// All fields are zeroed. Call `init()` or `setup_chained()` before use.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rdes0: VolatileCell::new(0),
            rdes1: VolatileCell::new(0),
            buffer1_addr: VolatileCell::new(0),
            buffer2_next_desc: VolatileCell::new(0),
            extended_status: VolatileCell::new(0),
            _reserved: 0,
            timestamp_low: VolatileCell::new(0),
            timestamp_high: VolatileCell::new(0),
        }
    }

    /// Initialize the descriptor in chained mode
    ///
    /// # Arguments
    /// * `buffer` - Pointer to the data buffer
    /// * `buffer_size` - Size of the data buffer in bytes
    /// * `next_desc` - Pointer to the next descriptor in the chain
    pub fn setup_chained(
        &self,
        buffer: *mut u8,
        buffer_size: usize,
        next_desc: *const RxDescriptor,
    ) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(next_desc as u32);
        self.rdes1.set(
            RDES1_SECOND_ADDR_CHAINED_INT | ((buffer_size as u32) & RDES1_BUFFER1_SIZE_MASK_INT),
        );
        // Give ownership to DMA
        self.rdes0.set(RDES0_OWN_INT);
    }

    /// Initialize as end of ring (last descriptor wraps to first)
    pub fn setup_end_of_ring(
        &self,
        buffer: *mut u8,
        buffer_size: usize,
        first_desc: *const RxDescriptor,
    ) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(first_desc as u32);
        self.rdes1.set(
            RDES1_SECOND_ADDR_CHAINED_INT
                | RDES1_RX_END_OF_RING_INT
                | ((buffer_size as u32) & RDES1_BUFFER1_SIZE_MASK_INT),
        );
        // Give ownership to DMA
        self.rdes0.set(RDES0_OWN_INT);
    }

    /// Check if descriptor is owned by DMA
    #[inline(always)]
    #[must_use]
    pub fn is_owned(&self) -> bool {
        (self.rdes0.get() & RDES0_OWN_INT) != 0
    }

    /// Give descriptor ownership to DMA
    #[inline(always)]
    pub fn set_owned(&self) {
        self.rdes0.set(RDES0_OWN_INT);
    }

    /// Take ownership from DMA (for CPU use)
    #[inline(always)]
    pub fn clear_owned(&self) {
        self.rdes0.update(|v| v & !RDES0_OWN_INT);
    }

    /// Check if this is the first descriptor of a frame
    #[inline(always)]
    #[must_use]
    pub fn is_first(&self) -> bool {
        (self.rdes0.get() & RDES0_FIRST_DESC_INT) != 0
    }

    /// Check if this is the last descriptor of a frame
    #[inline(always)]
    #[must_use]
    pub fn is_last(&self) -> bool {
        (self.rdes0.get() & RDES0_LAST_DESC_INT) != 0
    }

    /// Check if this descriptor contains a complete frame (first and last)
    #[inline(always)]
    #[must_use]
    pub fn is_complete_frame(&self) -> bool {
        let status = self.rdes0.get();
        (status & (RDES0_FIRST_DESC_INT | RDES0_LAST_DESC_INT))
            == (RDES0_FIRST_DESC_INT | RDES0_LAST_DESC_INT)
    }

    /// Check if frame has any errors
    #[inline(always)]
    #[must_use]
    pub fn has_error(&self) -> bool {
        (self.rdes0.get() & RDES0_ERR_SUMMARY_INT) != 0
    }

    /// Get all error flags
    #[inline(always)]
    #[must_use]
    pub fn error_flags(&self) -> u32 {
        self.rdes0.get() & RDES0_ALL_ERRORS_INT
    }

    /// Get received frame length (only valid if this is the last descriptor)
    ///
    /// Returns the total frame length including CRC (subtract 4 for payload length)
    #[inline(always)]
    #[must_use]
    pub fn frame_length(&self) -> usize {
        ((self.rdes0.get() & RDES0_FRAME_LEN_MASK_INT) >> RDES0_FRAME_LEN_SHIFT_INT) as usize
    }

    /// Get received frame length excluding CRC
    #[inline(always)]
    #[must_use]
    pub fn payload_length(&self) -> usize {
        self.frame_length().saturating_sub(4)
    }

    /// Check if frame has a VLAN tag
    #[inline(always)]
    #[must_use]
    pub fn has_vlan_tag(&self) -> bool {
        (self.rdes0.get() & RDES0_VLAN_TAG_INT) != 0
    }

    /// Check if frame is an Ethernet type frame (vs 802.3)
    #[inline(always)]
    #[must_use]
    pub fn is_ethernet_frame(&self) -> bool {
        (self.rdes0.get() & RDES0_FRAME_TYPE_INT) != 0
    }

    /// Check if timestamp is available
    #[inline(always)]
    #[must_use]
    pub fn has_timestamp(&self) -> bool {
        (self.rdes0.get() & RDES0_TIMESTAMP_AVAIL_INT) != 0
    }

    /// Get captured timestamp (low 32 bits)
    #[inline(always)]
    #[must_use]
    pub fn timestamp_low(&self) -> u32 {
        self.timestamp_low.get()
    }

    /// Get captured timestamp (high 32 bits)
    #[inline(always)]
    #[must_use]
    pub fn timestamp_high(&self) -> u32 {
        self.timestamp_high.get()
    }

    /// Get 64-bit timestamp
    #[inline(always)]
    #[must_use]
    pub fn timestamp(&self) -> u64 {
        ((self.timestamp_high.get() as u64) << 32) | (self.timestamp_low.get() as u64)
    }

    /// Get extended status (only valid if RDES0_EXT_STATUS is set)
    #[inline(always)]
    #[must_use]
    pub fn extended_status(&self) -> u32 {
        self.extended_status.get()
    }

    /// Check if extended status is available
    #[inline(always)]
    #[must_use]
    pub fn has_extended_status(&self) -> bool {
        (self.rdes0.get() & RDES0_EXT_STATUS_INT) != 0
    }

    /// Check if this is an IPv4 packet (from extended status)
    #[inline(always)]
    #[must_use]
    pub fn is_ipv4(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IPV4_PKT) != 0
    }

    /// Check if this is an IPv6 packet (from extended status)
    #[inline(always)]
    #[must_use]
    pub fn is_ipv6(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IPV6_PKT) != 0
    }

    /// Check for IP header checksum error (from extended status)
    #[inline(always)]
    #[must_use]
    pub fn has_ip_header_error(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IP_HEADER_ERR) != 0
    }

    /// Check for IP payload checksum error (from extended status)
    #[inline(always)]
    #[must_use]
    pub fn has_ip_payload_error(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IP_PAYLOAD_ERR) != 0
    }

    /// Get buffer address
    #[inline(always)]
    #[must_use]
    pub fn buffer_addr(&self) -> u32 {
        self.buffer1_addr.get()
    }

    /// Get next descriptor address (in chained mode)
    #[inline(always)]
    #[must_use]
    pub fn next_desc_addr(&self) -> u32 {
        self.buffer2_next_desc.get()
    }

    /// Get buffer size configured in descriptor
    #[inline(always)]
    #[must_use]
    pub fn buffer_size(&self) -> usize {
        (self.rdes1.get() & RDES1_BUFFER1_SIZE_MASK_INT) as usize
    }

    /// Reset descriptor and give back to DMA
    ///
    /// Preserves buffer address, size, and chain pointer
    pub fn recycle(&self) {
        // Clear status, keep control bits, give to DMA
        self.rdes0.set(RDES0_OWN_INT);
    }

    /// Get raw RDES0 value (for debugging)
    #[inline(always)]
    #[must_use]
    pub fn raw_rdes0(&self) -> u32 {
        self.rdes0.get()
    }

    /// Get raw RDES1 value (for debugging)
    #[inline(always)]
    #[must_use]
    pub fn raw_rdes1(&self) -> u32 {
        self.rdes1.get()
    }
}

impl Default for RxDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: RxDescriptor uses volatile cells for all DMA-accessed fields
unsafe impl Sync for RxDescriptor {}
unsafe impl Send for RxDescriptor {}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    // =========================================================================
    // Layout Tests
    // =========================================================================

    #[test]
    fn rx_descriptor_size() {
        // RX descriptor must be exactly 32 bytes for ESP32
        #[cfg(not(feature = "esp32p4"))]
        assert_eq!(core::mem::size_of::<RxDescriptor>(), 32);

        // ESP32-P4 uses 64-byte cache-aligned descriptors
        #[cfg(feature = "esp32p4")]
        assert_eq!(core::mem::size_of::<RxDescriptor>(), 64);
    }

    #[test]
    fn rx_descriptor_alignment() {
        // ESP32 requires 4-byte alignment
        #[cfg(not(feature = "esp32p4"))]
        assert_eq!(core::mem::align_of::<RxDescriptor>(), 4);

        // ESP32-P4 requires 64-byte alignment for cache
        #[cfg(feature = "esp32p4")]
        assert_eq!(core::mem::align_of::<RxDescriptor>(), 64);
    }

    #[test]
    fn rx_descriptor_const_size() {
        // Verify the SIZE constant matches actual size
        assert_eq!(RxDescriptor::SIZE, core::mem::size_of::<RxDescriptor>());
    }

    // =========================================================================
    // Ownership Bit Tests
    // =========================================================================

    #[test]
    fn rx_descriptor_new_not_owned() {
        let desc = RxDescriptor::new();
        assert!(
            !desc.is_owned(),
            "New descriptor should not be owned by DMA"
        );
    }

    #[test]
    fn rx_descriptor_set_owned() {
        let desc = RxDescriptor::new();
        desc.set_owned();
        assert!(
            desc.is_owned(),
            "Descriptor should be owned after set_owned()"
        );
    }

    #[test]
    fn rx_descriptor_clear_owned() {
        let desc = RxDescriptor::new();
        desc.set_owned();
        assert!(desc.is_owned());
        desc.clear_owned();
        assert!(
            !desc.is_owned(),
            "Descriptor should not be owned after clear_owned()"
        );
    }

    #[test]
    fn rx_descriptor_own_bit_position() {
        // OWN bit should be bit 31 of RDES0
        let desc = RxDescriptor::new();
        desc.set_owned();
        let raw = desc.raw_rdes0();
        assert_eq!(raw & RDES0_OWN, RDES0_OWN, "OWN bit should be bit 31");
        assert_eq!(raw & !RDES0_OWN, 0, "No other bits should be set");
    }

    // =========================================================================
    // Status Parsing Tests
    // =========================================================================

    #[test]
    fn rx_descriptor_first_last_flags() {
        let desc = RxDescriptor::new();

        // Initially neither first nor last
        assert!(!desc.is_first());
        assert!(!desc.is_last());

        // Set first descriptor flag
        desc.rdes0.set(RDES0_FIRST_DESC);
        assert!(desc.is_first());
        assert!(!desc.is_last());

        // Set last descriptor flag
        desc.rdes0.set(RDES0_LAST_DESC);
        assert!(!desc.is_first());
        assert!(desc.is_last());

        // Set both (complete frame in single descriptor)
        desc.rdes0.set(RDES0_FIRST_DESC | RDES0_LAST_DESC);
        assert!(desc.is_first());
        assert!(desc.is_last());
        assert!(desc.is_complete_frame());
    }

    #[test]
    fn rx_descriptor_frame_length_extraction() {
        let desc = RxDescriptor::new();

        // Frame length is in bits 16-29 of RDES0
        // Set a frame length of 1500 bytes
        let test_length: u32 = 1500;
        desc.rdes0.set(test_length << RDES0_FRAME_LEN_SHIFT);
        assert_eq!(desc.frame_length(), 1500);

        // Test with other values
        desc.rdes0.set(64 << RDES0_FRAME_LEN_SHIFT);
        assert_eq!(desc.frame_length(), 64);

        desc.rdes0.set(1518 << RDES0_FRAME_LEN_SHIFT);
        assert_eq!(desc.frame_length(), 1518);
    }

    #[test]
    fn rx_descriptor_frame_length_with_other_bits() {
        let desc = RxDescriptor::new();

        // Frame length with OWN bit and first/last flags
        let length: u32 = 256;
        desc.rdes0.set(
            RDES0_OWN | RDES0_FIRST_DESC | RDES0_LAST_DESC | (length << RDES0_FRAME_LEN_SHIFT),
        );

        assert_eq!(desc.frame_length(), 256);
        assert!(desc.is_owned());
        assert!(desc.is_complete_frame());
    }

    #[test]
    fn rx_descriptor_error_detection() {
        let desc = RxDescriptor::new();

        // No errors initially
        assert!(!desc.has_error());

        // Error summary bit triggers has_error
        desc.rdes0.set(RDES0_ERR_SUMMARY);
        assert!(desc.has_error());

        // Clear and verify
        desc.rdes0.set(0);
        assert!(!desc.has_error());

        // Multiple error bits including summary
        desc.rdes0
            .set(RDES0_ERR_SUMMARY | RDES0_CRC_ERR | RDES0_OVERFLOW_ERR);
        assert!(desc.has_error());

        // Can check error flags directly
        let errors = desc.error_flags();
        assert!(errors & RDES0_CRC_ERR != 0);
        assert!(errors & RDES0_OVERFLOW_ERR != 0);
    }

    // =========================================================================
    // Buffer Configuration Tests
    // =========================================================================

    #[test]
    fn rx_descriptor_buffer_size() {
        let desc = RxDescriptor::new();

        // Set buffer size in RDES1
        desc.rdes1.set(1600 & RDES1_BUFFER1_SIZE_MASK);
        assert_eq!(desc.buffer_size(), 1600);

        desc.rdes1.set(512 & RDES1_BUFFER1_SIZE_MASK);
        assert_eq!(desc.buffer_size(), 512);
    }

    #[test]
    fn rx_descriptor_recycle() {
        let desc = RxDescriptor::new();

        // Set up some state
        desc.rdes0
            .set(RDES0_FIRST_DESC | RDES0_LAST_DESC | (100 << RDES0_FRAME_LEN_SHIFT));
        desc.rdes1.set(1600);

        // Recycle should reset status and give to DMA
        desc.recycle();

        assert!(desc.is_owned());
        // Buffer size should be preserved in rdes1
        assert_eq!(desc.buffer_size(), 1600);
    }
}
