//! Named QA-only snapshots of ESP32 EMAC registers.

use ph_esp32_mac::unsafe_registers::{DmaRegs, ExtRegs, MacRegs};

const DPORT_WIFI_CLK_EN_REG: usize = 0x3FF0_00CC;
const DPORT_CORE_RST_EN_REG: usize = 0x3FF0_00D0;
const DPORT_EMAC_CLOCK_BIT: u32 = 1 << 14;
const DPORT_EMAC_RESET_BIT: u32 = 1 << 7;

const GPIO18_IOMUX_REG: usize = 0x3FF4_9070;
const GPIO18_OUT_SEL_REG: usize = 0x3FF4_4578;
const EMAC_MDI_IN_SEL_REG: usize = 0x3FF4_4454;

const MAC_BASE: usize = 0x3FF6_A000;
const MAC_ADDITIONAL_ADDRESS_HIGH: [usize; 4] = [0x48, 0x50, 0x58, 0x60];
const MAC_ADDITIONAL_ADDRESS_LOW: [usize; 4] = [0x4c, 0x54, 0x5c, 0x64];
const MAC_ADDRESS_ENABLE: u32 = 1 << 31;

const DMA_RX_DESCRIPTOR_BASE_REG: usize = 0x3FF6_900C;
const RX_DESCRIPTOR_STRIDE: usize = 32;
const RX_DESCRIPTOR_OWN: u32 = 1 << 31;
const RX_DESCRIPTOR_COUNT: usize = 4;

/// System and EMAC extension clock/reset state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RccSnapshot {
    /// Raw DPORT peripheral clock-enable value.
    pub peripheral_clock_enable: u32,
    /// Raw DPORT core-reset-enable value.
    pub peripheral_reset_enable: u32,
    /// Whether the DPORT EMAC peripheral clock is enabled.
    pub emac_clock_enabled: bool,
    /// Whether the DPORT EMAC peripheral reset remains asserted.
    pub emac_reset_asserted: bool,
    /// Whether extension-register fields were safe to sample.
    pub extension_valid: bool,
    /// Raw EMAC extension clock-control value.
    pub extension_clock_control: u32,
    /// Raw EMAC PHY-interface configuration.
    pub phy_interface_config: u32,
    /// Raw EMAC RAM power-down selection.
    pub power_down_select: u32,
}

impl RccSnapshot {
    /// Capture DPORT state and, when the EMAC clock is enabled, extension state.
    pub fn capture() -> Self {
        // SAFETY: These fixed addresses are the ESP32 DPORT clock/reset
        // registers and are read with volatile semantics only.
        let (clock, reset) = unsafe {
            (
                core::ptr::read_volatile(DPORT_WIFI_CLK_EN_REG as *const u32),
                core::ptr::read_volatile(DPORT_CORE_RST_EN_REG as *const u32),
            )
        };
        let emac_clock_enabled = (clock & DPORT_EMAC_CLOCK_BIT) != 0;
        let emac_reset_asserted = (reset & DPORT_EMAC_RESET_BIT) != 0;
        let extension_valid = emac_clock_enabled && !emac_reset_asserted;

        let (extension_clock_control, phy_interface_config, power_down_select) = if extension_valid
        {
            (
                ExtRegs::clk_ctrl(),
                ExtRegs::phy_inf_conf(),
                ExtRegs::pd_sel(),
            )
        } else {
            (0, 0, 0)
        };

        Self {
            peripheral_clock_enable: clock,
            peripheral_reset_enable: reset,
            emac_clock_enabled,
            emac_reset_asserted,
            extension_valid,
            extension_clock_control,
            phy_interface_config,
            power_down_select,
        }
    }
}

/// GPIO-matrix and SMI register state relevant to MDIO direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MdioSnapshot {
    /// GPIO18 IO_MUX configuration.
    pub gpio18_iomux: u32,
    /// GPIO18 output-matrix selection and output-enable ownership.
    pub gpio18_output_select: u32,
    /// EMAC MDI input-matrix selection.
    pub mdi_input_select: u32,
    /// Raw MAC MII address/control register.
    pub mii_address: u32,
    /// Raw MAC MII data register.
    pub mii_data: u32,
}

impl MdioSnapshot {
    /// Capture MDIO pin routing and MAC SMI registers.
    pub fn capture() -> Self {
        // SAFETY: These fixed addresses are ESP32 IO_MUX/GPIO Matrix registers
        // and are read with volatile semantics only.
        let (gpio18_iomux, gpio18_output_select, mdi_input_select) = unsafe {
            (
                core::ptr::read_volatile(GPIO18_IOMUX_REG as *const u32),
                core::ptr::read_volatile(GPIO18_OUT_SEL_REG as *const u32),
                core::ptr::read_volatile(EMAC_MDI_IN_SEL_REG as *const u32),
            )
        };

        Self {
            gpio18_iomux,
            gpio18_output_select,
            mdi_input_select,
            mii_address: MacRegs::mii_address(),
            mii_data: MacRegs::mii_data(),
        }
    }
}

/// Primary MAC address and receive-filter configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacSnapshot {
    /// Decoded primary MAC address.
    pub address: [u8; 6],
    /// Raw primary MAC address high register.
    pub address_high: u32,
    /// Raw primary MAC address low register.
    pub address_low: u32,
    /// Raw MAC configuration register.
    pub config: u32,
    /// Raw frame-filter register.
    pub frame_filter: u32,
    /// Raw high words for additional address-filter slots 1 through 4.
    pub additional_address_high: [u32; 4],
    /// Raw low words for additional address-filter slots 1 through 4.
    pub additional_address_low: [u32; 4],
    /// Number of enabled additional address-filter slots.
    pub enabled_additional_filters: u8,
}

impl MacSnapshot {
    /// Capture the primary address and filtering state.
    pub fn capture() -> Self {
        let mut additional_address_high = [0u32; 4];
        let mut additional_address_low = [0u32; 4];
        let mut enabled_additional_filters = 0u8;

        for index in 0..additional_address_high.len() {
            // SAFETY: These offsets select the four documented additional
            // address-filter register pairs in the ESP32 EMAC MAC block.
            let (high, low) = unsafe {
                (
                    core::ptr::read_volatile(
                        (MAC_BASE + MAC_ADDITIONAL_ADDRESS_HIGH[index]) as *const u32,
                    ),
                    core::ptr::read_volatile(
                        (MAC_BASE + MAC_ADDITIONAL_ADDRESS_LOW[index]) as *const u32,
                    ),
                )
            };
            additional_address_high[index] = high;
            additional_address_low[index] = low;
            enabled_additional_filters += u8::from((high & MAC_ADDRESS_ENABLE) != 0);
        }

        Self {
            address: MacRegs::get_mac_address(),
            address_high: MacRegs::mac_addr0_high(),
            address_low: MacRegs::mac_addr0_low(),
            config: MacRegs::config(),
            frame_filter: MacRegs::frame_filter(),
            additional_address_high,
            additional_address_low,
            enabled_additional_filters,
        }
    }

    /// Return whether the primary address exactly matches its hardware encoding.
    #[must_use]
    pub fn primary_address_matches(self, expected: [u8; 6]) -> bool {
        let low = (expected[0] as u32)
            | ((expected[1] as u32) << 8)
            | ((expected[2] as u32) << 16)
            | ((expected[3] as u32) << 24);
        let high = (expected[4] as u32) | ((expected[5] as u32) << 8) | MAC_ADDRESS_ENABLE;
        self.address == expected && self.address_high == high && self.address_low == low
    }

    /// Return whether one additional slot is an exact destination-address filter.
    ///
    /// `slot` is one-based and must be in the range 1 through 4. Exact matching
    /// also proves that source-address selection and byte masks are disabled.
    #[must_use]
    pub fn additional_destination_filter_matches(self, slot: usize, expected: [u8; 6]) -> bool {
        if slot == 0 || slot > 4 {
            return false;
        }
        let index = slot - 1;
        let low = (expected[0] as u32)
            | ((expected[1] as u32) << 8)
            | ((expected[2] as u32) << 16)
            | ((expected[3] as u32) << 24);
        let high = (expected[4] as u32) | ((expected[5] as u32) << 8) | MAC_ADDRESS_ENABLE;
        self.additional_address_high[index] == high && self.additional_address_low[index] == low
    }
}

/// DMA receive state and ownership of the QA runner's four descriptors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxSnapshot {
    /// Raw DMA status register.
    pub status: u32,
    /// Raw DMA interrupt-enable register.
    pub interrupt_enable: u32,
    /// Receive process state decoded from the DMA status register.
    pub process_state: u8,
    /// Whether receive-buffer-unavailable is pending.
    pub buffer_unavailable: bool,
    /// Whether the receive-buffer-unavailable interrupt is enabled.
    pub buffer_unavailable_interrupt_enabled: bool,
    /// Whether the abnormal-summary interrupt is enabled.
    pub abnormal_summary_interrupt_enabled: bool,
    /// Current hardware RX descriptor pointer.
    pub current_descriptor: u32,
    /// Current hardware RX buffer pointer.
    pub current_buffer: u32,
    /// Configured RX descriptor-list base.
    pub descriptor_base: u32,
    /// Whether descriptor ownership was sampled from a valid SRAM address.
    pub descriptor_sample_valid: bool,
    /// Number of descriptors currently owned by DMA.
    pub dma_owned_descriptors: u8,
}

impl RxSnapshot {
    /// Capture DMA receive status and the fixed four-entry QA descriptor ring.
    pub fn capture() -> Self {
        let status = DmaRegs::status();
        let interrupt_enable = DmaRegs::interrupt_enable();

        // SAFETY: This fixed address is the ESP32 DMA RX descriptor-list base
        // register and is read with volatile semantics only.
        let descriptor_base =
            unsafe { core::ptr::read_volatile(DMA_RX_DESCRIPTOR_BASE_REG as *const u32) };
        let descriptor_sample_valid = (0x3FFB_0000..0x4000_0000).contains(&descriptor_base);
        let mut dma_owned_descriptors = 0u8;

        if descriptor_sample_valid {
            for index in 0..RX_DESCRIPTOR_COUNT {
                let address = descriptor_base as usize + index * RX_DESCRIPTOR_STRIDE;
                // SAFETY: A validated DMA descriptor-list base points to the
                // runner's four 32-byte descriptors in internal SRAM. Word zero
                // is atomically sampled with volatile semantics while DMA runs.
                let word0 = unsafe { core::ptr::read_volatile(address as *const u32) };
                if (word0 & RX_DESCRIPTOR_OWN) != 0 {
                    dma_owned_descriptors += 1;
                }
            }
        }

        Self {
            status,
            interrupt_enable,
            process_state: ((status >> 17) & 0x7) as u8,
            buffer_unavailable: (status & (1 << 7)) != 0,
            buffer_unavailable_interrupt_enabled: (interrupt_enable & (1 << 7)) != 0,
            abnormal_summary_interrupt_enabled: (interrupt_enable & (1 << 15)) != 0,
            current_descriptor: DmaRegs::current_rx_desc(),
            current_buffer: DmaRegs::current_rx_buffer(),
            descriptor_base,
            descriptor_sample_valid,
            dma_owned_descriptors,
        }
    }
}
