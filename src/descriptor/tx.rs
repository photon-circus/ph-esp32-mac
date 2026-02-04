//! Transmit DMA Descriptor
//!
//! The TX descriptor controls frame transmission and reports status after completion.

use super::VolatileCell;
use crate::internal::descriptor_bits::{tdes0, tdes1};

// =============================================================================
// Import Internal Constants
// =============================================================================

use tdes0::ALL_ERRORS as TDES0_ALL_ERRORS_INT;
use tdes0::CHECKSUM_INSERT_MASK as TDES0_CHECKSUM_INSERT_MASK_INT;
use tdes0::CHECKSUM_INSERT_SHIFT as TDES0_CHECKSUM_INSERT_SHIFT_INT;
use tdes0::COLLISION_COUNT_MASK as TDES0_COLLISION_COUNT_MASK_INT;
use tdes0::COLLISION_COUNT_SHIFT as TDES0_COLLISION_COUNT_SHIFT_INT;
use tdes0::CRC_REPLACE as TDES0_CRC_REPLACE_INT;
use tdes0::DEFERRED as TDES0_DEFERRED_INT;
use tdes0::DISABLE_CRC as TDES0_DISABLE_CRC_INT;
use tdes0::DISABLE_PAD as TDES0_DISABLE_PAD_INT;
use tdes0::ERR_SUMMARY as TDES0_ERR_SUMMARY_INT;
use tdes0::EXCESSIVE_COLLISION as TDES0_EXCESSIVE_COLLISION_INT;
use tdes0::EXCESSIVE_DEFERRAL as TDES0_EXCESSIVE_DEFERRAL_INT;
use tdes0::FIRST_SEGMENT as TDES0_FIRST_SEGMENT_INT;
use tdes0::FRAME_FLUSHED as TDES0_FRAME_FLUSHED_INT;
use tdes0::FS_CTRL_FLAGS as TDES0_FS_CTRL_FLAGS_INT;
use tdes0::INTERRUPT_ON_COMPLETE as TDES0_INTERRUPT_ON_COMPLETE_INT;
use tdes0::IP_HEADER_ERR as TDES0_IP_HEADER_ERR_INT;
use tdes0::IP_PAYLOAD_ERR as TDES0_IP_PAYLOAD_ERR_INT;
use tdes0::JABBER_TIMEOUT as TDES0_JABBER_TIMEOUT_INT;
use tdes0::LAST_SEGMENT as TDES0_LAST_SEGMENT_INT;
use tdes0::LATE_COLLISION as TDES0_LATE_COLLISION_INT;
use tdes0::LOSS_OF_CARRIER as TDES0_LOSS_OF_CARRIER_INT;
use tdes0::LS_CTRL_FLAGS as TDES0_LS_CTRL_FLAGS_INT;
use tdes0::NO_CARRIER as TDES0_NO_CARRIER_INT;
use tdes0::OWN as TDES0_OWN_INT;
use tdes0::SECOND_ADDR_CHAINED as TDES0_SECOND_ADDR_CHAINED_INT;
use tdes0::TX_END_OF_RING as TDES0_TX_END_OF_RING_INT;
use tdes0::TX_TIMESTAMP_EN as TDES0_TX_TIMESTAMP_EN_INT;
use tdes0::TX_TIMESTAMP_STATUS as TDES0_TX_TIMESTAMP_STATUS_INT;
use tdes0::UNDERFLOW_ERR as TDES0_UNDERFLOW_ERR_INT;
use tdes0::VLAN_FRAME as TDES0_VLAN_FRAME_INT;
use tdes0::VLAN_INSERT_CTRL_MASK as TDES0_VLAN_INSERT_CTRL_MASK_INT;
use tdes0::VLAN_INSERT_CTRL_SHIFT as TDES0_VLAN_INSERT_CTRL_SHIFT_INT;

use tdes1::BUFFER1_SIZE_MASK as TDES1_BUFFER1_SIZE_MASK_INT;
use tdes1::BUFFER1_SIZE_SHIFT as TDES1_BUFFER1_SIZE_SHIFT_INT;
use tdes1::BUFFER2_SIZE_MASK as TDES1_BUFFER2_SIZE_MASK_INT;
use tdes1::BUFFER2_SIZE_SHIFT as TDES1_BUFFER2_SIZE_SHIFT_INT;
use tdes1::SA_INSERT_CTRL_MASK as TDES1_SA_INSERT_CTRL_MASK_INT;
use tdes1::SA_INSERT_CTRL_SHIFT as TDES1_SA_INSERT_CTRL_SHIFT_INT;

// =============================================================================
// TDES0 (TX Descriptor Word 0) - Deprecated Re-exports
// =============================================================================

/// Deferred Bit (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::DEFERRED")]
pub const TDES0_DEFERRED: u32 = TDES0_DEFERRED_INT;
/// Underflow Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::UNDERFLOW_ERR")]
pub const TDES0_UNDERFLOW_ERR: u32 = TDES0_UNDERFLOW_ERR_INT;
/// Excessive Deferral (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::EXCESSIVE_DEFERRAL")]
pub const TDES0_EXCESSIVE_DEFERRAL: u32 = TDES0_EXCESSIVE_DEFERRAL_INT;
/// Collision Count shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::COLLISION_COUNT_SHIFT")]
pub const TDES0_COLLISION_COUNT_SHIFT: u32 = TDES0_COLLISION_COUNT_SHIFT_INT;
/// Collision Count mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::COLLISION_COUNT_MASK")]
pub const TDES0_COLLISION_COUNT_MASK: u32 = TDES0_COLLISION_COUNT_MASK_INT;
/// VLAN Frame (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::VLAN_FRAME")]
pub const TDES0_VLAN_FRAME: u32 = TDES0_VLAN_FRAME_INT;
/// Excessive Collision (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::EXCESSIVE_COLLISION")]
pub const TDES0_EXCESSIVE_COLLISION: u32 = TDES0_EXCESSIVE_COLLISION_INT;
/// Late Collision (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::LATE_COLLISION")]
pub const TDES0_LATE_COLLISION: u32 = TDES0_LATE_COLLISION_INT;
/// No Carrier (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::NO_CARRIER")]
pub const TDES0_NO_CARRIER: u32 = TDES0_NO_CARRIER_INT;
/// Loss of Carrier (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::LOSS_OF_CARRIER")]
pub const TDES0_LOSS_OF_CARRIER: u32 = TDES0_LOSS_OF_CARRIER_INT;
/// IP Payload Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::IP_PAYLOAD_ERR")]
pub const TDES0_IP_PAYLOAD_ERR: u32 = TDES0_IP_PAYLOAD_ERR_INT;
/// Frame Flushed (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::FRAME_FLUSHED")]
pub const TDES0_FRAME_FLUSHED: u32 = TDES0_FRAME_FLUSHED_INT;
/// Jabber Timeout (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::JABBER_TIMEOUT")]
pub const TDES0_JABBER_TIMEOUT: u32 = TDES0_JABBER_TIMEOUT_INT;
/// Error Summary (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::ERR_SUMMARY")]
pub const TDES0_ERR_SUMMARY: u32 = TDES0_ERR_SUMMARY_INT;
/// IP Header Error (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::IP_HEADER_ERR")]
pub const TDES0_IP_HEADER_ERR: u32 = TDES0_IP_HEADER_ERR_INT;
/// TX Timestamp Status (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::TX_TIMESTAMP_STATUS")]
pub const TDES0_TX_TIMESTAMP_STATUS: u32 = TDES0_TX_TIMESTAMP_STATUS_INT;
/// VLAN Insert Control shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::VLAN_INSERT_CTRL_SHIFT")]
pub const TDES0_VLAN_INSERT_CTRL_SHIFT: u32 = TDES0_VLAN_INSERT_CTRL_SHIFT_INT;
/// VLAN Insert Control mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::VLAN_INSERT_CTRL_MASK")]
pub const TDES0_VLAN_INSERT_CTRL_MASK: u32 = TDES0_VLAN_INSERT_CTRL_MASK_INT;
/// Second Address Chained (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::SECOND_ADDR_CHAINED")]
pub const TDES0_SECOND_ADDR_CHAINED: u32 = TDES0_SECOND_ADDR_CHAINED_INT;
/// TX End of Ring (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::TX_END_OF_RING")]
pub const TDES0_TX_END_OF_RING: u32 = TDES0_TX_END_OF_RING_INT;
/// Checksum Insert shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::CHECKSUM_INSERT_SHIFT")]
pub const TDES0_CHECKSUM_INSERT_SHIFT: u32 = TDES0_CHECKSUM_INSERT_SHIFT_INT;
/// Checksum Insert mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::CHECKSUM_INSERT_MASK")]
pub const TDES0_CHECKSUM_INSERT_MASK: u32 = TDES0_CHECKSUM_INSERT_MASK_INT;
/// CRC Replace (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::CRC_REPLACE")]
pub const TDES0_CRC_REPLACE: u32 = TDES0_CRC_REPLACE_INT;
/// TX Timestamp Enable (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::TX_TIMESTAMP_EN")]
pub const TDES0_TX_TIMESTAMP_EN: u32 = TDES0_TX_TIMESTAMP_EN_INT;
/// Disable Pad (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::DISABLE_PAD")]
pub const TDES0_DISABLE_PAD: u32 = TDES0_DISABLE_PAD_INT;
/// Disable CRC (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::DISABLE_CRC")]
pub const TDES0_DISABLE_CRC: u32 = TDES0_DISABLE_CRC_INT;
/// First Segment (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::FIRST_SEGMENT")]
pub const TDES0_FIRST_SEGMENT: u32 = TDES0_FIRST_SEGMENT_INT;
/// Last Segment (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::LAST_SEGMENT")]
pub const TDES0_LAST_SEGMENT: u32 = TDES0_LAST_SEGMENT_INT;
/// Interrupt on Completion (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::INTERRUPT_ON_COMPLETE")]
pub const TDES0_INTERRUPT_ON_COMPLETE: u32 = TDES0_INTERRUPT_ON_COMPLETE_INT;
/// OWN bit (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::OWN")]
pub const TDES0_OWN: u32 = TDES0_OWN_INT;
/// All error bits (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::ALL_ERRORS")]
pub const TDES0_ALL_ERRORS: u32 = TDES0_ALL_ERRORS_INT;
/// First segment control flags (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::FS_CTRL_FLAGS")]
pub const TDES0_FS_CTRL_FLAGS: u32 = TDES0_FS_CTRL_FLAGS_INT;
/// Last segment control flags (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes0::LS_CTRL_FLAGS")]
pub const TDES0_LS_CTRL_FLAGS: u32 = TDES0_LS_CTRL_FLAGS_INT;

/// Checksum insertion modes (deprecated - use internal::descriptor_bits::checksum_mode)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::checksum_mode")]
pub mod checksum_mode {
    use crate::internal::descriptor_bits::checksum_mode as int_cm;
    /// Checksum insertion disabled
    pub const DISABLED: u32 = int_cm::DISABLED;
    /// Insert IP header checksum only
    pub const IP_ONLY: u32 = int_cm::IP_ONLY;
    /// Insert IP header and payload checksum (no pseudo-header)
    pub const IP_AND_PAYLOAD: u32 = int_cm::IP_AND_PAYLOAD;
    /// Insert IP header and payload checksum with pseudo-header
    pub const FULL: u32 = int_cm::FULL;
}

// =============================================================================
// TDES1 (TX Descriptor Word 1) - Deprecated Re-exports
// =============================================================================

/// Buffer 1 size mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes1::BUFFER1_SIZE_MASK")]
pub const TDES1_BUFFER1_SIZE_MASK: u32 = TDES1_BUFFER1_SIZE_MASK_INT;
/// Buffer 1 size shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes1::BUFFER1_SIZE_SHIFT")]
pub const TDES1_BUFFER1_SIZE_SHIFT: u32 = TDES1_BUFFER1_SIZE_SHIFT_INT;
/// Buffer 2 size mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes1::BUFFER2_SIZE_MASK")]
pub const TDES1_BUFFER2_SIZE_MASK: u32 = TDES1_BUFFER2_SIZE_MASK_INT;
/// Buffer 2 size shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes1::BUFFER2_SIZE_SHIFT")]
pub const TDES1_BUFFER2_SIZE_SHIFT: u32 = TDES1_BUFFER2_SIZE_SHIFT_INT;
/// SA Insert Control shift (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes1::SA_INSERT_CTRL_SHIFT")]
pub const TDES1_SA_INSERT_CTRL_SHIFT: u32 = TDES1_SA_INSERT_CTRL_SHIFT_INT;
/// SA Insert Control mask (deprecated)
#[deprecated(since = "0.2.0", note = "use internal::descriptor_bits::tdes1::SA_INSERT_CTRL_MASK")]
pub const TDES1_SA_INSERT_CTRL_MASK: u32 = TDES1_SA_INSERT_CTRL_MASK_INT;

// =============================================================================
// TxDescriptor Structure
// =============================================================================

/// Transmit DMA Descriptor
///
/// This structure must be aligned to 4 bytes for ESP32 or 64 bytes for ESP32-P4.
/// All fields are accessed through volatile operations.
#[repr(C)]
#[cfg_attr(not(feature = "esp32p4"), repr(align(4)))]
#[cfg_attr(feature = "esp32p4", repr(align(64)))]
pub struct TxDescriptor {
    /// TDES0: Status and control bits
    tdes0: VolatileCell<u32>,
    /// TDES1: Buffer sizes
    tdes1: VolatileCell<u32>,
    /// TDES2: Buffer 1 address
    buffer1_addr: VolatileCell<u32>,
    /// TDES3: Buffer 2 / Next descriptor address (in chained mode)
    buffer2_next_desc: VolatileCell<u32>,
    /// Reserved / Extended status (ESP32-P4)
    _reserved1: u32,
    /// Reserved
    _reserved2: u32,
    /// Timestamp low (when timestamping enabled)
    timestamp_low: VolatileCell<u32>,
    /// Timestamp high (when timestamping enabled)
    timestamp_high: VolatileCell<u32>,
}

impl TxDescriptor {
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
            tdes0: VolatileCell::new(0),
            tdes1: VolatileCell::new(0),
            buffer1_addr: VolatileCell::new(0),
            buffer2_next_desc: VolatileCell::new(0),
            _reserved1: 0,
            _reserved2: 0,
            timestamp_low: VolatileCell::new(0),
            timestamp_high: VolatileCell::new(0),
        }
    }

    /// Initialize the descriptor in chained mode
    ///
    /// # Arguments
    /// * `buffer` - Pointer to the data buffer
    /// * `next_desc` - Pointer to the next descriptor in the chain
    pub fn setup_chained(&self, buffer: *const u8, next_desc: *const TxDescriptor) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(next_desc as u32);
        self.tdes0.set(TDES0_SECOND_ADDR_CHAINED_INT);
        self.tdes1.set(0);
    }

    /// Initialize as end of ring (last descriptor wraps to first)
    pub fn setup_end_of_ring(&self, buffer: *const u8, first_desc: *const TxDescriptor) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(first_desc as u32);
        self.tdes0
            .set(TDES0_SECOND_ADDR_CHAINED_INT | TDES0_TX_END_OF_RING_INT);
        self.tdes1.set(0);
    }

    /// Check if descriptor is owned by DMA
    #[inline(always)]
    #[must_use]
    pub fn is_owned(&self) -> bool {
        (self.tdes0.get() & TDES0_OWN_INT) != 0
    }

    /// Give descriptor ownership to DMA
    #[inline(always)]
    pub fn set_owned(&self) {
        self.tdes0.update(|v| v | TDES0_OWN_INT);
    }

    /// Take ownership from DMA (for CPU use)
    #[inline(always)]
    pub fn clear_owned(&self) {
        self.tdes0.update(|v| v & !TDES0_OWN_INT);
    }

    /// Prepare descriptor for transmission
    ///
    /// # Arguments
    /// * `len` - Length of data in buffer
    /// * `first` - True if this is the first segment of the frame
    /// * `last` - True if this is the last segment of the frame
    pub fn prepare(&self, len: usize, first: bool, last: bool) {
        let mut flags = TDES0_SECOND_ADDR_CHAINED_INT;

        if first {
            flags |= TDES0_FIRST_SEGMENT_INT;
        }
        if last {
            flags |= TDES0_LAST_SEGMENT_INT | TDES0_INTERRUPT_ON_COMPLETE_INT;
        }

        // Set buffer size
        self.tdes1.set((len as u32) & TDES1_BUFFER1_SIZE_MASK_INT);

        // Set flags (but not OWN yet)
        self.tdes0.set(flags);
    }

    /// Prepare and give to DMA in one operation
    pub fn prepare_and_submit(&self, len: usize, first: bool, last: bool) {
        self.prepare(len, first, last);
        self.set_owned();
    }

    /// Set checksum insertion mode
    pub fn set_checksum_mode(&self, mode: u32) {
        self.tdes0.update(|v| {
            (v & !TDES0_CHECKSUM_INSERT_MASK_INT)
                | ((mode << TDES0_CHECKSUM_INSERT_SHIFT_INT) & TDES0_CHECKSUM_INSERT_MASK_INT)
        });
    }

    /// Enable timestamp capture for this frame
    pub fn enable_timestamp(&self) {
        self.tdes0.update(|v| v | TDES0_TX_TIMESTAMP_EN_INT);
    }

    /// Check if transmission had errors
    #[inline(always)]
    #[must_use]
    pub fn has_error(&self) -> bool {
        (self.tdes0.get() & TDES0_ERR_SUMMARY_INT) != 0
    }

    /// Get all error flags
    #[inline(always)]
    #[must_use]
    pub fn error_flags(&self) -> u32 {
        self.tdes0.get() & TDES0_ALL_ERRORS_INT
    }

    /// Get collision count (for half-duplex)
    #[inline(always)]
    #[must_use]
    pub fn collision_count(&self) -> u8 {
        ((self.tdes0.get() & TDES0_COLLISION_COUNT_MASK_INT) >> TDES0_COLLISION_COUNT_SHIFT_INT)
            as u8
    }

    /// Check if timestamp was captured
    #[inline(always)]
    #[must_use]
    pub fn has_timestamp(&self) -> bool {
        (self.tdes0.get() & TDES0_TX_TIMESTAMP_STATUS_INT) != 0
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

    /// Reset descriptor to initial state
    pub fn reset(&self) {
        let next = self.buffer2_next_desc.get();
        self.tdes0.set(TDES0_SECOND_ADDR_CHAINED_INT);
        self.tdes1.set(0);
        self.buffer2_next_desc.set(next);
    }

    /// Get raw TDES0 value (for debugging)
    #[inline(always)]
    #[must_use]
    pub fn raw_tdes0(&self) -> u32 {
        self.tdes0.get()
    }

    /// Get raw TDES1 value (for debugging)
    #[inline(always)]
    #[must_use]
    pub fn raw_tdes1(&self) -> u32 {
        self.tdes1.get()
    }
}

impl Default for TxDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: TxDescriptor uses volatile cells for all DMA-accessed fields
unsafe impl Sync for TxDescriptor {}
unsafe impl Send for TxDescriptor {}

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
    fn tx_descriptor_size() {
        // TX descriptor must be exactly 32 bytes for ESP32
        #[cfg(not(feature = "esp32p4"))]
        assert_eq!(core::mem::size_of::<TxDescriptor>(), 32);

        // ESP32-P4 uses 64-byte cache-aligned descriptors
        #[cfg(feature = "esp32p4")]
        assert_eq!(core::mem::size_of::<TxDescriptor>(), 64);
    }

    #[test]
    fn tx_descriptor_alignment() {
        // ESP32 requires 4-byte alignment
        #[cfg(not(feature = "esp32p4"))]
        assert_eq!(core::mem::align_of::<TxDescriptor>(), 4);

        // ESP32-P4 requires 64-byte alignment for cache
        #[cfg(feature = "esp32p4")]
        assert_eq!(core::mem::align_of::<TxDescriptor>(), 64);
    }

    #[test]
    fn tx_descriptor_const_size() {
        // Verify the SIZE constant matches actual size
        assert_eq!(TxDescriptor::SIZE, core::mem::size_of::<TxDescriptor>());
    }

    // =========================================================================
    // Ownership Bit Tests
    // =========================================================================

    #[test]
    fn tx_descriptor_new_not_owned() {
        let desc = TxDescriptor::new();
        assert!(
            !desc.is_owned(),
            "New descriptor should not be owned by DMA"
        );
    }

    #[test]
    fn tx_descriptor_set_owned() {
        let desc = TxDescriptor::new();
        desc.set_owned();
        assert!(
            desc.is_owned(),
            "Descriptor should be owned after set_owned()"
        );
    }

    #[test]
    fn tx_descriptor_clear_owned() {
        let desc = TxDescriptor::new();
        desc.set_owned();
        assert!(desc.is_owned());
        desc.clear_owned();
        assert!(
            !desc.is_owned(),
            "Descriptor should not be owned after clear_owned()"
        );
    }

    #[test]
    fn tx_descriptor_own_bit_position() {
        // OWN bit should be bit 31 of TDES0
        let desc = TxDescriptor::new();
        desc.set_owned();
        let raw = desc.raw_tdes0();
        assert_eq!(raw & TDES0_OWN, TDES0_OWN, "OWN bit should be bit 31");
    }

    // =========================================================================
    // Control Bit Tests
    // =========================================================================

    #[test]
    fn tx_descriptor_prepare_first_segment() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, false);

        let raw = desc.raw_tdes0();
        assert!(
            raw & TDES0_FIRST_SEGMENT != 0,
            "First segment flag should be set"
        );
        assert!(
            raw & TDES0_LAST_SEGMENT == 0,
            "Last segment flag should not be set"
        );
        assert!(raw & TDES0_OWN == 0, "OWN should not be set by prepare()");
    }

    #[test]
    fn tx_descriptor_prepare_last_segment() {
        let desc = TxDescriptor::new();
        desc.prepare(100, false, true);

        let raw = desc.raw_tdes0();
        assert!(
            raw & TDES0_FIRST_SEGMENT == 0,
            "First segment flag should not be set"
        );
        assert!(
            raw & TDES0_LAST_SEGMENT != 0,
            "Last segment flag should be set"
        );
        assert!(
            raw & TDES0_INTERRUPT_ON_COMPLETE != 0,
            "IOC should be set on last segment"
        );
    }

    #[test]
    fn tx_descriptor_prepare_complete_frame() {
        let desc = TxDescriptor::new();
        desc.prepare(1500, true, true);

        let raw = desc.raw_tdes0();
        assert!(raw & TDES0_FIRST_SEGMENT != 0);
        assert!(raw & TDES0_LAST_SEGMENT != 0);
        assert!(raw & TDES0_INTERRUPT_ON_COMPLETE != 0);
    }

    #[test]
    fn tx_descriptor_frame_length() {
        let desc = TxDescriptor::new();

        // Set various frame lengths and verify via raw_tdes1
        desc.prepare(64, true, true);
        let len = desc.raw_tdes1() & 0x1FFF; // TDES1_BUFFER1_SIZE_MASK
        assert_eq!(len, 64);

        desc.prepare(1500, true, true);
        let len = desc.raw_tdes1() & 0x1FFF;
        assert_eq!(len, 1500);

        desc.prepare(1518, true, true);
        let len = desc.raw_tdes1() & 0x1FFF;
        assert_eq!(len, 1518);
    }

    #[test]
    fn tx_descriptor_prepare_and_submit() {
        let desc = TxDescriptor::new();
        desc.prepare_and_submit(256, true, true);

        assert!(
            desc.is_owned(),
            "Descriptor should be owned after prepare_and_submit()"
        );

        // Verify length via raw_tdes1
        let len = desc.raw_tdes1() & 0x1FFF;
        assert_eq!(len, 256);

        let raw = desc.raw_tdes0();
        assert!(raw & TDES0_FIRST_SEGMENT != 0);
        assert!(raw & TDES0_LAST_SEGMENT != 0);
    }

    // =========================================================================
    // Checksum Mode Tests
    // =========================================================================

    #[test]
    fn tx_descriptor_checksum_mode_disabled() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, true);
        desc.set_checksum_mode(checksum_mode::DISABLED);

        let raw = desc.raw_tdes0();
        let mode = (raw & TDES0_CHECKSUM_INSERT_MASK) >> TDES0_CHECKSUM_INSERT_SHIFT;
        assert_eq!(mode, 0);
    }

    #[test]
    fn tx_descriptor_checksum_mode_ip_only() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, true);
        desc.set_checksum_mode(checksum_mode::IP_ONLY);

        let raw = desc.raw_tdes0();
        let mode = (raw & TDES0_CHECKSUM_INSERT_MASK) >> TDES0_CHECKSUM_INSERT_SHIFT;
        assert_eq!(mode, 1);
    }

    #[test]
    fn tx_descriptor_checksum_mode_full() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, true);
        desc.set_checksum_mode(checksum_mode::FULL);

        let raw = desc.raw_tdes0();
        let mode = (raw & TDES0_CHECKSUM_INSERT_MASK) >> TDES0_CHECKSUM_INSERT_SHIFT;
        assert_eq!(mode, 3);
    }

    // =========================================================================
    // Error Detection Tests
    // =========================================================================

    #[test]
    fn tx_descriptor_no_errors_initially() {
        let desc = TxDescriptor::new();
        assert!(!desc.has_error());
    }

    #[test]
    fn tx_descriptor_error_detection() {
        let desc = TxDescriptor::new();

        // has_error() only checks the error summary bit
        desc.tdes0.set(TDES0_ERR_SUMMARY);
        assert!(desc.has_error());

        // Clear and verify
        desc.tdes0.set(0);
        assert!(!desc.has_error());

        // With error summary and specific error bits
        desc.tdes0.set(TDES0_ERR_SUMMARY | TDES0_UNDERFLOW_ERR);
        assert!(desc.has_error());

        // Can check error flags directly
        let errors = desc.error_flags();
        assert!(errors & TDES0_UNDERFLOW_ERR != 0);

        // Multiple errors with summary
        desc.tdes0
            .set(TDES0_ERR_SUMMARY | TDES0_LATE_COLLISION | TDES0_UNDERFLOW_ERR);
        assert!(desc.has_error());
        let errors = desc.error_flags();
        assert!(errors & TDES0_LATE_COLLISION != 0);
        assert!(errors & TDES0_UNDERFLOW_ERR != 0);
    }

    // =========================================================================
    // Reset Tests
    // =========================================================================

    #[test]
    fn tx_descriptor_reset() {
        let desc = TxDescriptor::new();

        // Set up state
        desc.prepare_and_submit(1000, true, true);
        desc.set_checksum_mode(checksum_mode::FULL);

        // Store next descriptor address
        let next_addr = 0x1234_5678u32;
        desc.buffer2_next_desc.set(next_addr);

        // Reset
        desc.reset();

        // Should not be owned
        assert!(!desc.is_owned());
        // Frame length should be 0 (check via raw_tdes1)
        let len = desc.raw_tdes1() & 0x1FFF;
        assert_eq!(len, 0);
        // Next descriptor should be preserved
        assert_eq!(desc.next_desc_addr(), next_addr);
        // Chain flag should still be set
        assert!(desc.raw_tdes0() & TDES0_SECOND_ADDR_CHAINED != 0);
    }
}
