//! RX DMA descriptor for frame reception.

use super::VolatileCell;
use crate::internal::descriptor_bits::{rdes0, rdes1, rdes4};

/// RX DMA descriptor (32 bytes on ESP32, 64 bytes on ESP32-P4).
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

#[allow(dead_code)]
impl RxDescriptor {
    /// Size of the descriptor in bytes
    #[cfg(not(feature = "esp32p4"))]
    pub const SIZE: usize = 32;

    /// Size of the descriptor in bytes (ESP32-P4 with cache alignment)
    #[cfg(feature = "esp32p4")]
    pub const SIZE: usize = 64;

    /// Create a new zeroed descriptor. Call `setup_chained()` before use.
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

    /// Configure descriptor in chained mode and give to DMA.
    pub fn setup_chained(
        &self,
        buffer: *mut u8,
        buffer_size: usize,
        next_desc: *const RxDescriptor,
    ) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(next_desc as u32);
        self.rdes1.set(
            rdes1::SECOND_ADDR_CHAINED | ((buffer_size as u32) & rdes1::BUFFER1_SIZE_MASK),
        );
        // Give ownership to DMA
        self.rdes0.set(rdes0::OWN);
    }

    /// Configure as end of ring (wraps to first descriptor).
    pub fn setup_end_of_ring(
        &self,
        buffer: *mut u8,
        buffer_size: usize,
        first_desc: *const RxDescriptor,
    ) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(first_desc as u32);
        self.rdes1.set(
            rdes1::SECOND_ADDR_CHAINED
                | rdes1::RX_END_OF_RING
                | ((buffer_size as u32) & rdes1::BUFFER1_SIZE_MASK),
        );
        // Give ownership to DMA
        self.rdes0.set(rdes0::OWN);
    }

    /// Returns true if DMA owns this descriptor.
    #[inline(always)]
    #[must_use]
    pub fn is_owned(&self) -> bool {
        (self.rdes0.get() & rdes0::OWN) != 0
    }

    /// Give ownership to DMA.
    #[inline(always)]
    pub fn set_owned(&self) {
        self.rdes0.set(rdes0::OWN);
    }

    /// Take ownership from DMA.
    #[inline(always)]
    pub fn clear_owned(&self) {
        self.rdes0.update(|v| v & !rdes0::OWN);
    }

    /// First descriptor of a frame.
    #[inline(always)]
    #[must_use]
    pub fn is_first(&self) -> bool {
        (self.rdes0.get() & rdes0::FIRST_DESC) != 0
    }

    /// Last descriptor of a frame.
    #[inline(always)]
    #[must_use]
    pub fn is_last(&self) -> bool {
        (self.rdes0.get() & rdes0::LAST_DESC) != 0
    }

    /// Complete frame in single descriptor (first and last).
    #[inline(always)]
    #[must_use]
    pub fn is_complete_frame(&self) -> bool {
        let status = self.rdes0.get();
        (status & (rdes0::FIRST_DESC | rdes0::LAST_DESC))
            == (rdes0::FIRST_DESC | rdes0::LAST_DESC)
    }

    /// Returns true if error summary bit is set.
    #[inline(always)]
    #[must_use]
    pub fn has_error(&self) -> bool {
        (self.rdes0.get() & rdes0::ERR_SUMMARY) != 0
    }

    /// Raw error flags from RDES0.
    #[inline(always)]
    #[must_use]
    pub fn error_flags(&self) -> u32 {
        self.rdes0.get() & rdes0::ALL_ERRORS
    }

    /// Frame length including CRC (valid on last descriptor).
    #[inline(always)]
    #[must_use]
    pub fn frame_length(&self) -> usize {
        ((self.rdes0.get() & rdes0::FRAME_LEN_MASK) >> rdes0::FRAME_LEN_SHIFT) as usize
    }

    /// Frame length excluding 4-byte CRC.
    #[inline(always)]
    #[must_use]
    pub fn payload_length(&self) -> usize {
        self.frame_length().saturating_sub(4)
    }

    /// Frame has VLAN tag.
    #[inline(always)]
    #[must_use]
    pub fn has_vlan_tag(&self) -> bool {
        (self.rdes0.get() & rdes0::VLAN_TAG) != 0
    }

    /// Ethernet type frame (vs 802.3 length frame).
    #[inline(always)]
    #[must_use]
    pub fn is_ethernet_frame(&self) -> bool {
        (self.rdes0.get() & rdes0::FRAME_TYPE) != 0
    }

    /// Timestamp available in RDES6/7.
    #[inline(always)]
    #[must_use]
    pub fn has_timestamp(&self) -> bool {
        (self.rdes0.get() & rdes0::TIMESTAMP_AVAIL) != 0
    }

    /// Timestamp low 32 bits.
    #[inline(always)]
    #[must_use]
    pub fn timestamp_low(&self) -> u32 {
        self.timestamp_low.get()
    }

    /// Timestamp high 32 bits.
    #[inline(always)]
    #[must_use]
    pub fn timestamp_high(&self) -> u32 {
        self.timestamp_high.get()
    }

    /// Combined 64-bit timestamp.
    #[inline(always)]
    #[must_use]
    pub fn timestamp(&self) -> u64 {
        ((self.timestamp_high.get() as u64) << 32) | (self.timestamp_low.get() as u64)
    }

    /// Raw extended status (RDES4).
    #[inline(always)]
    #[must_use]
    pub fn extended_status(&self) -> u32 {
        self.extended_status.get()
    }

    /// Extended status available.
    #[inline(always)]
    #[must_use]
    pub fn has_extended_status(&self) -> bool {
        (self.rdes0.get() & rdes0::EXT_STATUS) != 0
    }

    /// IPv4 packet (from extended status).
    #[inline(always)]
    #[must_use]
    pub fn is_ipv4(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IPV4_PKT) != 0
    }

    /// IPv6 packet (from extended status).
    #[inline(always)]
    #[must_use]
    pub fn is_ipv6(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IPV6_PKT) != 0
    }

    /// IP header checksum error.
    #[inline(always)]
    #[must_use]
    pub fn has_ip_header_error(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IP_HEADER_ERR) != 0
    }

    /// IP payload checksum error.
    #[inline(always)]
    #[must_use]
    pub fn has_ip_payload_error(&self) -> bool {
        self.has_extended_status() && (self.extended_status.get() & rdes4::IP_PAYLOAD_ERR) != 0
    }

    /// Buffer address (RDES2).
    #[inline(always)]
    #[must_use]
    pub fn buffer_addr(&self) -> u32 {
        self.buffer1_addr.get()
    }

    /// Next descriptor address (RDES3, chained mode).
    #[inline(always)]
    #[must_use]
    pub fn next_desc_addr(&self) -> u32 {
        self.buffer2_next_desc.get()
    }

    /// Configured buffer size.
    #[inline(always)]
    #[must_use]
    pub fn buffer_size(&self) -> usize {
        (self.rdes1.get() & rdes1::BUFFER1_SIZE_MASK) as usize
    }

    /// Clear status and return to DMA.
    pub fn recycle(&self) {
        self.rdes0.set(rdes0::OWN);
    }

    /// Raw RDES0 (for debugging).
    #[inline(always)]
    #[must_use]
    pub fn raw_rdes0(&self) -> u32 {
        self.rdes0.get()
    }

    /// Raw RDES1 (for debugging).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::descriptor_bits::{rdes0, rdes1};

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
        assert_eq!(raw & rdes0::OWN, rdes0::OWN, "OWN bit should be bit 31");
        assert_eq!(raw & !rdes0::OWN, 0, "No other bits should be set");
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
        desc.rdes0.set(rdes0::FIRST_DESC);
        assert!(desc.is_first());
        assert!(!desc.is_last());

        // Set last descriptor flag
        desc.rdes0.set(rdes0::LAST_DESC);
        assert!(!desc.is_first());
        assert!(desc.is_last());

        // Set both (complete frame in single descriptor)
        desc.rdes0.set(rdes0::FIRST_DESC | rdes0::LAST_DESC);
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
        desc.rdes0.set(test_length << rdes0::FRAME_LEN_SHIFT);
        assert_eq!(desc.frame_length(), 1500);

        // Test with other values
        desc.rdes0.set(64 << rdes0::FRAME_LEN_SHIFT);
        assert_eq!(desc.frame_length(), 64);

        desc.rdes0.set(1518 << rdes0::FRAME_LEN_SHIFT);
        assert_eq!(desc.frame_length(), 1518);
    }

    #[test]
    fn rx_descriptor_frame_length_with_other_bits() {
        let desc = RxDescriptor::new();

        // Frame length with OWN bit and first/last flags
        let length: u32 = 256;
        desc.rdes0.set(
            rdes0::OWN | rdes0::FIRST_DESC | rdes0::LAST_DESC | (length << rdes0::FRAME_LEN_SHIFT),
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
        desc.rdes0.set(rdes0::ERR_SUMMARY);
        assert!(desc.has_error());

        // Clear and verify
        desc.rdes0.set(0);
        assert!(!desc.has_error());

        // Multiple error bits including summary
        desc.rdes0
            .set(rdes0::ERR_SUMMARY | rdes0::CRC_ERR | rdes0::OVERFLOW_ERR);
        assert!(desc.has_error());

        // Can check error flags directly
        let errors = desc.error_flags();
        assert!(errors & rdes0::CRC_ERR != 0);
        assert!(errors & rdes0::OVERFLOW_ERR != 0);
    }

    // =========================================================================
    // Buffer Configuration Tests
    // =========================================================================

    #[test]
    fn rx_descriptor_buffer_size() {
        let desc = RxDescriptor::new();

        // Set buffer size in RDES1
        desc.rdes1.set(1600 & rdes1::BUFFER1_SIZE_MASK);
        assert_eq!(desc.buffer_size(), 1600);

        desc.rdes1.set(512 & rdes1::BUFFER1_SIZE_MASK);
        assert_eq!(desc.buffer_size(), 512);
    }

    #[test]
    fn rx_descriptor_recycle() {
        let desc = RxDescriptor::new();

        // Set up some state
        desc.rdes0
            .set(rdes0::FIRST_DESC | rdes0::LAST_DESC | (100 << rdes0::FRAME_LEN_SHIFT));
        desc.rdes1.set(1600);

        // Recycle should reset status and give to DMA
        desc.recycle();

        assert!(desc.is_owned());
        // Buffer size should be preserved in rdes1
        assert_eq!(desc.buffer_size(), 1600);
    }
}
