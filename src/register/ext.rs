//! Extension Register Definitions
//!
//! ESP32-specific extension registers for clock, GPIO, and power management.
//!
//! Register layout from ESP-IDF emac_ext_struct.h:
//! - ex_clkout_conf (0x00): Clock output configuration (dividers, delay)
//! - ex_oscclk_conf (0x04): Oscillator clock config (10M/100M dividers, clk_sel)
//! - ex_clk_ctrl    (0x08): Clock control (ext_en, int_en, mii_clk enables)
//! - ex_phyinf_conf (0x0C): PHY interface config (phy_intf_sel, etc)
//! - pd_sel         (0x10): Power down select (RAM power down)

use super::{
    DPORT_WIFI_CLK_EMAC_EN, DPORT_WIFI_CLK_EN_REG, EXT_BASE, IO_MUX_BASE, IO_MUX_FUN_IE,
    IO_MUX_GPIO0_FUNC_EMAC_TX_CLK, IO_MUX_GPIO0_OFFSET, IO_MUX_MCU_SEL_MASK, IO_MUX_MCU_SEL_SHIFT,
    read_reg, reg_ro, reg_rw, write_reg,
};

// =============================================================================
// Register Offsets (from emac_ext_struct.h)
// =============================================================================

/// Clock output configuration register offset
pub const EX_CLKOUT_CONF_OFFSET: usize = 0x00;
/// Oscillator clock configuration register offset
pub const EX_OSCCLK_CONF_OFFSET: usize = 0x04;
/// Clock control register offset
pub const EX_CLK_CTRL_OFFSET: usize = 0x08;
/// PHY interface configuration register offset
pub const EX_PHYINF_CONF_OFFSET: usize = 0x0C;
/// Power down select register offset
pub const EX_PD_SEL_OFFSET: usize = 0x10;
/// Date register offset (version info)
pub const EX_DATE_OFFSET: usize = 0xFC;

/// Legacy alias for backwards compatibility
pub const EX_RAM_PD_OFFSET: usize = EX_PD_SEL_OFFSET;

// =============================================================================
// Clock Output Configuration (EX_CLKOUT_CONF @ 0x00)
// =============================================================================

/// Clock output divider number (bits 3:0)
pub const EX_CLKOUT_DIV_NUM_MASK: u32 = 0x0F;
/// Clock output half-period divider shift (bits 7:4)
pub const EX_CLKOUT_H_DIV_NUM_SHIFT: u32 = 4;
/// Clock output half-period divider mask (bits 7:4)
pub const EX_CLKOUT_H_DIV_NUM_MASK: u32 = 0xF0;
/// Delay number shift (bits 9:8)
pub const EX_CLKOUT_DLY_NUM_SHIFT: u32 = 8;
/// Delay number mask (bits 9:8)
pub const EX_CLKOUT_DLY_NUM_MASK: u32 = 0x300;

// =============================================================================
// Oscillator Clock Configuration (EX_OSCCLK_CONF @ 0x04)
// =============================================================================

/// 10M divider number (bits 5:0)
pub const EX_OSCCLK_DIV_NUM_10M_MASK: u32 = 0x3F;
/// 10M half-period divider (bits 11:6)
pub const EX_OSCCLK_H_DIV_NUM_10M_SHIFT: u32 = 6;
/// 100M divider number (bits 17:12)
pub const EX_OSCCLK_DIV_NUM_100M_SHIFT: u32 = 12;
/// 100M half-period divider (bits 23:18)
pub const EX_OSCCLK_H_DIV_NUM_100M_SHIFT: u32 = 18;
/// Clock source select (bit 24): 0 = internal, 1 = external
pub const EX_OSCCLK_CLK_SEL: u32 = 1 << 24;

// =============================================================================
// Clock Control Register (EX_CLK_CTRL @ 0x08)
// =============================================================================

/// External clock enable (bit 0) - enable external 50MHz clock input
pub const EX_CLK_EXT_EN: u32 = 1 << 0;
/// Internal clock enable (bit 1) - enable internal clock generation from APLL
pub const EX_CLK_INT_EN: u32 = 1 << 1;
/// RX 125 clock enable (bit 2) - for gigabit mode
pub const EX_CLK_RX_125_CLK_EN: u32 = 1 << 2;
/// MII TX clock enable (bit 3)
pub const EX_CLK_MII_CLK_TX_EN: u32 = 1 << 3;
/// MII RX clock enable (bit 4)
pub const EX_CLK_MII_CLK_RX_EN: u32 = 1 << 4;
/// Main clock enable (bit 5)
pub const EX_CLK_EN: u32 = 1 << 5;

// =============================================================================
// PHY Interface Configuration (EX_PHYINF_CONF @ 0x0C)
// =============================================================================

/// Internal RevMII RX clock select (bit 0)
pub const EX_PHYINF_INT_REVMII_RX_CLK_SEL: u32 = 1 << 0;
/// External RevMII RX clock select (bit 1)
pub const EX_PHYINF_EXT_REVMII_RX_CLK_SEL: u32 = 1 << 1;
/// SBD flow control enable (bit 2)
pub const EX_PHYINF_SBD_FLOWCTRL: u32 = 1 << 2;
/// Core PHY address shift (bits 7:3)
pub const EX_PHYINF_CORE_PHY_ADDR_SHIFT: u32 = 3;
/// Core PHY address mask (bits 7:3)
pub const EX_PHYINF_CORE_PHY_ADDR_MASK: u32 = 0x1F << 3;
/// RevMII PHY address shift (bits 12:8)
pub const EX_PHYINF_REVMII_PHY_ADDR_SHIFT: u32 = 8;
/// RevMII PHY address mask (bits 12:8)
pub const EX_PHYINF_REVMII_PHY_ADDR_MASK: u32 = 0x1F << 8;
/// PHY interface select shift (bits 15:13): 0=MII, 4=RMII
pub const EX_PHYINF_PHY_INTF_SEL_SHIFT: u32 = 13;
/// PHY interface select mask (bits 15:13)
pub const EX_PHYINF_PHY_INTF_SEL_MASK: u32 = 0x7 << 13;
/// PHY interface select value for MII mode
pub const EX_PHYINF_PHY_INTF_MII: u32 = 0;
/// PHY interface select value for RMII mode
pub const EX_PHYINF_PHY_INTF_RMII: u32 = 4;

// =============================================================================
// Power Down Select (EX_PD_SEL @ 0x10)
// =============================================================================

/// RAM power down enable (bits 1:0)
pub const EX_PD_SEL_RAM_PD_EN_MASK: u32 = 0x03;

// =============================================================================
// Extension Register Access Functions
// =============================================================================

/// Extension Register block for type-safe access
pub struct ExtRegs;

impl ExtRegs {
    /// Get the base address
    #[inline(always)]
    pub const fn base() -> usize {
        EXT_BASE
    }

    // -------------------------------------------------------------------------
    // Register accessors (generated by macros)
    // -------------------------------------------------------------------------

    reg_rw!(
        osc_clk_conf,
        set_osc_clk_conf,
        EXT_BASE,
        EX_OSCCLK_CONF_OFFSET,
        "Oscillator Clock Configuration"
    );
    reg_rw!(
        clk_ctrl,
        set_clk_ctrl,
        EXT_BASE,
        EX_CLK_CTRL_OFFSET,
        "Clock Control register"
    );
    reg_rw!(
        phy_inf_conf,
        set_phy_inf_conf,
        EXT_BASE,
        EX_PHYINF_CONF_OFFSET,
        "PHY Interface Configuration"
    );
    reg_rw!(
        pd_sel,
        set_pd_sel,
        EXT_BASE,
        EX_PD_SEL_OFFSET,
        "Power Down Select register"
    );
    reg_rw!(
        ram_pd,
        set_ram_pd,
        EXT_BASE,
        EX_RAM_PD_OFFSET,
        "RAM Power Down register"
    );
    reg_ro!(
        date,
        EXT_BASE,
        EX_DATE_OFFSET,
        "Date register (version info)"
    );

    // -------------------------------------------------------------------------
    // Clock control helpers
    // -------------------------------------------------------------------------

    /// Enable EMAC peripheral clock at system level (DPORT)
    ///
    /// This MUST be called before accessing any EMAC registers.
    /// Without this, the EMAC peripheral is not clocked and register
    /// access will fail or return garbage.
    #[inline(always)]
    pub fn enable_peripheral_clock() {
        unsafe {
            let current = read_reg(DPORT_WIFI_CLK_EN_REG);

            #[cfg(feature = "defmt")]
            defmt::debug!(
                "DPORT WIFI_CLK_EN before: {:#010x} (EMAC_EN bit 14 = {})",
                current,
                (current >> 14) & 1
            );

            let new_val = current | DPORT_WIFI_CLK_EMAC_EN;
            write_reg(DPORT_WIFI_CLK_EN_REG, new_val);

            #[cfg(feature = "defmt")]
            {
                let readback = read_reg(DPORT_WIFI_CLK_EN_REG);
                defmt::debug!(
                    "DPORT WIFI_CLK_EN after: {:#010x} (EMAC_EN bit 14 = {})",
                    readback,
                    (readback >> 14) & 1
                );
            }
        }
    }

    /// Enable EMAC clocks (extension register clocks)
    ///
    /// Note: `enable_peripheral_clock()` must be called first to enable
    /// the EMAC peripheral at the system level.
    #[inline(always)]
    pub fn enable_clocks() {
        unsafe {
            let ctrl = read_reg(EXT_BASE + EX_CLK_CTRL_OFFSET);
            write_reg(
                EXT_BASE + EX_CLK_CTRL_OFFSET,
                ctrl | EX_CLK_MII_CLK_RX_EN | EX_CLK_MII_CLK_TX_EN | EX_CLK_EN,
            );
        }
    }

    /// Disable EMAC clocks
    #[inline(always)]
    pub fn disable_clocks() {
        unsafe {
            let ctrl = read_reg(EXT_BASE + EX_CLK_CTRL_OFFSET);
            write_reg(
                EXT_BASE + EX_CLK_CTRL_OFFSET,
                ctrl & !(EX_CLK_MII_CLK_RX_EN | EX_CLK_MII_CLK_TX_EN | EX_CLK_EN),
            );
        }
    }

    // -------------------------------------------------------------------------
    // PHY interface helpers
    // -------------------------------------------------------------------------

    /// Configure for RMII mode
    ///
    /// Sets phy_intf_sel = 4 (RMII mode)
    #[inline(always)]
    pub fn set_rmii_mode() {
        unsafe {
            let conf = read_reg(EXT_BASE + EX_PHYINF_CONF_OFFSET);
            // Clear phy_intf_sel bits and set to 4 (RMII)
            let new_val = (conf & !EX_PHYINF_PHY_INTF_SEL_MASK)
                | (EX_PHYINF_PHY_INTF_RMII << EX_PHYINF_PHY_INTF_SEL_SHIFT);
            write_reg(EXT_BASE + EX_PHYINF_CONF_OFFSET, new_val);

            #[cfg(feature = "defmt")]
            defmt::debug!("EX_PHYINF_CONF set RMII mode: {:#010x}", new_val);
        }
    }

    /// Configure for MII mode
    ///
    /// Sets phy_intf_sel = 0 (MII mode)
    #[inline(always)]
    pub fn set_mii_mode() {
        unsafe {
            let conf = read_reg(EXT_BASE + EX_PHYINF_CONF_OFFSET);
            // Clear phy_intf_sel bits (MII = 0)
            let new_val = conf & !EX_PHYINF_PHY_INTF_SEL_MASK;
            write_reg(EXT_BASE + EX_PHYINF_CONF_OFFSET, new_val);
        }
    }

    /// Set RMII clock to external source (external oscillator on GPIO0)
    ///
    /// Configures EX_CLK_CTRL and EX_OSCCLK_CONF for external clock input:
    /// - ext_en = 1, int_en = 0 (use external clock)
    /// - clk_sel = 1 (select external clock)
    #[inline(always)]
    pub fn set_rmii_clock_external() {
        unsafe {
            // Configure clock control: enable external, disable internal
            let ctrl = read_reg(EXT_BASE + EX_CLK_CTRL_OFFSET);
            let new_ctrl = (ctrl | EX_CLK_EXT_EN) & !EX_CLK_INT_EN;
            write_reg(EXT_BASE + EX_CLK_CTRL_OFFSET, new_ctrl);

            // Configure oscillator clock: select external clock source
            let osc = read_reg(EXT_BASE + EX_OSCCLK_CONF_OFFSET);
            let new_osc = osc | EX_OSCCLK_CLK_SEL; // clk_sel = 1 for external
            write_reg(EXT_BASE + EX_OSCCLK_CONF_OFFSET, new_osc);

            #[cfg(feature = "defmt")]
            defmt::debug!(
                "RMII external clock: CLK_CTRL={:#010x} OSCCLK_CONF={:#010x}",
                new_ctrl,
                new_osc
            );
        }
    }

    /// Configure GPIO0 as RMII reference clock input via IO_MUX
    ///
    /// This configures GPIO0 to use function 5 (EMAC_TX_CLK) which routes
    /// the external 50MHz reference clock to the EMAC peripheral.
    ///
    /// IMPORTANT: This MUST be called BEFORE enabling EMAC clocks or attempting
    /// a DMA reset, as the DMA reset requires a working reference clock to complete.
    ///
    /// For WT32-ETH01 and similar boards with an external oscillator providing
    /// the 50MHz clock on GPIO0, this function enables the clock input path.
    #[inline(always)]
    pub fn configure_gpio0_rmii_clock_input() {
        unsafe {
            let addr = IO_MUX_BASE + IO_MUX_GPIO0_OFFSET;
            let current = read_reg(addr);

            // Clear MCU_SEL field and set to function 5 (EMAC_TX_CLK)
            // Also set FUN_IE (input enable) to allow clock signal input
            let new_val = (current & !IO_MUX_MCU_SEL_MASK)
                | (IO_MUX_GPIO0_FUNC_EMAC_TX_CLK << IO_MUX_MCU_SEL_SHIFT)
                | IO_MUX_FUN_IE;

            #[cfg(feature = "defmt")]
            defmt::debug!(
                "GPIO0 IO_MUX: addr={:#010x} before={:#010x} after={:#010x}",
                addr,
                current,
                new_val
            );

            write_reg(addr, new_val);

            // Verify the write
            #[cfg(feature = "defmt")]
            {
                let readback = read_reg(addr);
                defmt::debug!("GPIO0 IO_MUX readback: {:#010x}", readback);
            }
        }
    }

    /// Set RMII clock to internal source with output
    ///
    /// Configures EX_CLK_CTRL, EX_OSCCLK_CONF and EX_CLKOUT_CONF for internal clock generation:
    /// - ext_en = 0, int_en = 1 (use internal clock from APLL)
    /// - clk_sel = 0 (select internal clock)
    /// - div_num = 0, h_div_num = 0 (no dividers)
    ///
    /// Note: The internal clock comes from the ESP32's APLL. For this to work,
    /// the APLL must be properly configured (typically done by esp-hal or esp-idf).
    #[inline(always)]
    pub fn set_rmii_clock_internal() {
        unsafe {
            // Configure clock control: disable external, enable internal
            let ctrl = read_reg(EXT_BASE + EX_CLK_CTRL_OFFSET);
            let new_ctrl = (ctrl | EX_CLK_INT_EN) & !EX_CLK_EXT_EN;
            write_reg(EXT_BASE + EX_CLK_CTRL_OFFSET, new_ctrl);

            // Configure oscillator clock: select internal clock source
            let osc = read_reg(EXT_BASE + EX_OSCCLK_CONF_OFFSET);
            let new_osc = osc & !EX_OSCCLK_CLK_SEL; // clk_sel = 0 for internal
            write_reg(EXT_BASE + EX_OSCCLK_CONF_OFFSET, new_osc);

            // Configure clock output: no dividers
            let clkout = read_reg(EXT_BASE + EX_CLKOUT_CONF_OFFSET);
            let new_clkout = clkout & !(EX_CLKOUT_DIV_NUM_MASK | EX_CLKOUT_H_DIV_NUM_MASK);
            write_reg(EXT_BASE + EX_CLKOUT_CONF_OFFSET, new_clkout);

            #[cfg(feature = "defmt")]
            defmt::debug!(
                "RMII internal clock: CLK_CTRL={:#010x} OSCCLK_CONF={:#010x} CLKOUT_CONF={:#010x}",
                new_ctrl,
                new_osc,
                new_clkout
            );
        }
    }

    // -------------------------------------------------------------------------
    // Power management helpers
    // -------------------------------------------------------------------------

    /// Power up EMAC RAM
    #[inline(always)]
    pub fn power_up_ram() {
        Self::set_ram_pd(0);
    }

    /// Power down EMAC RAM
    #[inline(always)]
    pub fn power_down_ram() {
        Self::set_ram_pd(0xFFFF_FFFF);
    }
}
