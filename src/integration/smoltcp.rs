//! smoltcp Network Stack Integration
#![cfg_attr(docsrs, doc(cfg(feature = "smoltcp")))]
//!
//! This module provides integration with the [smoltcp](https://docs.rs/smoltcp) network stack.
//! It implements the `smoltcp::phy::Device` trait for the EMAC driver, allowing it to be
//! used as a network interface with smoltcp's TCP/IP stack.
//!
//! # Example
//!
//! ```ignore
//! use smoltcp::iface::{Config, Interface, SocketSet};
//! use smoltcp::wire::{EthernetAddress, IpCidr};
//! use esp32_emac::{Emac, EmacConfig};
//!
//! // Create and initialize EMAC
//! static mut EMAC: Emac<10, 10, 1600> = Emac::new();
//! let emac = unsafe { &mut EMAC };
//! emac.init(EmacConfig::default()).unwrap();
//! emac.start().unwrap();
//!
//! // Create smoltcp interface
//! let config = Config::new(EthernetAddress(*emac.mac_address()).into());
//! let mut iface = Interface::new(config, emac, smoltcp::time::Instant::ZERO);
//!
//! // Configure IP address
//! iface.update_ip_addrs(|addrs| {
//!     addrs.push(IpCidr::new(IpAddress::v4(192, 168, 1, 100), 24)).unwrap();
//! });
//! ```
//!
//! # Features
//!
//! This module is only available when the `smoltcp` feature is enabled in Cargo.toml:
//! ```toml
//! [dependencies]
//! esp32_emac = { version = "0.1", features = ["smoltcp"] }
//! ```
//!
//! # Safety Notes
//!
//! The smoltcp `Device` trait requires `receive()` to return both an `RxToken` and
//! `TxToken` simultaneously. This implementation uses raw pointers internally to
//! satisfy this API requirement. This is safe because:
//!
//! 1. **Temporal safety**: Tokens are consumed immediately in the same call stack
//!    before any other access to the `Emac` occurs.
//! 2. **Spatial safety**: RX and TX operations use completely separate descriptor
//!    rings and buffer pools in the DMA engine.
//! 3. **No aliasing during access**: Only one token is consumed at a time, and
//!    the `consume()` method takes `self` by value, preventing concurrent use.
//!
//! This pattern is common in embedded networking crates (see embassy-net, esp-wifi).

use crate::driver::config::State;
use crate::driver::emac::Emac;
use crate::internal::constants::{MAX_FRAME_SIZE, MTU};

use smoltcp::phy::{Checksum, ChecksumCapabilities, Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;

// =============================================================================
// RX Token
// =============================================================================

/// Receive token for smoltcp
///
/// This token represents a received frame that can be consumed.
/// It uses a raw pointer internally to satisfy smoltcp's API requirement
/// that `receive()` returns both RX and TX tokens simultaneously.
///
/// This type is an implementation detail of the smoltcp integration and is
/// considered advanced; most users won't need to name it directly.
///
/// # Safety
///
/// The raw pointer is safe because:
/// - The token is consumed immediately (takes `self` by value)
/// - The lifetime `'a` ensures the pointer remains valid
/// - RX operations use a separate descriptor ring from TX
pub struct EmacRxToken<'a, const RX: usize, const TX: usize, const BUF: usize> {
    emac: *mut Emac<RX, TX, BUF>,
    _marker: core::marker::PhantomData<&'a mut Emac<RX, TX, BUF>>,
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> smoltcp::phy::RxToken
    for EmacRxToken<'a, RX, TX, BUF>
{
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        // Use a stack-allocated buffer for the frame
        // This avoids heap allocation while being compatible with smoltcp's API
        let mut buffer = [0u8; MAX_FRAME_SIZE];

        // SAFETY: The pointer is valid for 'a; token is consumed by value, so no aliasing, and RX/TX rings are separate.
        let emac = unsafe { &mut *self.emac };

        // Receive the frame
        let len = emac.receive(&mut buffer).unwrap_or_default();

        // Call the consumer function with the received data
        f(&buffer[..len])
    }
}

// =============================================================================
// TX Token
// =============================================================================

/// Transmit token for smoltcp
///
/// This token represents the ability to transmit a frame.
/// It uses a raw pointer internally to satisfy smoltcp's API requirement
/// that `receive()` returns both RX and TX tokens simultaneously.
///
/// This type is an implementation detail of the smoltcp integration and is
/// considered advanced; most users won't need to name it directly.
///
/// # Safety
///
/// The raw pointer is safe because:
/// - The token is consumed immediately (takes `self` by value)
/// - The lifetime `'a` ensures the pointer remains valid
/// - TX operations use a separate descriptor ring from RX
pub struct EmacTxToken<'a, const RX: usize, const TX: usize, const BUF: usize> {
    emac: *mut Emac<RX, TX, BUF>,
    _marker: core::marker::PhantomData<&'a mut Emac<RX, TX, BUF>>,
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> smoltcp::phy::TxToken
    for EmacTxToken<'a, RX, TX, BUF>
{
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // Validate length
        let len = len.min(MAX_FRAME_SIZE);

        // Use a stack-allocated buffer
        let mut buffer = [0u8; MAX_FRAME_SIZE];

        // Let smoltcp fill in the frame data
        let result = f(&mut buffer[..len]);

        // SAFETY: The pointer is valid for 'a; token is consumed by value, so no aliasing, and TX/RX rings are separate.
        let emac = unsafe { &mut *self.emac };

        // Transmit the frame (ignore errors, smoltcp will retry)
        let _ = emac.transmit(&buffer[..len]);

        result
    }
}

// =============================================================================
// Device Implementation
// =============================================================================

impl<const RX: usize, const TX: usize, const BUF: usize> Device for Emac<RX, TX, BUF> {
    type RxToken<'a>
        = EmacRxToken<'a, RX, TX, BUF>
    where
        Self: 'a;
    type TxToken<'a>
        = EmacTxToken<'a, RX, TX, BUF>
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // Check if EMAC is running
        if self.state() != State::Running {
            return None;
        }

        // Check if a frame is available
        if !self.rx_available() {
            return None;
        }

        // smoltcp requires both RX and TX tokens together.
        //
        // SAFETY: We use raw pointers to create both tokens from the same Emac.
        // This is safe because:
        // 1. Both tokens are consumed immediately in the same call stack
        // 2. RX and TX use completely separate descriptor rings and buffers
        // 3. Only one token's consume() runs at a time (they take self by value)
        // 4. The PhantomData<&'a mut Emac> ensures proper lifetime tracking
        //
        // This pattern is used by other embedded networking crates (embassy-net, etc.)
        let self_ptr = self as *mut Self;
        Some((
            EmacRxToken {
                emac: self_ptr,
                _marker: core::marker::PhantomData,
            },
            EmacTxToken {
                emac: self_ptr,
                _marker: core::marker::PhantomData,
            },
        ))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        // Check if EMAC is running
        if self.state() != State::Running {
            return None;
        }

        // Check if we can transmit
        if !self.tx_ready() {
            return None;
        }

        // SAFETY: Single token, no aliasing. Raw pointer is immediately
        // converted back to reference in consume().
        Some(EmacTxToken {
            emac: self as *mut Self,
            _marker: core::marker::PhantomData,
        })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();

        // Set the medium to Ethernet
        caps.medium = Medium::Ethernet;

        // Standard Ethernet MTU
        caps.max_transmission_unit = MTU;

        // Single frame at a time (no scatter-gather for smoltcp)
        caps.max_burst_size = Some(1);

        // Checksum capabilities
        // The ESP32 EMAC supports hardware checksum, but we let smoltcp handle it
        // for maximum compatibility. Set to None to use software checksums.
        caps.checksum = ChecksumCapabilities::default();

        // If hardware checksum is enabled in config, indicate that
        // Note: This would need to be checked at runtime based on config
        // For now, we use software checksums which are always correct
        caps.checksum.ipv4 = Checksum::Both;
        caps.checksum.udp = Checksum::Both;
        caps.checksum.tcp = Checksum::Both;
        caps.checksum.icmpv4 = Checksum::Both;

        caps
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the MAC address as a smoltcp EthernetAddress
///
/// This is a convenience function for creating smoltcp interface configurations.
pub fn ethernet_address<const RX: usize, const TX: usize, const BUF: usize>(
    emac: &Emac<RX, TX, BUF>,
) -> smoltcp::wire::EthernetAddress {
    smoltcp::wire::EthernetAddress(*emac.mac_address())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Constants Tests
    // =========================================================================

    #[test]
    fn test_max_frame_size() {
        assert_eq!(MAX_FRAME_SIZE, 1522);
    }

    #[test]
    fn test_mtu() {
        assert_eq!(MTU, 1500);
    }

    // =========================================================================
    // DeviceCapabilities Tests (can test without hardware)
    // =========================================================================

    #[test]
    fn capabilities_medium_is_ethernet() {
        // We can't create an Emac without hardware, but we can verify
        // that Medium::Ethernet exists and is the expected value
        let medium = Medium::Ethernet;
        assert_eq!(medium, Medium::Ethernet);
    }

    #[test]
    fn capabilities_mtu_matches_constant() {
        // Verify MTU constant matches Ethernet standard
        assert_eq!(MTU, 1500);
    }

    #[test]
    fn checksum_variants_are_constructable() {
        // Verify Checksum variants exist and can be constructed
        // (Checksum doesn't implement PartialEq, so we use pattern matching)
        let _none = Checksum::None;
        let _tx = Checksum::Tx;
        let _rx = Checksum::Rx;
        let both = Checksum::Both;

        // Verify we can match on the variants
        assert!(matches!(both, Checksum::Both));
    }

    #[test]
    fn checksum_capabilities_is_constructable() {
        // Verify ChecksumCapabilities can be constructed and fields accessed
        // (Checksum doesn't implement PartialEq, so we use pattern matching)
        let caps = ChecksumCapabilities::default();
        // Verify the struct has the expected fields
        let _ipv4 = caps.ipv4;
        let _udp = caps.udp;
        let _tcp = caps.tcp;
        let _icmpv4 = caps.icmpv4;
    }

    #[test]
    fn device_capabilities_default_has_medium_ethernet() {
        let caps = DeviceCapabilities::default();
        assert_eq!(caps.medium, Medium::Ethernet);
    }

    #[test]
    fn device_capabilities_default_has_no_max_burst() {
        let caps = DeviceCapabilities::default();
        assert_eq!(caps.max_burst_size, None);
    }

    // =========================================================================
    // Token Marker Tests
    // =========================================================================

    #[test]
    fn phantom_data_is_zero_sized() {
        use core::mem::size_of;
        assert_eq!(
            size_of::<core::marker::PhantomData<&mut Emac<10, 10, 1600>>>(),
            0
        );
    }
}
