//! Core ESP32 EMAC driver implementation.
//!
//! This module contains the main [`Emac`] structure and core operations:
//!
//! - Initialization and configuration
//! - Start/stop control
//! - Frame transmission and reception
//! - Link configuration (speed, duplex, MAC address)
//! - MDIO/PHY register access
//! - Interrupt handling
//!
//! For filtering capabilities, see the [`filtering`](super::filtering) module.
//! For flow control, see the [`flow`](super::flow) module.

use embedded_hal::delay::DelayNs;

use super::config::{Duplex, EmacConfig, PhyInterface, RmiiClockMode, Speed, State};
use super::error::{ConfigError, IoError, Result};
use super::interrupt::InterruptStatus;
use crate::internal::dma::DmaEngine;
use crate::hal::reset::ResetController;
use crate::internal::constants::{
    CSR_CLOCK_DIV_42, FLUSH_TIMEOUT, MII_BUSY_TIMEOUT, TX_DMA_STATE_MASK, TX_DMA_STATE_SHIFT,
};
use crate::internal::register::dma::{
    DMABUSMODE_AAL, DMABUSMODE_ATDS, DMABUSMODE_FB, DMABUSMODE_PBL_MASK, DMABUSMODE_PBL_SHIFT,
    DMABUSMODE_USP, DMAOPERATION_RSF, DMAOPERATION_TSF, DmaRegs,
};
use crate::internal::register::ext::ExtRegs;
use crate::internal::register::mac::{
    GMACCONFIG_ACS, GMACCONFIG_DM, GMACCONFIG_FES, GMACCONFIG_IPC, GMACCONFIG_JD, GMACCONFIG_PS,
    GMACCONFIG_WD, GMACFF_PM, GMACFF_PR, GMACMIIADDR_CR_MASK, GMACMIIADDR_CR_SHIFT, GMACMIIADDR_GB,
    GMACMIIADDR_GR_SHIFT, GMACMIIADDR_GW, GMACMIIADDR_PA_SHIFT, MacRegs,
};

// =============================================================================
// Helper Types
// =============================================================================

/// Wrapper to use a mutable reference as a DelayNs implementor
struct BorrowedDelay<'a, D: DelayNs>(&'a mut D);

impl<D: DelayNs> DelayNs for BorrowedDelay<'_, D> {
    fn delay_ns(&mut self, ns: u32) {
        self.0.delay_ns(ns);
    }
}

// =============================================================================
// EMAC Driver
// =============================================================================

/// ESP32 EMAC Driver
///
/// This is the main driver struct that provides complete Ethernet MAC functionality.
/// It manages the DMA engine, MAC configuration, and PHY interface.
///
/// # Type Parameters
/// * `RX_BUFS` - Number of receive buffers (typically 10)
/// * `TX_BUFS` - Number of transmit buffers (typically 10)
/// * `BUF_SIZE` - Size of each buffer in bytes (typically 1600)
///
/// # Example
/// ```ignore
/// // Static allocation
/// #[link_section = ".dram1"]
/// static mut EMAC: Emac<10, 10, 1600> = Emac::new();
///
/// let emac = unsafe { &mut EMAC };
/// emac.init(EmacConfig::default()).unwrap();
/// emac.start().unwrap();
/// ```
///
/// # Module Organization
///
/// The EMAC driver is split across several modules for clarity:
/// - Core operations (this module): init, start/stop, tx/rx, link config
/// - [`filtering`](super::filtering): MAC address, hash, and VLAN filtering
/// - [`flow`](super::flow): IEEE 802.3 flow control
pub struct Emac<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    /// DMA engine
    pub(super) dma: DmaEngine<RX_BUFS, TX_BUFS, BUF_SIZE>,
    /// Current configuration
    pub(super) config: EmacConfig,
    /// Current state
    state: State,
    /// MAC address
    mac_addr: [u8; 6],
    /// Current link speed
    speed: Speed,
    /// Current duplex mode
    duplex: Duplex,
    /// Flow control state: peer supports PAUSE frames
    pub(super) peer_pause_ability: bool,
    /// Flow control state: currently applying backpressure
    pub(super) flow_control_active: bool,
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Create a new EMAC instance
    ///
    /// This is a const function suitable for static initialization.
    /// The EMAC is created in the `Uninitialized` state.
    pub const fn new() -> Self {
        Self {
            dma: DmaEngine::new(),
            config: EmacConfig::new(),
            state: State::Uninitialized,
            mac_addr: [0u8; 6],
            speed: Speed::Mbps100,
            duplex: Duplex::Full,
            peer_pause_ability: false,
            flow_control_active: false,
        }
    }

    // =========================================================================
    // State Accessors
    // =========================================================================

    /// Get the current state
    #[inline(always)]
    pub fn state(&self) -> State {
        self.state
    }

    /// Get the current MAC address
    #[inline(always)]
    pub fn mac_address(&self) -> &[u8; 6] {
        &self.mac_addr
    }

    /// Get the current link speed
    #[inline(always)]
    pub fn speed(&self) -> Speed {
        self.speed
    }

    /// Get the current duplex mode
    #[inline(always)]
    pub fn duplex(&self) -> Duplex {
        self.duplex
    }

    // =========================================================================
    // Initialization
    // =========================================================================

    /// Initialize the EMAC with the given configuration
    ///
    /// This performs the full initialization sequence:
    /// 1. Enable peripheral clocks
    /// 2. Configure GPIO pins
    /// 3. Perform software reset
    /// 4. Configure MAC and DMA
    /// 5. Set up descriptor chains
    ///
    /// After initialization, the EMAC is in the `Initialized` state.
    /// Call `start()` to begin receiving and transmitting.
    ///
    /// # Parameters
    /// * `config` - EMAC configuration
    /// * `delay` - Delay provider implementing `embedded_hal::delay::DelayNs`
    ///
    /// # Errors
    /// - `AlreadyInitialized` - EMAC was already initialized
    /// - `ResetFailed` - Software reset did not complete
    pub fn init<D: DelayNs>(&mut self, config: EmacConfig, mut delay: D) -> Result<()> {
        if self.state != State::Uninitialized {
            return Err(ConfigError::AlreadyInitialized.into());
        }

        self.config = config;

        // === STEP 1: Configure GPIO routing BEFORE any EMAC operations ===
        if matches!(self.config.rmii_clock, RmiiClockMode::ExternalInput { .. }) {
            ExtRegs::configure_gpio0_rmii_clock_input();

            #[cfg(feature = "defmt")]
            defmt::info!("GPIO0 configured for external RMII clock input");
        }

        // === STEP 2: Enable DPORT peripheral clock ===
        ExtRegs::enable_peripheral_clock();

        #[cfg(feature = "defmt")]
        defmt::info!("EMAC peripheral clock enabled via DPORT");

        // === STEP 3: Configure PHY interface in extension registers ===
        self.configure_phy_interface_regs();

        // === STEP 4: Enable EMAC extension clocks ===
        ExtRegs::enable_clocks();
        ExtRegs::power_up_ram();

        // === STEP 5: Perform software reset ===
        self.software_reset(&mut delay)?;

        // Configure MAC defaults
        self.configure_mac_defaults();

        // Configure DMA defaults
        self.configure_dma_defaults();

        // Initialize DMA engine (descriptor chains)
        self.dma.init();

        // Set MAC address from configuration
        self.mac_addr = self.config.mac_address;
        MacRegs::set_mac_address(&self.mac_addr);

        self.state = State::Initialized;
        Ok(())
    }

    /// Configure PHY interface extension registers (MII/RMII mode and clock source)
    fn configure_phy_interface_regs(&self) {
        match self.config.phy_interface {
            PhyInterface::Rmii => {
                ExtRegs::set_rmii_mode();

                match self.config.rmii_clock {
                    RmiiClockMode::ExternalInput { .. } => {
                        ExtRegs::set_rmii_clock_external();
                    }
                    RmiiClockMode::InternalOutput { .. } => {
                        ExtRegs::set_rmii_clock_internal();
                    }
                }
            }
            PhyInterface::Mii => {
                ExtRegs::set_mii_mode();
            }
        }
    }

    /// Perform software reset using the HAL ResetController
    fn software_reset<D: DelayNs>(&self, delay: &mut D) -> Result<()> {
        let mut reset_ctrl = ResetController::new(BorrowedDelay(delay));
        reset_ctrl
            .soft_reset()
            .map_err(|_| ConfigError::ResetFailed.into())
    }

    /// Configure MAC defaults
    fn configure_mac_defaults(&self) {
        let mut cfg = 0u32;

        // Port select (must be 1 for MII/RMII)
        cfg |= GMACCONFIG_PS;
        // Speed: 100 Mbps
        cfg |= GMACCONFIG_FES;
        // Full duplex
        cfg |= GMACCONFIG_DM;
        // Automatic pad/CRC stripping
        cfg |= GMACCONFIG_ACS;
        // Disable jabber timer
        cfg |= GMACCONFIG_JD;
        // Disable watchdog
        cfg |= GMACCONFIG_WD;

        // Checksum offload if enabled
        if self.config.checksum.rx_checksum {
            cfg |= GMACCONFIG_IPC;
        }

        MacRegs::set_config(cfg);

        // Configure frame filter
        let mut filter = 0u32;

        if self.config.promiscuous {
            filter |= GMACFF_PR;
        }

        // Pass all multicast (for now)
        filter |= GMACFF_PM;

        MacRegs::set_frame_filter(filter);

        // Clear hash tables
        MacRegs::set_hash_table_high(0);
        MacRegs::set_hash_table_low(0);
    }

    /// Configure DMA defaults
    fn configure_dma_defaults(&self) {
        let pbl = self.config.dma_burst_len.to_pbl();
        let bus_mode = DMABUSMODE_FB  // Fixed burst
            | DMABUSMODE_AAL           // Address-aligned beats
            | DMABUSMODE_USP           // Use separate PBL
            | DMABUSMODE_ATDS          // Alternate descriptor size (8 words)
            | ((pbl << DMABUSMODE_PBL_SHIFT) & DMABUSMODE_PBL_MASK);

        DmaRegs::set_bus_mode(bus_mode);

        let op_mode = DMAOPERATION_TSF  // TX store and forward
            | DMAOPERATION_RSF; // RX store and forward

        DmaRegs::set_operation_mode(op_mode);

        // Disable all interrupts initially
        DmaRegs::disable_all_interrupts();

        // Clear any pending interrupts
        DmaRegs::clear_all_interrupts();
    }

    // =========================================================================
    // Start / Stop
    // =========================================================================

    /// Start the EMAC (enable TX and RX)
    ///
    /// After calling this, the EMAC will begin receiving frames into
    /// the RX buffers and is ready to transmit frames.
    ///
    /// # Errors
    /// - `InvalidState` - EMAC is not initialized
    pub fn start(&mut self) -> Result<()> {
        match self.state {
            State::Initialized | State::Stopped => {}
            State::Running => return Ok(()), // Already running
            State::Uninitialized => return Err(IoError::InvalidState.into()),
        }

        // Reset DMA descriptors
        self.dma.reset();

        // Clear pending interrupts
        DmaRegs::clear_all_interrupts();

        // Enable interrupts
        DmaRegs::enable_default_interrupts();

        // Enable MAC transmitter
        self.mac_tx_enable(true);

        // Start DMA TX
        DmaRegs::start_tx();

        // Start DMA RX
        DmaRegs::start_rx();

        // Enable MAC receiver
        self.mac_rx_enable(true);

        // Issue RX poll demand to start receiving
        DmaRegs::rx_poll_demand();

        self.state = State::Running;
        Ok(())
    }

    /// Stop the EMAC (disable TX and RX)
    ///
    /// This gracefully stops all DMA operations and disables the MAC.
    ///
    /// # Errors
    /// - `InvalidState` - EMAC is not running
    /// - `Timeout` - DMA did not stop in time
    pub fn stop(&mut self) -> Result<()> {
        if self.state != State::Running {
            return Err(IoError::InvalidState.into());
        }

        // Stop DMA TX
        DmaRegs::stop_tx();

        // Wait for TX to complete
        self.wait_tx_idle()?;

        // Stop DMA RX
        DmaRegs::stop_rx();

        // Disable MAC TX/RX
        self.mac_tx_enable(false);
        self.mac_rx_enable(false);

        // Flush TX FIFO
        self.flush_tx_fifo()?;

        // Disable interrupts
        DmaRegs::disable_all_interrupts();

        // Clear pending interrupts
        DmaRegs::clear_all_interrupts();

        self.state = State::Stopped;
        Ok(())
    }

    /// Enable/disable MAC transmitter
    fn mac_tx_enable(&self, enable: bool) {
        if enable {
            MacRegs::enable_tx();
        } else {
            MacRegs::disable_tx();
        }
    }

    /// Enable/disable MAC receiver
    fn mac_rx_enable(&self, enable: bool) {
        if enable {
            MacRegs::enable_rx();
        } else {
            MacRegs::disable_rx();
        }
    }

    /// Wait for TX DMA to become idle
    fn wait_tx_idle(&self) -> Result<()> {
        for _ in 0..FLUSH_TIMEOUT {
            let status = DmaRegs::status();
            let tx_state = (status >> TX_DMA_STATE_SHIFT) & TX_DMA_STATE_MASK;
            if tx_state == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(IoError::Timeout.into())
    }

    /// Flush TX FIFO
    fn flush_tx_fifo(&self) -> Result<()> {
        DmaRegs::flush_tx_fifo();

        for _ in 0..FLUSH_TIMEOUT {
            if DmaRegs::is_tx_fifo_flush_complete() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(IoError::Timeout.into())
    }

    // =========================================================================
    // TX / RX Operations
    // =========================================================================

    /// Transmit a frame
    ///
    /// Copies the frame data to TX buffers and submits to DMA.
    /// Returns the number of bytes submitted.
    ///
    /// # Errors
    /// - `InvalidState` - EMAC not running
    /// - `InvalidLength` - Empty frame
    /// - `FrameTooLarge` - Frame exceeds buffer capacity
    /// - `NoDescriptorsAvailable` - No free TX descriptors
    pub fn transmit(&mut self, data: &[u8]) -> Result<usize> {
        if self.state != State::Running {
            return Err(IoError::InvalidState.into());
        }
        self.dma.transmit(data)
    }

    /// Check if a frame is available for receiving
    #[inline(always)]
    pub fn rx_available(&self) -> bool {
        self.dma.rx_available()
    }

    /// Get the length of the next available frame
    pub fn peek_rx_length(&self) -> Option<usize> {
        self.dma.peek_frame_length()
    }

    /// Receive a frame
    ///
    /// Copies received frame data to the provided buffer.
    /// Returns the actual frame length (excluding CRC).
    ///
    /// # Errors
    /// - `InvalidState` - EMAC not running
    /// - `BufferTooSmall` - Buffer smaller than frame
    /// - `IncompleteFrame` - No complete frame available
    /// - `FrameError` - Frame has receive errors
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if self.state != State::Running {
            return Err(IoError::InvalidState.into());
        }
        self.dma.receive(buffer)
    }

    /// Check if TX is ready (descriptors available)
    pub fn tx_ready(&self) -> bool {
        self.dma.tx_available() > 0
    }

    /// Check if TX can accept a frame of given size
    pub fn can_transmit(&self, len: usize) -> bool {
        self.dma.can_transmit(len)
    }

    // =========================================================================
    // Link Configuration
    // =========================================================================

    /// Set the MAC address
    pub fn set_mac_address(&mut self, addr: &[u8; 6]) {
        self.mac_addr = *addr;
        MacRegs::set_mac_address(addr);
    }

    /// Set the link speed
    ///
    /// This should be called when link status changes (from PHY).
    pub fn set_speed(&mut self, speed: Speed) {
        self.speed = speed;
        MacRegs::set_speed_100mbps(matches!(speed, Speed::Mbps100));
    }

    /// Set the duplex mode
    ///
    /// This should be called when link status changes (from PHY).
    pub fn set_duplex(&mut self, duplex: Duplex) {
        self.duplex = duplex;
        MacRegs::set_duplex_full(matches!(duplex, Duplex::Full));
    }

    /// Update link parameters (speed and duplex)
    pub fn update_link(&mut self, speed: Speed, duplex: Duplex) {
        self.set_speed(speed);
        self.set_duplex(duplex);
    }

    /// Enable/disable promiscuous mode
    pub fn set_promiscuous(&mut self, enable: bool) {
        MacRegs::set_promiscuous(enable);
    }

    /// Enable/disable pass all multicast frames
    pub fn set_pass_all_multicast(&mut self, enable: bool) {
        MacRegs::set_pass_all_multicast(enable);
    }

    // =========================================================================
    // MDIO / PHY Interface
    // =========================================================================

    /// Write to a PHY register via MDIO
    ///
    /// # Arguments
    /// * `phy_addr` - PHY address (0-31)
    /// * `reg` - Register address (0-31)
    /// * `value` - Value to write
    pub fn write_phy_reg(&self, phy_addr: u8, reg: u8, value: u16) -> Result<()> {
        self.wait_mii_not_busy()?;

        MacRegs::set_mii_data(value as u32);

        let cmd = GMACMIIADDR_GB
            | GMACMIIADDR_GW
            | ((phy_addr as u32) << GMACMIIADDR_PA_SHIFT)
            | ((reg as u32) << GMACMIIADDR_GR_SHIFT)
            | ((CSR_CLOCK_DIV_42 << GMACMIIADDR_CR_SHIFT) & GMACMIIADDR_CR_MASK);

        MacRegs::set_mii_address(cmd);

        self.wait_mii_not_busy()
    }

    /// Read from a PHY register via MDIO
    ///
    /// # Arguments
    /// * `phy_addr` - PHY address (0-31)
    /// * `reg` - Register address (0-31)
    ///
    /// # Returns
    /// The 16-bit register value
    pub fn read_phy_reg(&self, phy_addr: u8, reg: u8) -> Result<u16> {
        self.wait_mii_not_busy()?;

        let cmd = GMACMIIADDR_GB
            | ((phy_addr as u32) << GMACMIIADDR_PA_SHIFT)
            | ((reg as u32) << GMACMIIADDR_GR_SHIFT)
            | ((CSR_CLOCK_DIV_42 << GMACMIIADDR_CR_SHIFT) & GMACMIIADDR_CR_MASK);

        MacRegs::set_mii_address(cmd);

        self.wait_mii_not_busy()?;

        let value = MacRegs::mii_data() & 0xFFFF;
        Ok(value as u16)
    }

    /// Wait for MII to become not busy
    fn wait_mii_not_busy(&self) -> Result<()> {
        for _ in 0..MII_BUSY_TIMEOUT {
            if !MacRegs::is_mii_busy() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(IoError::PhyError.into())
    }

    // =========================================================================
    // Interrupt Handling
    // =========================================================================

    /// Get the current interrupt status
    ///
    /// This reads the DMA status register and returns the parsed flags.
    pub fn interrupt_status(&self) -> InterruptStatus {
        InterruptStatus::from_raw(DmaRegs::status())
    }

    /// Clear interrupt flags
    ///
    /// Write-1-to-clear the specified interrupt flags.
    pub fn clear_interrupts(&self, status: InterruptStatus) {
        DmaRegs::set_status(status.to_raw());
    }

    /// Clear all pending interrupts
    pub fn clear_all_interrupts(&self) {
        DmaRegs::clear_all_interrupts();
    }

    /// Handle interrupt (call from ISR)
    ///
    /// Reads and clears interrupt status, returns the status.
    /// Use this in your interrupt handler to process EMAC events.
    pub fn handle_interrupt(&self) -> InterruptStatus {
        let status = self.interrupt_status();
        self.clear_interrupts(status);
        status
    }

    /// Enable/disable TX complete interrupt
    pub fn enable_tx_interrupt(&self, enable: bool) {
        let mut int_en = DmaRegs::interrupt_enable();
        if enable {
            int_en |= 1 << 0; // TIE
        } else {
            int_en &= !(1 << 0);
        }
        DmaRegs::set_interrupt_enable(int_en);
    }

    /// Enable/disable RX complete interrupt
    pub fn enable_rx_interrupt(&self, enable: bool) {
        let mut int_en = DmaRegs::interrupt_enable();
        if enable {
            int_en |= 1 << 6; // RIE
        } else {
            int_en &= !(1 << 6);
        }
        DmaRegs::set_interrupt_enable(int_en);
    }

    // =========================================================================
    // Debug / Statistics
    // =========================================================================

    /// Get the number of available TX descriptors
    pub fn tx_descriptors_available(&self) -> usize {
        self.dma.tx_available()
    }

    /// Get the number of complete RX frames waiting
    pub fn rx_frames_waiting(&self) -> usize {
        self.dma.rx_frame_count()
    }

    /// Get total memory usage of this EMAC instance
    pub const fn memory_usage() -> usize {
        DmaEngine::<RX_BUFS, TX_BUFS, BUF_SIZE>::memory_usage()
            + core::mem::size_of::<EmacConfig>()
            + core::mem::size_of::<State>()
            + 6 // mac_addr
            + core::mem::size_of::<Speed>()
            + core::mem::size_of::<Duplex>()
    }
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Default
    for Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    fn default() -> Self {
        Self::new()
    }
}

// Safety: Emac can be safely shared between threads when properly synchronized
unsafe impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Sync
    for Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
}

unsafe impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Send
    for Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
}

// =============================================================================
// Type Aliases
// =============================================================================

/// Default EMAC configuration with 10 RX/TX buffers of 1600 bytes each
pub type EmacDefault = Emac<10, 10, 1600>;

/// Small EMAC configuration for memory-constrained systems
pub type EmacSmall = Emac<4, 4, 1600>;

/// Large EMAC configuration for high-throughput applications
pub type EmacLarge = Emac<16, 16, 1600>;
