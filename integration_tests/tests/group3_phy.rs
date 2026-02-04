//! Group 3: PHY Communication Tests
//!
//! Validates MDIO bus and LAN8720A PHY functionality.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-3-001 | PHY MDIO read | Verify PHY ID via MDIO |
//! | IT-3-002 | PHY init | Initialize PHY with auto-neg |
//! | IT-3-003 | PHY link up | Wait for link up with timeout |

use log::{error, info, warn};

use ph_esp32_mac::{Duplex, Speed};

use super::framework::{TestContext, TestResult, EMAC};
use crate::boards::wt32_eth01::Wt32Eth01Config as Board;

/// IT-3-001: Verify PHY responds to MDIO read operations
pub fn test_phy_mdio_read(ctx: &mut TestContext) -> TestResult {
    use ph_esp32_mac::PhyDriver;
    
    match ctx.phy.phy_id(&mut ctx.mdio) {
        Ok(phy_id) => {
            info!("  PHY ID={:#010x}", phy_id);
            
            if phy_id & Board::PHY_ID_MASK == Board::PHY_ID {
                info!("  Confirmed: {}", Board::PHY_TYPE);
            } else {
                warn!("  Unexpected PHY ID (expected {})", Board::PHY_TYPE);
            }
            TestResult::Pass
        }
        Err(e) => {
            error!("  MDIO read failed: {:?}", e);
            TestResult::Fail
        }
    }
}

/// IT-3-002: Test PHY initialization
pub fn test_phy_init(ctx: &mut TestContext) -> TestResult {
    use ph_esp32_mac::PhyDriver;
    
    match ctx.phy.init(&mut ctx.mdio) {
        Ok(()) => {
            info!("  PHY initialized");
            TestResult::Pass
        }
        Err(e) => {
            error!("  PHY init failed: {:?}", e);
            TestResult::Fail
        }
    }
}

/// IT-3-003: Test PHY link detection (requires cable connected)
pub fn test_phy_link_up(ctx: &mut TestContext, timeout_ms: u32) -> TestResult {
    use ph_esp32_mac::PhyDriver;
    
    info!("  Waiting for link (max {}ms)...", timeout_ms);
    
    let delay = esp_hal::delay::Delay::new();
    let iterations = timeout_ms / 100;
    
    for i in 0..iterations {
        match ctx.phy.poll_link(&mut ctx.mdio) {
            Ok(Some(status)) => {
                ctx.link_speed = status.speed;
                ctx.link_duplex = status.duplex;
                ctx.link_up = true;
                
                let speed = match status.speed { 
                    Speed::Mbps10 => "10Mbps", 
                    Speed::Mbps100 => "100Mbps" 
                };
                let duplex = match status.duplex { 
                    Duplex::Half => "Half", 
                    Duplex::Full => "Full" 
                };
                
                info!("  Link UP: {} {} ({}ms)", speed, duplex, (i + 1) * 100);
                
                // Configure MAC with negotiated parameters
                critical_section::with(|cs| {
                    if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                        emac.set_speed(status.speed);
                        emac.set_duplex(status.duplex);
                    }
                });
                
                return TestResult::Pass;
            }
            Ok(None) => {
                delay.delay_millis(100);
            }
            Err(e) => {
                error!("  Link poll error: {:?}", e);
                return TestResult::Fail;
            }
        }
    }
    
    warn!("  Link timeout - is cable connected?");
    TestResult::Skip
}
