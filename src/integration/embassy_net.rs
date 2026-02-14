//! Embassy network driver integration.
#![cfg_attr(docsrs, doc(cfg(feature = "embassy-net")))]
//!
//! This module provides an `embassy-net-driver` implementation for the EMAC.
//! It follows Embassy guidance by depending only on `embassy-net-driver` and
//! exposing a small wrapper that integrates with `embassy-net` stacks.
//!
//! # Usage
//!
//! ```ignore
//! use embassy_net::{Config, Stack, StackResources};
//! use embassy_net_driver::LinkState;
//! use ph_esp32_mac::{EmbassyEmac, EmbassyEmacState, Emac, EmacConfig};
//! use static_cell::StaticCell;
//!
//! static mut EMAC: Emac<10, 10, 1600> = Emac::new();
//! static EMAC_STATE: EmbassyEmacState = EmbassyEmacState::new(LinkState::Down);
//! static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
//!
//! // Initialize EMAC first (GPIO/clock/PHY setup omitted here).
//! let emac = unsafe { &mut EMAC };
//! emac.init(EmacConfig::default(), /* delay */).unwrap();
//! emac.start().unwrap();
//!
//! // Create Embassy driver wrapper.
//! let driver = EmbassyEmac::new(emac, &EMAC_STATE);
//! let config = Config::dhcpv4(Default::default());
//! let seed = 0x1234_5678_9ABC_DEF0;
//! let (stack, runner) = embassy_net::new(
//!     driver,
//!     config,
//!     RESOURCES.init(StackResources::new()),
//!     seed,
//! );
//!
//! // In your EMAC interrupt handler:
//! // EMAC_STATE.handle_interrupt();
//! ```
//!
//! # Macro Helpers
//!
//! When using `embassy-net` with esp-hal, you can reduce boilerplate:
//!
//! ```ignore
//! ph_esp32_mac::embassy_net_statics!(EMAC, EMAC_STATE, RESOURCES, 10, 10, 1600, 4);
//!
//! // After EMAC init:
//! let driver = ph_esp32_mac::embassy_net_driver!(emac_ptr, &EMAC_STATE);
//! let (stack, runner) =
//!     ph_esp32_mac::embassy_net_stack!(driver, RESOURCES, Config::default(), seed);
//! ```
//!
//! # Interrupt Handling
//!
//! Call [`EmbassyEmacState::handle_interrupt`] from the EMAC ISR to wake tasks.
//! This clears DMA interrupt flags and wakes RX/TX/link wakers as needed.
//!
//! # Link State Updates
//!
//! Use [`EmbassyEmacState::update_link_from_phy`] in a periodic task to keep the
//! network stack informed of link changes. This method polls the PHY and updates
//! the cached [`LinkState`], waking the stack on transitions.
//!
//! # esp-hal + Embassy Runtime
//!
//! With `esp-hal` 1.0.0, the recommended Embassy runtime integration is via
//! `esp-rtos` with its `embassy` feature enabled. Ensure the time driver is
//! started before running the executor (see the example in `apps/examples/embassy_net.rs`).

use core::{marker::PhantomData, task::Context};

use embassy_net_driver::{
    Capabilities, ChecksumCapabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken,
};

use crate::driver::error::Result;
use crate::hal::mdio::MdioBus;
use crate::internal::constants::{MAX_FRAME_SIZE, MTU};
use crate::internal::register::dma::DmaRegs;
use crate::phy::{LinkStatus, PhyDriver};
use crate::sync::primitives::{AtomicWaker, CriticalSectionCell};
use crate::{Emac, InterruptStatus};

// =============================================================================
// Embassy Driver State
// =============================================================================

/// Shared Embassy driver state for EMAC wakers and link status.
///
/// Store this in a `static` so it can be accessed from interrupts.
pub struct EmbassyEmacState {
    rx_waker: AtomicWaker,
    tx_waker: AtomicWaker,
    link_waker: AtomicWaker,
    link_state: CriticalSectionCell<LinkState>,
}

impl EmbassyEmacState {
    /// Create a new Embassy EMAC state.
    ///
    /// # Arguments
    ///
    /// * `initial_link` - Initial link state reported to the stack
    pub const fn new(initial_link: LinkState) -> Self {
        Self {
            rx_waker: AtomicWaker::new(),
            tx_waker: AtomicWaker::new(),
            link_waker: AtomicWaker::new(),
            link_state: CriticalSectionCell::new(initial_link),
        }
    }

    /// Get the cached link state.
    pub fn link_state(&self) -> LinkState {
        self.link_state.with_ref(|state| *state)
    }

    /// Update the cached link state and wake any waiters.
    pub fn set_link_state(&self, state: LinkState) {
        self.link_state.with(|current| {
            *current = state;
        });
        self.link_waker.wake();
    }

    /// Poll the PHY and update the cached link state.
    ///
    /// This is intended for periodic link-state polling in async tasks.
    /// It updates the internal [`LinkState`] cache and wakes the stack
    /// when a transition occurs.
    ///
    /// # Arguments
    ///
    /// * `phy` - PHY driver instance
    /// * `mdio` - MDIO bus implementation
    ///
    /// # Returns
    ///
    /// The current link status (speed/duplex) if link is up.
    ///
    /// # Errors
    ///
    /// Propagates PHY/MDIO errors from the underlying driver.
    pub fn update_link_from_phy<M: MdioBus, P: PhyDriver>(
        &self,
        phy: &mut P,
        mdio: &mut M,
    ) -> Result<Option<LinkStatus>> {
        let status = phy.link_status(mdio)?;
        if status.is_some() {
            self.set_link_state(LinkState::Up);
        } else {
            self.set_link_state(LinkState::Down);
        }
        Ok(status)
    }

    /// Wake RX/TX tasks based on an interrupt status snapshot.
    pub fn on_interrupt(&self, status: InterruptStatus) {
        if status.rx_complete || status.rx_buf_unavailable {
            self.rx_waker.wake();
        }

        if status.tx_complete || status.tx_buf_unavailable {
            self.tx_waker.wake();
        }

        if status.has_error() {
            self.rx_waker.wake();
            self.tx_waker.wake();
        }
    }

    /// Handle the EMAC interrupt and wake Embassy tasks.
    ///
    /// This reads the DMA interrupt status, wakes any waiting tasks, and
    /// clears the interrupt flags.
    pub fn handle_interrupt(&self) {
        let status = InterruptStatus::from_raw(DmaRegs::status());
        self.on_interrupt(status);
        DmaRegs::set_status(status.to_raw());
    }
}

// =============================================================================
// Embassy Driver Wrapper
// =============================================================================

/// Embassy-net driver wrapper for EMAC.
///
/// This type implements [`embassy_net_driver::Driver`] and provides RX/TX tokens.
pub struct EmbassyEmac<'a, const RX: usize, const TX: usize, const BUF: usize> {
    emac: *mut Emac<RX, TX, BUF>,
    state: &'a EmbassyEmacState,
    _marker: PhantomData<&'a mut Emac<RX, TX, BUF>>,
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> EmbassyEmac<'a, RX, TX, BUF> {
    /// Create a new Embassy driver wrapper.
    ///
    /// # Arguments
    ///
    /// * `emac` - Initialized EMAC instance (placed in final memory location)
    /// * `state` - Shared Embassy driver state (static recommended)
    pub fn new(emac: &'a mut Emac<RX, TX, BUF>, state: &'a EmbassyEmacState) -> Self {
        Self {
            emac: emac as *mut Emac<RX, TX, BUF>,
            state,
            _marker: PhantomData,
        }
    }

    /// Get the shared Embassy state.
    pub fn state(&self) -> &EmbassyEmacState {
        self.state
    }
}

// =============================================================================
// RX/TX Tokens
// =============================================================================

/// Embassy RX token for EMAC.
///
/// This is an implementation detail of the embassy-net driver. Most users
/// should not need to reference it directly.
pub struct EmbassyRxToken<'a, const RX: usize, const TX: usize, const BUF: usize> {
    emac: *mut Emac<RX, TX, BUF>,
    _marker: PhantomData<&'a mut Emac<RX, TX, BUF>>,
}

impl<const RX: usize, const TX: usize, const BUF: usize> RxToken
    for EmbassyRxToken<'_, RX, TX, BUF>
{
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = [0u8; MAX_FRAME_SIZE];

        // SAFETY: The raw pointer is valid for the driver lifetime and tokens are consumed immediately by the stack.
        let emac = unsafe { &mut *self.emac };

        let len = emac.receive(&mut buffer).unwrap_or(0);
        f(&mut buffer[..len])
    }
}

/// Embassy TX token for EMAC.
///
/// This is an implementation detail of the embassy-net driver. Most users
/// should not need to reference it directly.
pub struct EmbassyTxToken<'a, const RX: usize, const TX: usize, const BUF: usize> {
    emac: *mut Emac<RX, TX, BUF>,
    _marker: PhantomData<&'a mut Emac<RX, TX, BUF>>,
}

impl<const RX: usize, const TX: usize, const BUF: usize> TxToken
    for EmbassyTxToken<'_, RX, TX, BUF>
{
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let len = len.min(MAX_FRAME_SIZE);
        let mut buffer = [0u8; MAX_FRAME_SIZE];
        let result = f(&mut buffer[..len]);

        // SAFETY: The raw pointer is valid for the driver lifetime and tokens are consumed immediately by the stack.
        let emac = unsafe { &mut *self.emac };

        let _ = emac.transmit(&buffer[..len]);
        result
    }
}

// =============================================================================
// Driver Implementation
// =============================================================================

impl<const RX: usize, const TX: usize, const BUF: usize> Driver for EmbassyEmac<'_, RX, TX, BUF> {
    type RxToken<'a>
        = EmbassyRxToken<'a, RX, TX, BUF>
    where
        Self: 'a;
    type TxToken<'a>
        = EmbassyTxToken<'a, RX, TX, BUF>
    where
        Self: 'a;

    fn receive(&mut self, cx: &mut Context<'_>) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // SAFETY: The raw pointer is valid for the driver lifetime.
        let emac = unsafe { &mut *self.emac };

        if !emac.rx_available() {
            self.state.rx_waker.register(cx.waker());
            if !emac.rx_available() {
                return None;
            }
        }

        Some((
            EmbassyRxToken {
                emac: self.emac,
                _marker: PhantomData,
            },
            EmbassyTxToken {
                emac: self.emac,
                _marker: PhantomData,
            },
        ))
    }

    fn transmit(&mut self, cx: &mut Context<'_>) -> Option<Self::TxToken<'_>> {
        // SAFETY: The raw pointer is valid for the driver lifetime.
        let emac = unsafe { &mut *self.emac };

        if !emac.tx_ready() {
            self.state.tx_waker.register(cx.waker());
            if !emac.tx_ready() {
                return None;
            }
        }

        Some(EmbassyTxToken {
            emac: self.emac,
            _marker: PhantomData,
        })
    }

    fn link_state(&mut self, cx: &mut Context<'_>) -> LinkState {
        self.state.link_waker.register(cx.waker());
        self.state.link_state()
    }

    fn capabilities(&self) -> Capabilities {
        let mut caps = Capabilities::default();
        caps.max_transmission_unit = MTU;
        caps.max_burst_size = Some(1);
        caps.checksum = ChecksumCapabilities::default();
        caps
    }

    fn hardware_address(&self) -> HardwareAddress {
        // SAFETY: The raw pointer is valid for the driver lifetime.
        let emac = unsafe { &*self.emac };
        HardwareAddress::Ethernet(*emac.mac_address())
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::std_instead_of_core, clippy::std_instead_of_alloc)]
mod tests {
    extern crate std;

    use super::*;
    use core::task::{RawWaker, RawWakerVTable, Waker};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // =========================================================================
    // Test Waker Helper
    // =========================================================================

    struct WakeCounter {
        count: AtomicUsize,
    }

    impl WakeCounter {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                count: AtomicUsize::new(0),
            })
        }

        fn count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    fn test_waker(counter: Arc<WakeCounter>) -> Waker {
        fn clone_fn(ptr: *const ()) -> RawWaker {
            // SAFETY: ptr was created from Arc::into_raw and is valid.
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            let cloned = arc.clone();
            core::mem::forget(arc);
            RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
        }

        fn wake_fn(ptr: *const ()) {
            // SAFETY: ptr was created from Arc::into_raw and is valid.
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref_fn(ptr: *const ()) {
            // SAFETY: ptr was created from Arc::into_raw and is valid.
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
            core::mem::forget(arc);
        }

        fn drop_fn(ptr: *const ()) {
            // SAFETY: ptr was created from Arc::into_raw and is valid.
            unsafe {
                Arc::from_raw(ptr as *const WakeCounter);
            }
        }

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(clone_fn, wake_fn, wake_by_ref_fn, drop_fn);

        let raw = RawWaker::new(Arc::into_raw(counter) as *const (), &VTABLE);
        // SAFETY: raw waker was correctly constructed with a valid vtable.
        unsafe { Waker::from_raw(raw) }
    }

    // =========================================================================
    // EmbassyEmacState Tests
    // =========================================================================

    #[test]
    fn state_initial_link_down() {
        let state = EmbassyEmacState::new(LinkState::Down);
        assert!(matches!(state.link_state(), LinkState::Down));
    }

    #[test]
    fn state_initial_link_up() {
        let state = EmbassyEmacState::new(LinkState::Up);
        assert!(matches!(state.link_state(), LinkState::Up));
    }

    #[test]
    fn state_set_link_state_updates() {
        let state = EmbassyEmacState::new(LinkState::Down);
        state.set_link_state(LinkState::Up);
        assert!(matches!(state.link_state(), LinkState::Up));
    }

    #[test]
    fn state_set_link_state_wakes_link_waker() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let counter = WakeCounter::new();
        state.link_waker.register(&test_waker(counter.clone()));

        state.set_link_state(LinkState::Up);

        assert_eq!(counter.count(), 1);
    }

    #[test]
    fn on_interrupt_rx_complete_wakes_rx() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();

        state.rx_waker.register(&test_waker(rx_counter.clone()));
        state.tx_waker.register(&test_waker(tx_counter.clone()));

        let status = InterruptStatus {
            rx_complete: true,
            ..InterruptStatus::default()
        };
        state.on_interrupt(status);

        assert_eq!(rx_counter.count(), 1);
        assert_eq!(tx_counter.count(), 0);
    }

    #[test]
    fn on_interrupt_tx_complete_wakes_tx() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();

        state.rx_waker.register(&test_waker(rx_counter.clone()));
        state.tx_waker.register(&test_waker(tx_counter.clone()));

        let status = InterruptStatus {
            tx_complete: true,
            ..InterruptStatus::default()
        };
        state.on_interrupt(status);

        assert_eq!(rx_counter.count(), 0);
        assert_eq!(tx_counter.count(), 1);
    }

    #[test]
    fn on_interrupt_rx_buf_unavailable_wakes_rx() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let rx_counter = WakeCounter::new();

        state.rx_waker.register(&test_waker(rx_counter.clone()));

        let status = InterruptStatus {
            rx_buf_unavailable: true,
            ..InterruptStatus::default()
        };
        state.on_interrupt(status);

        assert_eq!(rx_counter.count(), 1);
    }

    #[test]
    fn on_interrupt_tx_buf_unavailable_wakes_tx() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let tx_counter = WakeCounter::new();

        state.tx_waker.register(&test_waker(tx_counter.clone()));

        let status = InterruptStatus {
            tx_buf_unavailable: true,
            ..InterruptStatus::default()
        };
        state.on_interrupt(status);

        assert_eq!(tx_counter.count(), 1);
    }

    #[test]
    fn on_interrupt_error_wakes_both_rx_and_tx() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();

        state.rx_waker.register(&test_waker(rx_counter.clone()));
        state.tx_waker.register(&test_waker(tx_counter.clone()));

        let status = InterruptStatus {
            fatal_bus_error: true,
            ..InterruptStatus::default()
        };
        state.on_interrupt(status);

        assert_eq!(rx_counter.count(), 1);
        assert_eq!(tx_counter.count(), 1);
    }

    #[test]
    fn on_interrupt_no_flags_wakes_nothing() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();

        state.rx_waker.register(&test_waker(rx_counter.clone()));
        state.tx_waker.register(&test_waker(tx_counter.clone()));

        let status = InterruptStatus::default();
        state.on_interrupt(status);

        assert_eq!(rx_counter.count(), 0);
        assert_eq!(tx_counter.count(), 0);
    }

    #[test]
    fn on_interrupt_combined_flags_wake_correctly() {
        let state = EmbassyEmacState::new(LinkState::Down);
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();

        state.rx_waker.register(&test_waker(rx_counter.clone()));
        state.tx_waker.register(&test_waker(tx_counter.clone()));

        let status = InterruptStatus {
            rx_complete: true,
            tx_complete: true,
            ..InterruptStatus::default()
        };
        state.on_interrupt(status);

        assert_eq!(rx_counter.count(), 1);
        assert_eq!(tx_counter.count(), 1);
    }

    #[test]
    fn capabilities_defaults() {
        let mut caps = Capabilities::default();
        caps.max_transmission_unit = MTU;
        caps.max_burst_size = Some(1);
        caps.checksum = ChecksumCapabilities::default();

        assert_eq!(caps.max_transmission_unit, 1500);
        assert_eq!(caps.max_burst_size, Some(1));
    }

    #[test]
    fn update_link_from_phy_reports_up() {
        use crate::testing::MockMdioBus;

        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(1);
        mdio.simulate_link_up_100_fd(1);
        // Set LAN8720A vendor-specific PSCSR (reg 31) with AUTODONE and 100FD speed
        mdio.set_register(1, 31, (1 << 12) | (0x6 << 2));

        let mut phy = crate::phy::Lan8720a::new(1);
        let state = EmbassyEmacState::new(LinkState::Down);

        let result = state.update_link_from_phy(&mut phy, &mut mdio).unwrap();
        assert!(result.is_some());
        assert!(matches!(state.link_state(), LinkState::Up));
    }

    #[test]
    fn update_link_from_phy_reports_down() {
        use crate::testing::MockMdioBus;

        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(1);
        // Link stays down (no simulate_link_up call)

        let mut phy = crate::phy::Lan8720a::new(1);
        let state = EmbassyEmacState::new(LinkState::Up);

        let result = state.update_link_from_phy(&mut phy, &mut mdio).unwrap();
        assert!(result.is_none());
        assert!(matches!(state.link_state(), LinkState::Down));
    }
}
