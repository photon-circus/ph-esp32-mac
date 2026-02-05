//! Embassy network driver integration.
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
//! started before running the executor (see the example in `examples/embassy_net.rs`).

use core::{marker::PhantomData, task::Context};

use embassy_net_driver::{
    Capabilities, ChecksumCapabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken,
};

use crate::driver::error::Result;
use crate::hal::mdio::MdioBus;
use crate::internal::constants::{MAX_FRAME_SIZE, MTU};
use crate::internal::register::dma::DmaRegs;
use crate::phy::{LinkStatus, PhyDriver};
use crate::sync::{AtomicWaker, CriticalSectionCell};
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

        // SAFETY: The raw pointer is valid for the lifetime of the driver,
        // and tokens are consumed immediately by the stack.
        let emac = unsafe { &mut *self.emac };

        let len = emac.receive(&mut buffer).unwrap_or(0);
        f(&mut buffer[..len])
    }
}

/// Embassy TX token for EMAC.
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

        // SAFETY: The raw pointer is valid for the lifetime of the driver,
        // and tokens are consumed immediately by the stack.
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
        // SAFETY: The raw pointer is valid for the lifetime of the driver.
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
        // SAFETY: The raw pointer is valid for the lifetime of the driver.
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
        // SAFETY: The raw pointer is valid for the lifetime of the driver.
        let emac = unsafe { &*self.emac };
        HardwareAddress::Ethernet(*emac.mac_address())
    }
}
