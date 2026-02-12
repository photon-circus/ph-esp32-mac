//! Configuration types for ESP32 EMAC driver

use crate::internal::constants::{
    DEFAULT_FLOW_HIGH_WATER, DEFAULT_FLOW_LOW_WATER, DEFAULT_MAC_ADDR, MDC_MAX_FREQ_HZ,
    PAUSE_TIME_MAX, SOFT_RESET_TIMEOUT_MS,
};

/// Ethernet link speed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Speed {
    /// 10 Mbps
    Mbps10,
    /// 100 Mbps
    #[default]
    Mbps100,
}

/// Ethernet duplex mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Duplex {
    /// Half duplex
    Half,
    /// Full duplex
    #[default]
    Full,
}

/// PHY interface type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PhyInterface {
    /// Media Independent Interface
    Mii,
    /// Reduced Media Independent Interface
    #[default]
    Rmii,
}

/// Clock mode for RMII interface
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RmiiClockMode {
    /// External 50MHz clock input on specified GPIO
    ExternalInput {
        /// GPIO number for clock input (typically GPIO0)
        gpio: u8,
    },
    /// Internal 50MHz clock output on specified GPIO
    InternalOutput {
        /// GPIO number for clock output (GPIO16 or GPIO17)
        gpio: u8,
    },
}

impl Default for RmiiClockMode {
    fn default() -> Self {
        RmiiClockMode::ExternalInput { gpio: 0 }
    }
}

/// DMA burst length configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum DmaBurstLen {
    /// 1 beat burst
    Burst1 = 1,
    /// 2 beat burst
    Burst2 = 2,
    /// 4 beat burst
    Burst4 = 4,
    /// 8 beat burst
    Burst8 = 8,
    /// 16 beat burst
    Burst16 = 16,
    /// 32 beat burst (default, best performance)
    #[default]
    Burst32 = 32,
}

impl DmaBurstLen {
    /// Convert to the programmable burst length value for DMA register
    #[must_use]
    pub const fn to_pbl(self) -> u32 {
        self as u32
    }
}

/// Maximum number of additional MAC address filter slots
pub const MAC_FILTER_SLOTS: usize = 4;

/// MAC address filter type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MacFilterType {
    /// Filter by destination address (most common)
    #[default]
    Destination,
    /// Filter by source address
    Source,
}

/// MAC address filter entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MacAddressFilter {
    /// The MAC address to filter
    pub address: [u8; 6],
    /// Filter type (source or destination)
    pub filter_type: MacFilterType,
    /// Byte mask - each bit masks one byte of the address comparison
    /// Bit 0 masks `addr[0]`, bit 5 masks `addr[5]`.
    /// Masked bytes are not compared (act as wildcards)
    pub byte_mask: u8,
}

impl MacAddressFilter {
    /// Create a new destination address filter with exact match
    #[must_use]
    pub const fn new(address: [u8; 6]) -> Self {
        Self {
            address,
            filter_type: MacFilterType::Destination,
            byte_mask: 0,
        }
    }

    /// Create a new source address filter with exact match
    #[must_use]
    pub const fn source(address: [u8; 6]) -> Self {
        Self {
            address,
            filter_type: MacFilterType::Source,
            byte_mask: 0,
        }
    }

    /// Create a filter with masked bytes (for group addresses)
    #[must_use]
    pub const fn with_mask(address: [u8; 6], byte_mask: u8) -> Self {
        Self {
            address,
            filter_type: MacFilterType::Destination,
            byte_mask,
        }
    }
}

// =============================================================================
// Hardware-Fixed Pin Assignments
// =============================================================================

// Note: The ESP32 EMAC uses dedicated internal routing for RMII data pins.
// These pins are fixed by hardware and cannot be reassigned:
//
// | Signal   | GPIO | Direction | Description                    |
// |----------|------|-----------|--------------------------------|
// | TXD0     | 19   | Output    | Transmit Data 0                |
// | TXD1     | 22   | Output    | Transmit Data 1                |
// | TX_EN    | 21   | Output    | Transmit Enable                |
// | RXD0     | 25   | Input     | Receive Data 0                 |
// | RXD1     | 26   | Input     | Receive Data 1                 |
// | CRS_DV   | 27   | Input     | Carrier Sense / Data Valid     |
// | REF_CLK  | 0    | Input     | 50 MHz Reference Clock (default)|
//
// SMI (MDIO) pins typically use GPIO23 (MDC) and GPIO18 (MDIO) but
// can be routed via GPIO matrix. Pin configuration is handled by esp-hal.
//
// See `internal::gpio_pins` or the WT32-ETH01 board helper for pin definitions.

/// Checksum offload configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ChecksumConfig {
    /// Enable RX checksum offload (IP/TCP/UDP)
    pub rx_checksum: bool,
    /// TX checksum insertion mode
    pub tx_checksum: TxChecksumMode,
}

/// Flow control configuration
///
/// Implements IEEE 802.3 PAUSE frame-based flow control to prevent
/// buffer overflow during high traffic conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FlowControlConfig {
    /// Enable flow control (user preference)
    pub enabled: bool,
    /// Low water mark: when free RX descriptors drop below this,
    /// send PAUSE frame to request sender to stop
    pub low_water_mark: usize,
    /// High water mark: when free RX descriptors rise above this,
    /// send PAUSE frame with zero quanta to resume
    pub high_water_mark: usize,
    /// PAUSE time in slot times (512 bit times each)
    /// Default 0xFFFF = max pause time (~33ms at 100Mbps)
    pub pause_time: u16,
    /// PAUSE low threshold - controls when retransmit occurs
    pub pause_low_threshold: PauseLowThreshold,
    /// Enable unicast PAUSE frame detection
    pub unicast_pause_detect: bool,
}

impl Default for FlowControlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            low_water_mark: DEFAULT_FLOW_LOW_WATER,
            high_water_mark: DEFAULT_FLOW_HIGH_WATER,
            pause_time: PAUSE_TIME_MAX,
            pause_low_threshold: PauseLowThreshold::Minus4,
            unicast_pause_detect: false,
        }
    }
}

impl FlowControlConfig {
    /// Create flow control config with custom water marks
    #[must_use]
    pub const fn with_water_marks(low: usize, high: usize) -> Self {
        Self {
            enabled: true,
            low_water_mark: low,
            high_water_mark: high,
            pause_time: PAUSE_TIME_MAX,
            pause_low_threshold: PauseLowThreshold::Minus4,
            unicast_pause_detect: false,
        }
    }
}

/// PAUSE low threshold values
///
/// Threshold of PAUSE timer at which retransmit is requested,
/// relative to the current pause_time value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum PauseLowThreshold {
    /// Pause time minus 4 slot times
    #[default]
    Minus4 = 0,
    /// Pause time minus 28 slot times
    Minus28 = 1,
    /// Pause time minus 144 slot times
    Minus144 = 2,
    /// Pause time minus 256 slot times
    Minus256 = 3,
}

/// TX checksum insertion mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum TxChecksumMode {
    /// Checksum insertion disabled
    #[default]
    Disabled = 0,
    /// Insert IP header checksum only
    IpHeaderOnly = 1,
    /// Insert IP header and payload checksum (TCP/UDP pseudo-header not calculated)
    IpAndPayload = 2,
    /// Insert IP header and payload checksum with pseudo-header
    Full = 3,
}

/// Complete EMAC configuration
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct EmacConfig {
    /// PHY interface type (MII or RMII)
    pub phy_interface: PhyInterface,
    /// RMII clock mode (only used if phy_interface is RMII)
    pub rmii_clock: RmiiClockMode,
    /// MAC address (6 bytes)
    pub mac_address: [u8; 6],
    /// DMA burst length
    pub dma_burst_len: DmaBurstLen,
    /// Software reset timeout in milliseconds
    pub sw_reset_timeout_ms: u32,
    /// MDC clock frequency in Hz (max 2.5 MHz per IEEE 802.3)
    pub mdc_freq_hz: u32,
    /// Enable promiscuous mode (receive all frames)
    pub promiscuous: bool,
    /// Checksum offload configuration
    pub checksum: ChecksumConfig,
    /// Flow control configuration
    pub flow_control: FlowControlConfig,
}

impl Default for EmacConfig {
    fn default() -> Self {
        Self {
            phy_interface: PhyInterface::default(),
            rmii_clock: RmiiClockMode::default(),
            mac_address: DEFAULT_MAC_ADDR,
            dma_burst_len: DmaBurstLen::default(),
            sw_reset_timeout_ms: SOFT_RESET_TIMEOUT_MS,
            mdc_freq_hz: MDC_MAX_FREQ_HZ,
            promiscuous: false,
            checksum: ChecksumConfig::default(),
            flow_control: FlowControlConfig::default(),
        }
    }
}

impl EmacConfig {
    /// Create a new configuration with defaults
    #[must_use]
    pub const fn new() -> Self {
        Self {
            phy_interface: PhyInterface::Rmii,
            rmii_clock: RmiiClockMode::ExternalInput { gpio: 0 },
            mac_address: DEFAULT_MAC_ADDR,
            dma_burst_len: DmaBurstLen::Burst32,
            sw_reset_timeout_ms: SOFT_RESET_TIMEOUT_MS,
            mdc_freq_hz: MDC_MAX_FREQ_HZ,
            promiscuous: false,
            checksum: ChecksumConfig {
                rx_checksum: false,
                tx_checksum: TxChecksumMode::Disabled,
            },
            flow_control: FlowControlConfig {
                enabled: false,
                low_water_mark: DEFAULT_FLOW_LOW_WATER,
                high_water_mark: DEFAULT_FLOW_HIGH_WATER,
                pause_time: PAUSE_TIME_MAX,
                pause_low_threshold: PauseLowThreshold::Minus4,
                unicast_pause_detect: false,
            },
        }
    }

    /// Create a default configuration for ESP32 RMII with esp-hal defaults.
    ///
    /// This uses RMII with an external 50 MHz clock on GPIO0, which is the
    /// most common ESP32 board configuration.
    ///
    /// # Returns
    ///
    /// Default RMII configuration for ESP32.
    #[must_use]
    pub const fn rmii_esp32_default() -> Self {
        Self::new()
    }

    // =========================================================================
    // Builder Methods
    // =========================================================================

    /// Set the PHY interface type
    #[must_use]
    pub const fn with_phy_interface(mut self, interface: PhyInterface) -> Self {
        self.phy_interface = interface;
        self
    }

    /// Set the RMII clock mode
    #[must_use]
    pub const fn with_rmii_clock(mut self, clock: RmiiClockMode) -> Self {
        self.rmii_clock = clock;
        self
    }

    /// Set the MAC address
    ///
    /// The MAC address should be 6 bytes. If not set, a default locally-administered
    /// address (02:00:00:00:00:01) will be used.
    #[must_use]
    pub const fn with_mac_address(mut self, addr: [u8; 6]) -> Self {
        self.mac_address = addr;
        self
    }

    /// Set the DMA burst length
    #[must_use]
    pub const fn with_dma_burst_len(mut self, burst_len: DmaBurstLen) -> Self {
        self.dma_burst_len = burst_len;
        self
    }

    /// Set the software reset timeout
    #[must_use]
    pub const fn with_reset_timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.sw_reset_timeout_ms = timeout_ms;
        self
    }

    /// Set the MDC clock frequency
    #[must_use]
    pub const fn with_mdc_freq_hz(mut self, freq_hz: u32) -> Self {
        self.mdc_freq_hz = freq_hz;
        self
    }

    /// Enable or disable promiscuous mode
    #[must_use]
    pub const fn with_promiscuous(mut self, enabled: bool) -> Self {
        self.promiscuous = enabled;
        self
    }

    /// Set the checksum offload configuration
    #[must_use]
    pub const fn with_checksum(mut self, checksum: ChecksumConfig) -> Self {
        self.checksum = checksum;
        self
    }

    /// Enable RX checksum offload
    #[must_use]
    pub const fn with_rx_checksum(mut self, enabled: bool) -> Self {
        self.checksum.rx_checksum = enabled;
        self
    }

    /// Set the TX checksum mode
    #[must_use]
    pub const fn with_tx_checksum(mut self, mode: TxChecksumMode) -> Self {
        self.checksum.tx_checksum = mode;
        self
    }

    /// Set the flow control configuration
    #[must_use]
    pub const fn with_flow_control(mut self, flow_control: FlowControlConfig) -> Self {
        self.flow_control = flow_control;
        self
    }

    /// Enable flow control with default settings
    #[must_use]
    pub const fn with_flow_control_enabled(mut self, enabled: bool) -> Self {
        self.flow_control.enabled = enabled;
        self
    }
}

/// EMAC driver state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum State {
    /// Not initialized
    #[default]
    Uninitialized,
    /// Initialized but not started
    Initialized,
    /// Running (TX/RX enabled)
    Running,
    /// Stopped (TX/RX disabled but still initialized)
    Stopped,
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::constants::DEFAULT_MAC_ADDR;

    // =========================================================================
    // Default Value Tests
    // =========================================================================

    #[test]
    fn config_default_values() {
        let config = EmacConfig::new();

        assert_eq!(config.mac_address, DEFAULT_MAC_ADDR);
        assert_eq!(config.phy_interface, PhyInterface::Rmii);
        assert_eq!(config.dma_burst_len, DmaBurstLen::Burst32);
        assert!(!config.promiscuous);
        assert!(!config.checksum.rx_checksum);
        assert_eq!(config.checksum.tx_checksum, TxChecksumMode::Disabled);
        assert!(!config.flow_control.enabled);
    }

    #[test]
    fn config_default_trait_matches_new() {
        let from_default = EmacConfig::default();
        let from_new = EmacConfig::new();

        assert_eq!(from_default.mac_address, from_new.mac_address);
        assert_eq!(from_default.phy_interface, from_new.phy_interface);
        assert_eq!(from_default.dma_burst_len, from_new.dma_burst_len);
    }

    #[test]
    fn config_rmii_esp32_default_matches_new() {
        let from_rmii = EmacConfig::rmii_esp32_default();
        let from_new = EmacConfig::new();

        assert_eq!(from_rmii.mac_address, from_new.mac_address);
        assert_eq!(from_rmii.phy_interface, from_new.phy_interface);
        assert_eq!(from_rmii.rmii_clock, from_new.rmii_clock);
    }

    // =========================================================================
    // Builder Pattern Tests
    // =========================================================================

    #[test]
    fn config_builder_mac_address() {
        let mac = [0x02, 0x00, 0x00, 0x11, 0x22, 0x33];
        let config = EmacConfig::new().with_mac_address(mac);

        assert_eq!(config.mac_address, mac);
    }

    #[test]
    fn config_builder_phy_interface() {
        let config = EmacConfig::new().with_phy_interface(PhyInterface::Mii);
        assert_eq!(config.phy_interface, PhyInterface::Mii);

        let config = EmacConfig::new().with_phy_interface(PhyInterface::Rmii);
        assert_eq!(config.phy_interface, PhyInterface::Rmii);
    }

    #[test]
    fn config_builder_dma_burst_len() {
        let config = EmacConfig::new().with_dma_burst_len(DmaBurstLen::Burst16);
        assert_eq!(config.dma_burst_len, DmaBurstLen::Burst16);

        let config = EmacConfig::new().with_dma_burst_len(DmaBurstLen::Burst1);
        assert_eq!(config.dma_burst_len, DmaBurstLen::Burst1);
    }

    #[test]
    fn config_builder_promiscuous() {
        let config = EmacConfig::new().with_promiscuous(true);
        assert!(config.promiscuous);

        let config = EmacConfig::new().with_promiscuous(false);
        assert!(!config.promiscuous);
    }

    #[test]
    fn config_builder_chaining() {
        let mac = [0x02, 0x00, 0x00, 0xAA, 0xBB, 0xCC];
        let config = EmacConfig::new()
            .with_mac_address(mac)
            .with_phy_interface(PhyInterface::Mii)
            .with_dma_burst_len(DmaBurstLen::Burst8)
            .with_promiscuous(true)
            .with_rx_checksum(true)
            .with_tx_checksum(TxChecksumMode::Full)
            .with_flow_control_enabled(true);

        assert_eq!(config.mac_address, mac);
        assert_eq!(config.phy_interface, PhyInterface::Mii);
        assert_eq!(config.dma_burst_len, DmaBurstLen::Burst8);
        assert!(config.promiscuous);
        assert!(config.checksum.rx_checksum);
        assert_eq!(config.checksum.tx_checksum, TxChecksumMode::Full);
        assert!(config.flow_control.enabled);
    }

    #[test]
    fn config_builder_checksum() {
        let config = EmacConfig::new()
            .with_rx_checksum(true)
            .with_tx_checksum(TxChecksumMode::IpHeaderOnly);

        assert!(config.checksum.rx_checksum);
        assert_eq!(config.checksum.tx_checksum, TxChecksumMode::IpHeaderOnly);
    }

    #[test]
    fn config_builder_rmii_clock() {
        let config = EmacConfig::new().with_rmii_clock(RmiiClockMode::InternalOutput { gpio: 17 });

        match config.rmii_clock {
            RmiiClockMode::InternalOutput { gpio } => assert_eq!(gpio, 17),
            _ => panic!("Expected InternalOutput"),
        }
    }

    // =========================================================================
    // Enum Conversion Tests
    // =========================================================================

    #[test]
    fn dma_burst_len_to_pbl() {
        assert_eq!(DmaBurstLen::Burst1.to_pbl(), 1);
        assert_eq!(DmaBurstLen::Burst2.to_pbl(), 2);
        assert_eq!(DmaBurstLen::Burst4.to_pbl(), 4);
        assert_eq!(DmaBurstLen::Burst8.to_pbl(), 8);
        assert_eq!(DmaBurstLen::Burst16.to_pbl(), 16);
        assert_eq!(DmaBurstLen::Burst32.to_pbl(), 32);
    }

    #[test]
    fn speed_default() {
        assert_eq!(Speed::default(), Speed::Mbps100);
    }

    #[test]
    fn duplex_default() {
        assert_eq!(Duplex::default(), Duplex::Full);
    }

    #[test]
    fn phy_interface_default() {
        assert_eq!(PhyInterface::default(), PhyInterface::Rmii);
    }

    #[test]
    fn state_default() {
        assert_eq!(State::default(), State::Uninitialized);
    }

    // =========================================================================
    // MAC Address Filter Tests
    // =========================================================================

    #[test]
    fn mac_filter_new() {
        let addr = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let filter = MacAddressFilter::new(addr);

        assert_eq!(filter.address, addr);
        assert_eq!(filter.filter_type, MacFilterType::Destination);
        assert_eq!(filter.byte_mask, 0);
    }

    #[test]
    fn mac_filter_source() {
        let addr = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let filter = MacAddressFilter::source(addr);

        assert_eq!(filter.address, addr);
        assert_eq!(filter.filter_type, MacFilterType::Source);
        assert_eq!(filter.byte_mask, 0);
    }

    #[test]
    fn mac_filter_with_mask() {
        let addr = [0x01, 0x00, 0x5E, 0x00, 0x00, 0x00];
        let filter = MacAddressFilter::with_mask(addr, 0b00_0111); // Mask lower 3 bytes

        assert_eq!(filter.address, addr);
        assert_eq!(filter.byte_mask, 0b00_0111);
    }

    // =========================================================================
    // Flow Control Tests
    // =========================================================================

    #[test]
    fn flow_control_default() {
        let fc = FlowControlConfig::default();

        assert!(!fc.enabled);
        assert_eq!(fc.low_water_mark, DEFAULT_FLOW_LOW_WATER);
        assert_eq!(fc.high_water_mark, DEFAULT_FLOW_HIGH_WATER);
        assert_eq!(fc.pause_time, PAUSE_TIME_MAX);
    }

    #[test]
    fn flow_control_with_water_marks() {
        let fc = FlowControlConfig::with_water_marks(2, 8);

        assert!(fc.enabled);
        assert_eq!(fc.low_water_mark, 2);
        assert_eq!(fc.high_water_mark, 8);
    }
}
