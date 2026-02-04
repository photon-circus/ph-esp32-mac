//! Group 1: Register Access Tests
//!
//! Verifies EMAC peripheral registers are accessible and contain reasonable values.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-1-001 | EMAC clock enable | Verify EMAC peripheral clock via DPORT |
//! | IT-1-002 | DMA registers | Verify DMA registers readable |
//! | IT-1-003 | MAC registers | Verify MAC registers readable |
//! | IT-1-004 | Extension registers | Verify extension registers readable |

use log::{error, info};

use super::framework::{TestResult, DMA_BASE, MAC_BASE, DPORT_WIFI_CLK_EN};

/// IT-1-001: Verify EMAC peripheral clock can be enabled via DPORT
pub fn test_emac_clock_enable() -> TestResult {
    let clk_reg = unsafe { core::ptr::read_volatile(DPORT_WIFI_CLK_EN as *const u32) };
    
    if (clk_reg & (1 << 14)) != 0 {
        info!("  DPORT WIFI_CLK_EN={:#010x}, EMAC_EN=1", clk_reg);
        return TestResult::Pass;
    }
    
    // Enable EMAC clock
    info!("  Enabling EMAC clock...");
    unsafe {
        core::ptr::write_volatile(DPORT_WIFI_CLK_EN as *mut u32, clk_reg | (1 << 14));
    }
    
    let clk_after = unsafe { core::ptr::read_volatile(DPORT_WIFI_CLK_EN as *const u32) };
    if (clk_after & (1 << 14)) != 0 {
        info!("  EMAC clock enabled successfully");
        TestResult::Pass
    } else {
        error!("  Failed to enable EMAC clock");
        TestResult::Fail
    }
}

/// IT-1-002: Verify DMA registers are readable and contain reasonable values
pub fn test_dma_registers_accessible() -> TestResult {
    let bus_mode = unsafe { core::ptr::read_volatile(DMA_BASE as *const u32) };
    info!("  DMA BUS_MODE={:#010x}", bus_mode);
    
    if bus_mode == 0x0000_0000 || bus_mode == 0xFFFF_FFFF {
        error!("  DMA registers not accessible");
        TestResult::Fail
    } else {
        TestResult::Pass
    }
}

/// IT-1-003: Verify MAC registers are readable
pub fn test_mac_registers_accessible() -> TestResult {
    let mac_config = unsafe { core::ptr::read_volatile(MAC_BASE as *const u32) };
    let mac_ff = unsafe { core::ptr::read_volatile((MAC_BASE + 4) as *const u32) };
    
    info!("  GMACCONFIG={:#010x}, GMACFF={:#010x}", mac_config, mac_ff);
    
    if mac_config == 0xFFFF_FFFF || mac_ff == 0xFFFF_FFFF {
        error!("  MAC registers not accessible");
        TestResult::Fail
    } else {
        TestResult::Pass
    }
}

/// IT-1-004: Verify extension registers are readable  
pub fn test_extension_registers() -> TestResult {
    let ext_clkout = unsafe { core::ptr::read_volatile((DMA_BASE + 0x800) as *const u32) };
    let ext_phyinf = unsafe { core::ptr::read_volatile((DMA_BASE + 0x80C) as *const u32) };
    
    info!("  EX_CLKOUT_CONF={:#010x}, EX_PHYINF_CONF={:#010x}", ext_clkout, ext_phyinf);
    
    if ext_clkout == 0xFFFF_FFFF || ext_phyinf == 0xFFFF_FFFF {
        error!("  Extension registers not accessible");
        TestResult::Fail
    } else {
        TestResult::Pass
    }
}
