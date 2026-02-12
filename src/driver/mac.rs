//! ESP32 EMAC Driver
//!
//! This module provides the main EMAC driver that integrates the DMA engine,
//! MAC core, and PHY interface into a complete Ethernet MAC implementation.

use embedded_hal::delay::DelayNs;

use super::config::{
    Duplex, EmacConfig, FlowControlConfig, PhyInterface, RmiiClockMode, Speed, State,
};
use crate::internal::constants::{
    CSR_CLOCK_DIV_42, FLUSH_TIMEOUT, MII_BUSY_TIMEOUT, TX_DMA_STATE_MASK, TX_DMA_STATE_SHIFT,
};
use crate::dma::DmaEngine;
use super::error::{ConfigError, DmaError, IoError, Result};
use crate::hal::reset::ResetController;
use crate::internal::register::dma::{
    DMABUSMODE_AAL, DMABUSMODE_ATDS, DMABUSMODE_FB, DMABUSMODE_PBL_MASK, DMABUSMODE_PBL_SHIFT,
    DMABUSMODE_USP, DMAOPERATION_RSF, DMAOPERATION_TSF, DMASTATUS_AIS, DMASTATUS_FBI,
    DMASTATUS_NIS, DMASTATUS_OVF, DMASTATUS_RI, DMASTATUS_RPS, DMASTATUS_RU, DMASTATUS_TI,
    DMASTATUS_TPS, DMASTATUS_TU, DMASTATUS_UNF, DmaRegs,
};
use crate::internal::register::ext::ExtRegs;
use crate::internal::register::gpio::GpioMatrix;
use crate::internal::register::mac::{
    GMACCONFIG_ACS, GMACCONFIG_DM, GMACCONFIG_FES, GMACCONFIG_IPC, GMACCONFIG_JD, GMACCONFIG_PS,
    GMACCONFIG_WD, GMACFF_PM, GMACFF_PR, GMACMIIADDR_CR_MASK, GMACMIIADDR_CR_SHIFT, GMACMIIADDR_GB,
    GMACMIIADDR_GR_SHIFT, GMACMIIADDR_GW, GMACMIIADDR_PA_SHIFT, MacRegs,
};

// =============================================================================
// Helper Types
// =============================================================================

/// Wrapper to use a mutable reference as a DelayNs implementor
///
/// This allows passing `&mut D` to APIs that need `impl DelayNs`.
struct BorrowedDelay<'a, D: DelayNs>(&'a mut D);

impl<D: DelayNs> DelayNs for BorrowedDelay<'_, D> {
    fn delay_ns(&mut self, ns: u32) {
        self.0.delay_ns(ns);
    }
}

// =============================================================================
// Interrupt Status
// =============================================================================

/// Interrupt status flags
#[derive(Debug, Clone, Copy, Default)]
pub struct InterruptStatus {
    /// TX complete - frame transmitted successfully
    pub tx_complete: bool,
    /// TX stopped - TX DMA stopped
    pub tx_stopped: bool,
    /// TX buffer unavailable - no TX descriptors available
    pub tx_buf_unavailable: bool,
    /// TX underflow - TX FIFO underflow
    pub tx_underflow: bool,
    /// RX complete - frame received
    pub rx_complete: bool,
    /// RX stopped - RX DMA stopped
    pub rx_stopped: bool,
    /// RX buffer unavailable - no RX descriptors available
    pub rx_buf_unavailable: bool,
    /// RX overflow - RX FIFO overflow
    pub rx_overflow: bool,
    /// Fatal bus error - unrecoverable DMA error
    pub fatal_bus_error: bool,
    /// Normal interrupt summary
    pub normal_summary: bool,
    /// Abnormal interrupt summary
    pub abnormal_summary: bool,
}

impl InterruptStatus {
    /// Create from raw DMA status register value
    pub fn from_raw(status: u32) -> Self {
        Self {
            tx_complete: (status & DMASTATUS_TI) != 0,
            tx_stopped: (status & DMASTATUS_TPS) != 0,
            tx_buf_unavailable: (status & DMASTATUS_TU) != 0,
            tx_underflow: (status & DMASTATUS_UNF) != 0,
            rx_complete: (status & DMASTATUS_RI) != 0,
            rx_stopped: (status & DMASTATUS_RPS) != 0,
            rx_buf_unavailable: (status & DMASTATUS_RU) != 0,
            rx_overflow: (status & DMASTATUS_OVF) != 0,
            fatal_bus_error: (status & DMASTATUS_FBI) != 0,
            normal_summary: (status & DMASTATUS_NIS) != 0,
            abnormal_summary: (status & DMASTATUS_AIS) != 0,
        }
    }

    /// Convert to raw value for clearing (write-1-to-clear)
    pub fn to_raw(&self) -> u32 {
        let mut val = 0u32;
        if self.tx_complete {
            val |= DMASTATUS_TI;
        }
        if self.tx_stopped {
            val |= DMASTATUS_TPS;
        }
        if self.tx_buf_unavailable {
            val |= DMASTATUS_TU;
        }
        if self.tx_underflow {
            val |= DMASTATUS_UNF;
        }
        if self.rx_complete {
            val |= DMASTATUS_RI;
        }
        if self.rx_stopped {
            val |= DMASTATUS_RPS;
        }
        if self.rx_buf_unavailable {
            val |= DMASTATUS_RU;
        }
        if self.rx_overflow {
            val |= DMASTATUS_OVF;
        }
        if self.fatal_bus_error {
            val |= DMASTATUS_FBI;
        }
        if self.normal_summary {
            val |= DMASTATUS_NIS;
        }
        if self.abnormal_summary {
            val |= DMASTATUS_AIS;
        }
        val
    }

    /// Check if any interrupt occurred
    pub fn any(&self) -> bool {
        self.tx_complete
            || self.tx_stopped
            || self.tx_buf_unavailable
            || self.tx_underflow
            || self.rx_complete
            || self.rx_stopped
            || self.rx_buf_unavailable
            || self.rx_overflow
            || self.fatal_bus_error
    }

    /// Check if any error occurred
    pub fn has_error(&self) -> bool {
        self.tx_underflow || self.rx_overflow || self.fatal_bus_error
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
pub struct Emac<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    /// DMA engine
    dma: DmaEngine<RX_BUFS, TX_BUFS, BUF_SIZE>,
    /// Current configuration
    config: EmacConfig,
    /// Current state
    state: State,
    /// MAC address
    mac_addr: [u8; 6],
    /// Current link speed
    speed: Speed,
    /// Current duplex mode
    duplex: Duplex,
    /// Flow control state: peer supports PAUSE frames
    peer_pause_ability: bool,
    /// Flow control state: currently applying backpressure
    flow_control_active: bool,
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
    pub fn init<D: DelayNs>(&mut self, config: EmacConfig, mut delay: D) -> Result<()> {
        if self.state != State::Uninitialized {
            return Err(ConfigError::AlreadyInitialized.into());
        }

        self.config = config;

        // === STEP 1: Configure GPIO routing BEFORE any EMAC operations ===
        // GPIO0 IO_MUX is NOT part of EMAC peripheral, so it can be configured
        // before EMAC peripheral clock is enabled. This routes the external
        // 50MHz clock to the EMAC block.
        if matches!(self.config.rmii_clock, RmiiClockMode::ExternalInput { .. }) {
            ExtRegs::configure_gpio0_rmii_clock_input();

            #[cfg(feature = "defmt")]
            defmt::info!("GPIO0 configured for external RMII clock input");
        }

        // Configure SMI pins (MDC/MDIO) via GPIO Matrix
        // This MUST be done before using MDIO to communicate with the PHY
        GpioMatrix::configure_smi_pins();

        #[cfg(feature = "defmt")]
        defmt::info!("SMI pins configured: GPIO23=MDC, GPIO18=MDIO");

        // Configure RMII data pins via IO_MUX (fixed pins, function 5)
        // This MUST be done for TX/RX to work
        GpioMatrix::configure_rmii_pins();

        #[cfg(feature = "defmt")]
        defmt::info!("RMII data pins configured via IO_MUX");

        // === STEP 2: Enable DPORT peripheral clock ===
        // This enables access to EMAC registers (required before any EMAC register access)
        ExtRegs::enable_peripheral_clock();

        #[cfg(feature = "defmt")]
        defmt::info!("EMAC peripheral clock enabled via DPORT");

        // === STEP 3: Configure PHY interface in extension registers ===
        // Now that EMAC peripheral is accessible, configure RMII/MII mode and clock source
        self.configure_phy_interface_regs();

        // === STEP 4: Enable EMAC extension clocks ===
        // The external clock should now be reaching the EMAC, so we can enable internal clocks
        ExtRegs::enable_clocks();
        ExtRegs::power_up_ram();

        #[cfg(feature = "defmt")]
        {
            let bus_mode = crate::register::dma::DmaRegs::bus_mode();
            defmt::debug!("DMA bus_mode after clock enable: {:#010x}", bus_mode);
        }

        // === STEP 5: Perform software reset ===
        // With clocks properly configured, DMA reset should complete
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

    /// Enable EMAC peripheral clocks
    ///
    /// NOTE: This is now deprecated/unused - the clock enable logic has been
    /// moved to init() for more explicit control over initialization order.
    /// Keeping for reference.
    #[allow(dead_code)]
    fn enable_clocks_deprecated(&self) {
        ExtRegs::enable_peripheral_clock();
        ExtRegs::enable_clocks();
        ExtRegs::power_up_ram();
    }

    /// Configure PHY interface extension registers (MII/RMII mode and clock source)
    ///
    /// This configures the EMAC extension registers for the PHY interface.
    /// NOTE: DPORT peripheral clock must be enabled before calling this!
    /// NOTE: GPIO0 should already be configured for external clock if applicable.
    fn configure_phy_interface_regs(&self) {
        match self.config.phy_interface {
            PhyInterface::Rmii => {
                ExtRegs::set_rmii_mode();

                match self.config.rmii_clock {
                    RmiiClockMode::ExternalInput { .. } => {
                        // GPIO0 should already be configured via IO_MUX in init()
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
    ///
    /// This uses the provided delay to perform the DMA soft reset
    /// and wait for completion with timeout.
    fn software_reset<D: DelayNs>(&self, delay: &mut D) -> Result<()> {
        // Create a borrowed delay wrapper for the reset controller
        let mut reset_ctrl = ResetController::new(BorrowedDelay(delay));

        // Perform soft reset via HAL
        reset_ctrl
            .soft_reset()
            .map_err(|_| ConfigError::ResetFailed.into())
    }

    /// Configure MAC defaults
    fn configure_mac_defaults(&self) {
        // Build configuration register value
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
        // Configure bus mode
        let pbl = self.config.dma_burst_len.to_pbl();
        let bus_mode = DMABUSMODE_FB  // Fixed burst
            | DMABUSMODE_AAL           // Address-aligned beats
            | DMABUSMODE_USP           // Use separate PBL
            | DMABUSMODE_ATDS          // Alternate descriptor size (8 words)
            | ((pbl << DMABUSMODE_PBL_SHIFT) & DMABUSMODE_PBL_MASK);

        DmaRegs::set_bus_mode(bus_mode);

        // Configure operation mode (store and forward)
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
            // Check TX process state (bits 22:20)
            let tx_state = (status >> TX_DMA_STATE_SHIFT) & TX_DMA_STATE_MASK;
            if tx_state == 0 {
                // Stopped
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

    /// Enable or disable broadcast frame reception.
    ///
    /// When disabled, broadcast frames are filtered out.
    pub fn set_broadcast_enabled(&mut self, enable: bool) {
        MacRegs::set_broadcast_enabled(enable);
    }

    // =========================================================================
    // Flow Control
    // =========================================================================

    /// Enable or disable flow control
    ///
    /// This configures the MAC for IEEE 802.3 PAUSE frame-based flow control.
    /// Must be called after init() and before or after start().
    pub fn enable_flow_control(&mut self, enable: bool) {
        if enable && self.peer_pause_ability {
            let fc = &self.config.flow_control;
            MacRegs::configure_flow_control(
                fc.pause_time,
                fc.pause_low_threshold as u8,
                fc.unicast_pause_detect,
                true, // TX flow control
                true, // RX flow control
            );
        } else {
            MacRegs::configure_flow_control(0, 0, false, false, false);
        }
    }

    /// Set peer PAUSE frame ability
    ///
    /// This should be called after PHY auto-negotiation completes to indicate
    /// whether the link partner supports PAUSE frames. Flow control will only
    /// be enabled if both the user configuration requests it AND the peer
    /// supports it.
    ///
    /// # Arguments
    /// * `ability` - true if peer advertised PAUSE capability
    pub fn set_peer_pause_ability(&mut self, ability: bool) {
        self.peer_pause_ability = ability;

        // Re-configure flow control based on new peer ability
        if self.config.flow_control.enabled {
            self.enable_flow_control(ability);
        }
    }

    /// Check if flow control action is needed and send PAUSE frame if necessary
    ///
    /// This implements software flow control logic based on RX descriptor
    /// availability. Call this periodically (e.g., from RX interrupt handler)
    /// to manage PAUSE frame transmission.
    ///
    /// Returns true if flow control state changed.
    pub fn check_flow_control(&mut self) -> bool {
        // Only do flow control if enabled and peer supports it
        if !self.config.flow_control.enabled || !self.peer_pause_ability {
            return false;
        }

        let fc = &self.config.flow_control;
        let free_descriptors = self.dma.rx_free_count();
        let frames_remain = self.rx_frames_waiting() > 0;

        // Check if we need to activate flow control (send PAUSE)
        if !self.flow_control_active && free_descriptors < fc.low_water_mark && frames_remain {
            MacRegs::send_pause_frame(true);
            self.flow_control_active = true;
            return true;
        }

        // Check if we can deactivate flow control (resume)
        if self.flow_control_active && (free_descriptors > fc.high_water_mark || !frames_remain) {
            MacRegs::send_pause_frame(false);
            self.flow_control_active = false;
            return true;
        }

        false
    }

    /// Get current flow control state
    #[inline(always)]
    pub fn is_flow_control_active(&self) -> bool {
        self.flow_control_active
    }

    /// Get flow control configuration
    #[inline(always)]
    pub fn flow_control_config(&self) -> &FlowControlConfig {
        &self.config.flow_control
    }

    // =========================================================================
    // MAC Address Filtering
    // =========================================================================

    /// Add a MAC address filter
    ///
    /// The ESP32 supports up to 4 additional MAC address filters (beyond the
    /// primary address). This allows receiving frames addressed to multiple
    /// unicast or multicast addresses without enabling promiscuous mode.
    ///
    /// # Arguments
    /// * `addr` - MAC address to accept
    ///
    /// # Returns
    /// * `Ok(slot)` - The filter slot (1-4) where the address was added
    /// * `Err(NoDescriptorsAvailable)` - All 4 filter slots are in use
    ///
    /// # Example
    /// ```ignore
    /// // Accept frames addressed to a multicast group
    /// emac.add_mac_filter(&[0x01, 0x00, 0x5E, 0x00, 0x00, 0x01])?;
    /// ```
    pub fn add_mac_filter(&mut self, addr: &[u8; 6]) -> Result<usize> {
        // Check if already in filter
        if MacRegs::find_mac_filter(addr).is_some() {
            return Err(ConfigError::AlreadyInitialized.into());
        }

        // Find a free slot
        let slot = MacRegs::find_free_mac_filter_slot().ok_or(DmaError::NoDescriptorsAvailable)?;

        // Add the filter (destination address, no mask)
        MacRegs::set_mac_filter(slot, addr, false, 0);

        Ok(slot)
    }

    /// Add a MAC address filter with full configuration
    ///
    /// # Arguments
    /// * `filter` - Complete filter configuration including type and mask
    ///
    /// # Returns
    /// * `Ok(slot)` - The filter slot (1-4) where the filter was added
    /// * `Err(NoDescriptorsAvailable)` - All 4 filter slots are in use
    pub fn add_mac_filter_config(
        &mut self,
        filter: &super::config::MacAddressFilter,
    ) -> Result<usize> {
        // Check if already in filter
        if MacRegs::find_mac_filter(&filter.address).is_some() {
            return Err(ConfigError::AlreadyInitialized.into());
        }

        // Find a free slot
        let slot = MacRegs::find_free_mac_filter_slot().ok_or(DmaError::NoDescriptorsAvailable)?;

        let is_source = matches!(filter.filter_type, super::config::MacFilterType::Source);
        MacRegs::set_mac_filter(slot, &filter.address, is_source, filter.byte_mask);

        Ok(slot)
    }

    /// Remove a MAC address from the filter
    ///
    /// # Arguments
    /// * `addr` - MAC address to remove
    ///
    /// # Returns
    /// * `Ok(())` - Address was found and removed
    /// * `Err(InvalidLength)` - Address was not in the filter
    pub fn remove_mac_filter(&mut self, addr: &[u8; 6]) -> Result<()> {
        let slot = MacRegs::find_mac_filter(addr).ok_or(DmaError::InvalidLength)?;

        MacRegs::clear_mac_filter(slot);
        Ok(())
    }

    /// Clear all MAC address filters
    ///
    /// This removes all additional address filters, leaving only the primary
    /// MAC address active.
    pub fn clear_mac_filters(&mut self) {
        MacRegs::clear_all_mac_filters();
    }

    /// Get the number of active MAC address filters
    pub fn mac_filter_count(&self) -> usize {
        let mut count = 0;
        for slot in 1..=4 {
            if MacRegs::is_mac_filter_enabled(slot) == Some(true) {
                count += 1;
            }
        }
        count
    }

    /// Check if there are free MAC filter slots available
    pub fn has_free_mac_filter_slot(&self) -> bool {
        MacRegs::find_free_mac_filter_slot().is_some()
    }

    // =========================================================================
    // Hash Table Filtering
    // =========================================================================

    /// Add a MAC address to the hash filter
    ///
    /// The hash filter provides an efficient way to filter multiple multicast
    /// addresses without using the limited perfect filter slots. It uses a
    /// 64-bit hash table where each address maps to one bit.
    ///
    /// **Note:** Hash collisions are possible - multiple addresses may map to
    /// the same bit, causing some unwanted frames to pass through. This is
    /// acceptable for multicast filtering where software can do final filtering.
    ///
    /// # Arguments
    /// * `addr` - MAC address to add to the hash filter
    ///
    /// # Returns
    /// The hash index (0-63) where the address was added
    ///
    /// # Example
    /// ```ignore
    /// // Subscribe to IPv4 multicast group 224.0.0.1
    /// let multicast_addr = [0x01, 0x00, 0x5E, 0x00, 0x00, 0x01];
    /// emac.add_hash_filter(&multicast_addr);
    /// ```
    pub fn add_hash_filter(&mut self, addr: &[u8; 6]) -> u8 {
        let index = MacRegs::compute_hash_index(addr);
        MacRegs::set_hash_bit(index);
        index
    }

    /// Remove a MAC address from the hash filter
    ///
    /// **Warning:** If multiple addresses hash to the same bit, removing one
    /// will affect all of them. Consider tracking reference counts externally
    /// if you need precise removal behavior.
    ///
    /// # Arguments
    /// * `addr` - MAC address to remove from the hash filter
    ///
    /// # Returns
    /// The hash index (0-63) that was cleared
    pub fn remove_hash_filter(&mut self, addr: &[u8; 6]) -> u8 {
        let index = MacRegs::compute_hash_index(addr);
        MacRegs::clear_hash_bit(index);
        index
    }

    /// Check if a MAC address would pass the hash filter
    ///
    /// # Arguments
    /// * `addr` - MAC address to check
    ///
    /// # Returns
    /// `true` if the address's hash bit is set
    pub fn check_hash_filter(&self, addr: &[u8; 6]) -> bool {
        let index = MacRegs::compute_hash_index(addr);
        MacRegs::is_hash_bit_set(index)
    }

    /// Clear the entire hash table
    ///
    /// This disables hash-based filtering for all addresses.
    pub fn clear_hash_table(&mut self) {
        MacRegs::clear_hash_table();
    }

    /// Get the current hash table value
    pub fn hash_table(&self) -> u64 {
        MacRegs::hash_table()
    }

    /// Set the entire hash table at once
    ///
    /// Useful for restoring a saved state or bulk configuration.
    pub fn set_hash_table(&mut self, value: u64) {
        MacRegs::set_hash_table(value);
    }

    /// Enable hash-based multicast filtering
    ///
    /// When enabled, multicast frames are filtered using the hash table
    /// instead of passing all multicast (PM bit). This is more efficient
    /// for subscribing to specific multicast groups.
    ///
    /// # Arguments
    /// * `enable` - `true` to enable hash multicast filtering
    pub fn enable_hash_multicast(&mut self, enable: bool) {
        MacRegs::enable_hash_multicast(enable);

        // If enabling hash multicast, disable pass-all-multicast
        if enable {
            MacRegs::set_pass_all_multicast(false);
        }
    }

    /// Enable hash-based unicast filtering
    ///
    /// When enabled, unicast frames can be filtered using the hash table
    /// in addition to the perfect filter. This allows accepting frames
    /// for more unicast addresses than the perfect filter slots allow.
    ///
    /// # Arguments
    /// * `enable` - `true` to enable hash unicast filtering
    pub fn enable_hash_unicast(&mut self, enable: bool) {
        MacRegs::enable_hash_unicast(enable);
    }

    /// Compute hash index for a MAC address without modifying the table
    ///
    /// Useful for debugging or checking for potential collisions.
    pub fn compute_hash_index(addr: &[u8; 6]) -> u8 {
        MacRegs::compute_hash_index(addr)
    }

    // =========================================================================
    // VLAN Filtering
    // =========================================================================

    /// Enable VLAN tag filtering with a specific VLAN ID
    ///
    /// Configures the MAC to filter frames based on the 802.1Q VLAN tag.
    /// Only frames with the specified VLAN ID will be received.
    ///
    /// # Arguments
    /// * `vid` - VLAN Identifier (0-4095)
    ///
    /// # Example
    /// ```ignore
    /// // Only receive frames from VLAN 100
    /// emac.set_vlan_filter(100);
    /// ```
    pub fn set_vlan_filter(&mut self, vid: u16) {
        MacRegs::set_vlan_id_filter(vid);
        MacRegs::enable_vlan_filter(true);
    }

    /// Configure VLAN filter with full options
    ///
    /// # Arguments
    /// * `vid` - VLAN Identifier (0-4095) or full 16-bit tag
    /// * `vid_only` - If true, compare only 12-bit VID; if false, compare full tag
    /// * `inverse` - If true, pass frames that DON'T match (exclusion filter)
    /// * `svlan` - If true, match S-VLAN (0x88A8); if false, match C-VLAN (0x8100)
    pub fn configure_vlan_filter(&mut self, vid: u16, vid_only: bool, inverse: bool, svlan: bool) {
        MacRegs::configure_vlan_filter(vid, vid_only, inverse, svlan);
        MacRegs::enable_vlan_filter(true);
    }

    /// Disable VLAN filtering
    ///
    /// After calling this, frames will not be filtered by VLAN tag.
    pub fn disable_vlan_filter(&mut self) {
        MacRegs::clear_vlan_filter();
    }

    /// Check if VLAN filtering is currently enabled
    pub fn is_vlan_filter_enabled(&self) -> bool {
        MacRegs::is_vlan_filter_enabled()
    }

    /// Get the currently configured VLAN ID filter
    ///
    /// Returns the 12-bit VLAN ID that is being filtered.
    pub fn vlan_filter_id(&self) -> u16 {
        MacRegs::get_vlan_id_filter()
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
        // Wait for not busy
        self.wait_mii_not_busy()?;

        // Write data
        MacRegs::set_mii_data(value as u32);

        // Build command: write, busy, PHY address, register address, clock divider
        let cmd = GMACMIIADDR_GB  // Busy
            | GMACMIIADDR_GW     // Write
            | ((phy_addr as u32) << GMACMIIADDR_PA_SHIFT)
            | ((reg as u32) << GMACMIIADDR_GR_SHIFT)
            | ((CSR_CLOCK_DIV_42 << GMACMIIADDR_CR_SHIFT) & GMACMIIADDR_CR_MASK);

        MacRegs::set_mii_address(cmd);

        // Wait for completion
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
        // Wait for not busy
        self.wait_mii_not_busy()?;

        // Build command: read (no GW), busy, PHY address, register address, clock divider
        let cmd = GMACMIIADDR_GB  // Busy
            | ((phy_addr as u32) << GMACMIIADDR_PA_SHIFT)
            | ((reg as u32) << GMACMIIADDR_GR_SHIFT)
            | ((CSR_CLOCK_DIV_42 << GMACMIIADDR_CR_SHIFT) & GMACMIIADDR_CR_MASK);

        MacRegs::set_mii_address(cmd);

        // Wait for completion
        self.wait_mii_not_busy()?;

        // Read data
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

// SAFETY: Emac can be safely shared between threads when properly synchronized.
unsafe impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Sync
    for Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
}

// SAFETY: Emac can be safely shared between threads when properly synchronized.
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

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // InterruptStatus Tests
    // =========================================================================

    #[test]
    fn interrupt_status_from_raw_zero() {
        let status = InterruptStatus::from_raw(0);

        assert!(!status.tx_complete);
        assert!(!status.tx_stopped);
        assert!(!status.tx_buf_unavailable);
        assert!(!status.tx_underflow);
        assert!(!status.rx_complete);
        assert!(!status.rx_stopped);
        assert!(!status.rx_buf_unavailable);
        assert!(!status.rx_overflow);
        assert!(!status.fatal_bus_error);
        assert!(!status.normal_summary);
        assert!(!status.abnormal_summary);
    }

    #[test]
    fn interrupt_status_from_raw_tx_complete() {
        let status = InterruptStatus::from_raw(DMASTATUS_TI);

        assert!(status.tx_complete);
        assert!(!status.rx_complete);
        assert!(!status.fatal_bus_error);
    }

    #[test]
    fn interrupt_status_from_raw_rx_complete() {
        let status = InterruptStatus::from_raw(DMASTATUS_RI);

        assert!(status.rx_complete);
        assert!(!status.tx_complete);
        assert!(!status.fatal_bus_error);
    }

    #[test]
    fn interrupt_status_from_raw_tx_stopped() {
        let status = InterruptStatus::from_raw(DMASTATUS_TPS);

        assert!(status.tx_stopped);
        assert!(!status.tx_complete);
    }

    #[test]
    fn interrupt_status_from_raw_rx_stopped() {
        let status = InterruptStatus::from_raw(DMASTATUS_RPS);

        assert!(status.rx_stopped);
        assert!(!status.rx_complete);
    }

    #[test]
    fn interrupt_status_from_raw_tx_buf_unavailable() {
        let status = InterruptStatus::from_raw(DMASTATUS_TU);

        assert!(status.tx_buf_unavailable);
    }

    #[test]
    fn interrupt_status_from_raw_rx_buf_unavailable() {
        let status = InterruptStatus::from_raw(DMASTATUS_RU);

        assert!(status.rx_buf_unavailable);
    }

    #[test]
    fn interrupt_status_from_raw_tx_underflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_UNF);

        assert!(status.tx_underflow);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_from_raw_rx_overflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_OVF);

        assert!(status.rx_overflow);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_from_raw_fatal_bus_error() {
        let status = InterruptStatus::from_raw(DMASTATUS_FBI);

        assert!(status.fatal_bus_error);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_from_raw_normal_summary() {
        let status = InterruptStatus::from_raw(DMASTATUS_NIS);

        assert!(status.normal_summary);
        assert!(!status.abnormal_summary);
    }

    #[test]
    fn interrupt_status_from_raw_abnormal_summary() {
        let status = InterruptStatus::from_raw(DMASTATUS_AIS);

        assert!(status.abnormal_summary);
        assert!(!status.normal_summary);
    }

    #[test]
    fn interrupt_status_from_raw_all_bits() {
        let all_bits = DMASTATUS_TI
            | DMASTATUS_TPS
            | DMASTATUS_TU
            | DMASTATUS_UNF
            | DMASTATUS_RI
            | DMASTATUS_RPS
            | DMASTATUS_RU
            | DMASTATUS_OVF
            | DMASTATUS_FBI
            | DMASTATUS_NIS
            | DMASTATUS_AIS;

        let status = InterruptStatus::from_raw(all_bits);

        assert!(status.tx_complete);
        assert!(status.tx_stopped);
        assert!(status.tx_buf_unavailable);
        assert!(status.tx_underflow);
        assert!(status.rx_complete);
        assert!(status.rx_stopped);
        assert!(status.rx_buf_unavailable);
        assert!(status.rx_overflow);
        assert!(status.fatal_bus_error);
        assert!(status.normal_summary);
        assert!(status.abnormal_summary);
    }

    #[test]
    fn interrupt_status_to_raw_roundtrip() {
        // Test that from_raw -> to_raw gives back the original value
        let original = DMASTATUS_TI | DMASTATUS_RI | DMASTATUS_NIS;
        let status = InterruptStatus::from_raw(original);
        let roundtrip = status.to_raw();

        assert_eq!(roundtrip, original);
    }

    #[test]
    fn interrupt_status_to_raw_roundtrip_all() {
        let all_bits = DMASTATUS_TI
            | DMASTATUS_TPS
            | DMASTATUS_TU
            | DMASTATUS_UNF
            | DMASTATUS_RI
            | DMASTATUS_RPS
            | DMASTATUS_RU
            | DMASTATUS_OVF
            | DMASTATUS_FBI
            | DMASTATUS_NIS
            | DMASTATUS_AIS;

        let status = InterruptStatus::from_raw(all_bits);
        let roundtrip = status.to_raw();

        assert_eq!(roundtrip, all_bits);
    }

    #[test]
    fn interrupt_status_to_raw_zero() {
        let status = InterruptStatus::default();
        let raw = status.to_raw();

        assert_eq!(raw, 0);
    }

    // =========================================================================
    // any() Method Tests
    // =========================================================================

    #[test]
    fn interrupt_status_any_false_when_zero() {
        let status = InterruptStatus::from_raw(0);
        assert!(!status.any());
    }

    #[test]
    fn interrupt_status_any_true_for_tx() {
        let status = InterruptStatus::from_raw(DMASTATUS_TI);
        assert!(status.any());
    }

    #[test]
    fn interrupt_status_any_true_for_rx() {
        let status = InterruptStatus::from_raw(DMASTATUS_RI);
        assert!(status.any());
    }

    #[test]
    fn interrupt_status_any_true_for_error() {
        let status = InterruptStatus::from_raw(DMASTATUS_FBI);
        assert!(status.any());
    }

    // Note: normal_summary and abnormal_summary are NOT included in any()
    // They are summary bits, not actual interrupt sources
    #[test]
    fn interrupt_status_any_ignores_summary_bits() {
        // Only summary bits set - any() should still be false
        let status = InterruptStatus::from_raw(DMASTATUS_NIS | DMASTATUS_AIS);
        assert!(!status.any());
    }

    // =========================================================================
    // has_error() Method Tests
    // =========================================================================

    #[test]
    fn interrupt_status_has_error_false_when_zero() {
        let status = InterruptStatus::from_raw(0);
        assert!(!status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_false_for_normal() {
        // Normal TX/RX completion is not an error
        let status = InterruptStatus::from_raw(DMASTATUS_TI | DMASTATUS_RI | DMASTATUS_NIS);
        assert!(!status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_underflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_UNF);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_overflow() {
        let status = InterruptStatus::from_raw(DMASTATUS_OVF);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_fatal_bus() {
        let status = InterruptStatus::from_raw(DMASTATUS_FBI);
        assert!(status.has_error());
    }

    #[test]
    fn interrupt_status_has_error_true_for_multiple_errors() {
        let status = InterruptStatus::from_raw(DMASTATUS_UNF | DMASTATUS_OVF | DMASTATUS_FBI);
        assert!(status.has_error());
    }

    // =========================================================================
    // Default Implementation Tests
    // =========================================================================

    #[test]
    fn interrupt_status_default_is_zero() {
        let status = InterruptStatus::default();

        assert!(!status.any());
        assert!(!status.has_error());
        assert_eq!(status.to_raw(), 0);
    }
}
