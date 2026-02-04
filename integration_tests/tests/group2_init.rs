//! Group 2: EMAC Initialization Tests
//!
//! Verifies driver initialization and hardware configuration.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-2-001 | EMAC init | Initialize EMAC with board config |
//! | IT-2-002 | RMII pin config | Verify GPIO MUX settings |
//! | IT-2-003 | DMA descriptor chain | Verify descriptor linkage |

use log::{error, info};

use ph_esp32_mac::{Emac, EmacConfig, PhyInterface, RmiiClockMode};

use super::framework::{TestContext, TestResult, EMAC, IO_MUX_BASE, DMA_BASE};
use crate::boards::wt32_eth01::Wt32Eth01Config as Board;

/// IT-2-001: Test EMAC initialization with board-specific configuration
pub fn test_emac_init(ctx: &mut TestContext) -> TestResult {
    let config = EmacConfig::new()
        .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(RmiiClockMode::ExternalInput { 
            gpio: Board::REF_CLK_GPIO 
        });

    // Place EMAC in static location BEFORE init (required for DMA descriptors)
    critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs).replace(Emac::new());
    });

    // Initialize in-place
    let result = critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            let mut delay = esp_hal::delay::Delay::new();
            match emac.init(config, &mut delay) {
                Ok(()) => {
                    info!("  EMAC initialized");
                    ctx.emac_initialized = true;
                    TestResult::Pass
                }
                Err(e) => {
                    error!("  EMAC init failed: {:?}", e);
                    TestResult::Fail
                }
            }
        } else {
            error!("  EMAC static unavailable");
            TestResult::Fail
        }
    });

    result
}

/// IT-2-002: Verify RMII pins are correctly configured via IO_MUX
pub fn test_rmii_pins() -> TestResult {
    let pins = [
        ("GPIO0  REF_CLK", 0x44u32, true),
        ("GPIO19 TXD0", 0x74, false),
        ("GPIO21 TX_EN", 0x7C, false),
        ("GPIO22 TXD1", 0x80, false),
        ("GPIO25 RXD0", 0x24, true),
        ("GPIO26 RXD1", 0x28, true),
        ("GPIO27 CRS_DV", 0x2C, true),
    ];

    let mut all_ok = true;

    for (name, offset, is_input) in pins {
        let reg = unsafe { core::ptr::read_volatile((IO_MUX_BASE + offset) as *const u32) };
        let mcu_sel = (reg >> 12) & 0x7;
        let fun_ie = (reg >> 9) & 0x1;
        
        let ok = mcu_sel == 5 && (!is_input || fun_ie == 1);
        
        if ok {
            info!("  {} MCU_SEL=5 {}", name, if is_input { "FUN_IE=1" } else { "" });
        } else {
            error!("  {} MCU_SEL={} FUN_IE={} EXPECTED MCU_SEL=5", name, mcu_sel, fun_ie);
            all_ok = false;
        }
    }

    if all_ok { TestResult::Pass } else { TestResult::Fail }
}

/// IT-2-003: Verify DMA descriptor chain is correctly linked
pub fn test_dma_descriptor_chain() -> TestResult {
    let rx_base = unsafe { core::ptr::read_volatile((DMA_BASE + 0x0C) as *const u32) };
    let tx_base = unsafe { core::ptr::read_volatile((DMA_BASE + 0x10) as *const u32) };
    
    // Split address printing to avoid espflash addr2line symbolication
    info!("  RX desc base=0x{:04X}_{:04X}, TX desc base=0x{:04X}_{:04X}", 
          (rx_base >> 16) & 0xFFFF, rx_base & 0xFFFF,
          (tx_base >> 16) & 0xFFFF, tx_base & 0xFFFF);
    
    // Verify addresses are in SRAM region
    let sram_ok = |addr: u32| addr >= 0x3FFB_0000 && addr < 0x4000_0000;
    
    if !sram_ok(rx_base) || !sram_ok(tx_base) {
        error!("  Descriptor bases not in SRAM region");
        return TestResult::Fail;
    }
    
    // Check descriptor chain linkage (4 descriptors, 32 bytes each)
    for i in 0..4u32 {
        let desc_addr = rx_base + (i * 32);
        let rdes0 = unsafe { core::ptr::read_volatile(desc_addr as *const u32) };
        let rdes3 = unsafe { core::ptr::read_volatile((desc_addr + 12) as *const u32) };
        
        let expected_next = if i == 3 { rx_base } else { rx_base + ((i + 1) * 32) };
        let dma_owns = (rdes0 & 0x8000_0000) != 0;
        
        if rdes3 != expected_next || !dma_owns {
            error!("  Desc[{}] NEXT=0x{:04X}_{:04X} expected 0x{:04X}_{:04X}, OWN={}", 
                   i, 
                   (rdes3 >> 16) & 0xFFFF, rdes3 & 0xFFFF,
                   (expected_next >> 16) & 0xFFFF, expected_next & 0xFFFF,
                   if dma_owns { 1 } else { 0 });
            return TestResult::Fail;
        }
    }
    
    info!("  Descriptor chain valid and loops correctly");
    TestResult::Pass
}
