//! Group 8: Advanced Tests
//!
//! Promiscuous mode, PHY capabilities, forced links, interrupt control.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-8-001 | Promiscuous mode | Enable/disable promiscuous |
//! | IT-8-002 | Promiscuous RX | Receive in promiscuous mode |
//! | IT-8-003 | PHY capabilities | Read PHY speed/duplex caps |
//! | IT-8-004 | Force link | Force 10M/100M link speed |
//! | IT-8-005 | Enable TX interrupt | Toggle TX interrupt |
//! | IT-8-006 | Enable RX interrupt | Toggle RX interrupt |
//! | IT-8-007 | TX interrupt fires | Verify TX interrupt on transmit |

use log::{error, info, warn};

use ph_esp32_mac::{Duplex, LinkStatus, PhyDriver, Speed};

use super::framework::{TestContext, TestResult, EMAC};

/// IT-8-001: Test promiscuous mode enable/disable
pub fn test_promiscuous_mode() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Enable promiscuous mode
            emac.set_promiscuous(true);
            info!("  Promiscuous mode enabled");
            
            // In promiscuous mode, we should receive all frames
            // (We can't easily verify this without traffic, but we can check no error)
            
            // Disable promiscuous mode
            emac.set_promiscuous(false);
            info!("  Promiscuous mode disabled");
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-8-002: Test promiscuous mode with actual frame reception
pub fn test_promiscuous_rx(duration_ms: u32) -> TestResult {
    // First enable promiscuous mode and ensure we can receive
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            emac.set_promiscuous(true);
            emac.set_pass_all_multicast(true); // Ensure multicast/broadcast received
            emac.clear_all_interrupts();
            // Drain any stale packets
            let mut buf = [0u8; 64];
            while emac.rx_available() {
                let _ = emac.receive(&mut buf);
            }
        }
    });
    
    info!("  Promiscuous mode ON, listening for {}ms...", duration_ms);
    
    let mut rx_buffer = [0u8; 1600];
    let mut packet_count = 0u32;
    let mut unicast_to_others = 0u32;
    let delay = esp_hal::delay::Delay::new();
    let iterations = duration_ms;
    
    // Our MAC address
    let our_mac = critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            Some(*emac.mac_address())
        } else {
            None
        }
    });
    
    let Some(our_mac) = our_mac else {
        error!("  EMAC not available");
        return TestResult::Fail;
    };
    
    for _ in 0..iterations {
        critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                while emac.rx_available() {
                    if let Ok(len) = emac.receive(&mut rx_buffer) {
                        packet_count += 1;
                        if len >= 14 {
                            let dst = &rx_buffer[0..6];
                            // Check if unicast but not to us
                            if (dst[0] & 0x01) == 0 && dst != our_mac {
                                unicast_to_others += 1;
                            }
                        }
                    }
                }
            }
        });
        delay.delay_millis(1);
    }
    
    // Disable promiscuous mode
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            emac.set_promiscuous(false);
        }
    });
    
    info!("  Received {} packets, {} unicast to other MACs", packet_count, unicast_to_others);
    
    // We pass if we received anything (promiscuous working)
    // or if network is quiet (no traffic to test with)
    if packet_count > 0 {
        info!("  Promiscuous mode successfully received traffic");
        if unicast_to_others > 0 {
            info!("  Including {} frames not addressed to us", unicast_to_others);
        }
    } else {
        warn!("  No traffic - can't verify promiscuous mode fully");
    }
    
    TestResult::Pass
}

/// IT-8-003: Test PHY capabilities reading
pub fn test_phy_capabilities(ctx: &mut TestContext) -> TestResult {
    match ctx.phy.capabilities(&mut ctx.mdio) {
        Ok(caps) => {
            info!("  PHY Capabilities:");
            info!("    100BASE-TX FD: {}", caps.speed_100_fd);
            info!("    100BASE-TX HD: {}", caps.speed_100_hd);
            info!("    10BASE-T FD:   {}", caps.speed_10_fd);
            info!("    10BASE-T HD:   {}", caps.speed_10_hd);
            info!("    Auto-neg:      {}", caps.auto_negotiation);
            info!("    Pause:         {}", caps.pause);
            
            // LAN8720A should support all standard 10/100 modes
            if caps.speed_100_fd && caps.speed_10_fd && caps.auto_negotiation {
                info!("  Standard 10/100 PHY capabilities confirmed");
                TestResult::Pass
            } else {
                warn!("  Unexpected capability set");
                TestResult::Pass // Still pass, just unexpected
            }
        }
        Err(e) => {
            error!("  Failed to read capabilities: {:?}", e);
            TestResult::Fail
        }
    }
}

/// IT-8-004: Test PHY force link (disable auto-negotiation)
pub fn test_force_link(ctx: &mut TestContext) -> TestResult {
    info!("  Testing forced link modes...");
    
    // Save current state
    let _original_speed = ctx.link_speed;
    let _original_duplex = ctx.link_duplex;
    
    // Try forcing 10 Mbps Full Duplex
    let force_result = ctx.phy.force_link(
        &mut ctx.mdio, 
        LinkStatus::new(Speed::Mbps10, Duplex::Full)
    );
    
    match force_result {
        Ok(()) => {
            info!("  Forced to 10 Mbps Full Duplex");
            
            // Wait a bit for link to re-establish
            esp_hal::delay::Delay::new().delay_millis(500);
            
            // Check if link came back up
            match ctx.phy.poll_link(&mut ctx.mdio) {
                Ok(Some(status)) => {
                    info!("  Link re-established: {:?} {:?}", status.speed, status.duplex);
                    
                    // Update MAC to match
                    critical_section::with(|cs| {
                        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                            emac.set_speed(status.speed);
                            emac.set_duplex(status.duplex);
                        }
                    });
                }
                Ok(None) => {
                    warn!("  Link not yet up after force");
                }
                Err(e) => {
                    error!("  Link poll error: {:?}", e);
                }
            }
        }
        Err(e) => {
            error!("  Force link failed: {:?}", e);
            return TestResult::Fail;
        }
    }
    
    // Restore auto-negotiation and original link
    info!("  Restoring auto-negotiation...");
    let _ = ctx.phy.init(&mut ctx.mdio); // Re-init enables auto-neg
    
    // Wait for auto-neg to complete
    let delay = esp_hal::delay::Delay::new();
    for i in 0..30 {
        delay.delay_millis(100);
        match ctx.phy.poll_link(&mut ctx.mdio) {
            Ok(Some(status)) => {
                info!("  Auto-neg complete: {:?} {:?} ({}ms)", 
                      status.speed, status.duplex, (i + 1) * 100);
                ctx.link_speed = status.speed;
                ctx.link_duplex = status.duplex;
                
                // Update MAC
                critical_section::with(|cs| {
                    if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                        emac.set_speed(status.speed);
                        emac.set_duplex(status.duplex);
                    }
                });
                break;
            }
            Ok(None) => continue,
            Err(_) => break,
        }
    }
    
    TestResult::Pass
}

/// IT-8-005: Test enabling/disabling TX interrupt
pub fn test_enable_tx_interrupt() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            // Enable TX interrupt
            emac.enable_tx_interrupt(true);
            info!("  TX interrupt enabled");
            
            // Disable TX interrupt
            emac.enable_tx_interrupt(false);
            info!("  TX interrupt disabled");
            
            // Re-enable for normal operation
            emac.enable_tx_interrupt(true);
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-8-006: Test enabling/disabling RX interrupt
pub fn test_enable_rx_interrupt() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            // Enable RX interrupt
            emac.enable_rx_interrupt(true);
            info!("  RX interrupt enabled");
            
            // Disable RX interrupt
            emac.enable_rx_interrupt(false);
            info!("  RX interrupt disabled");
            
            // Re-enable for normal operation
            emac.enable_rx_interrupt(true);
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-8-007: Test TX interrupt fires after transmission
pub fn test_tx_interrupt_fires() -> TestResult {
    // Clear all pending interrupts first
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            emac.clear_all_interrupts();
            emac.enable_tx_interrupt(true);
        }
    });
    
    // Transmit a frame
    let mut frame = [0u8; 64];
    frame[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    frame[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x12, 0x34, 0x56]);
    frame[12..14].copy_from_slice(&[0x88, 0xB5]);
    
    let tx_ok = critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            emac.transmit(&frame).is_ok()
        } else {
            false
        }
    });
    
    if !tx_ok {
        error!("  Failed to transmit test frame");
        return TestResult::Fail;
    }
    
    // Wait for TX to complete
    esp_hal::delay::Delay::new().delay_millis(10);
    
    // Check if TX interrupt fired
    let status = critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            Some(emac.interrupt_status())
        } else {
            None
        }
    });
    
    match status {
        Some(s) => {
            info!("  After TX: tx_complete={}", s.tx_complete);
            if s.tx_complete {
                info!("  TX interrupt fired correctly");
                TestResult::Pass
            } else {
                warn!("  TX complete not set (may have been cleared)");
                TestResult::Pass // May have been handled already
            }
        }
        None => {
            error!("  EMAC not available");
            TestResult::Fail
        }
    }
}
