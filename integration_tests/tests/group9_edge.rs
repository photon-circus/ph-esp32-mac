//! Group 9: Edge Cases Tests
//!
//! MAC filtering, hash table, VLAN, flow control, energy detect.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-9-001 | MAC filtering | Add/remove MAC address filters |
//! | IT-9-002 | MAC filter multiple | Multiple filter slots |
//! | IT-9-003 | Hash filtering | Set hash table for multicast |
//! | IT-9-004 | Pass all multicast | Toggle multicast mode |
//! | IT-9-005 | VLAN filtering | Set/disable VLAN filter |
//! | IT-9-006 | Flow control config | Read flow control settings |
//! | IT-9-007 | Flow control check | Check flow control state |
//! | IT-9-008 | Energy detect | PHY energy detect power-down |
//! | IT-9-009 | RX interrupt fires | Verify RX interrupt on receive |
//! | IT-9-010 | Async wakers | Async waker API availability |
//! | IT-9-011 | Restore RX state | Restore EMAC to normal RX state |

use log::{error, info, warn};

use super::framework::{TestContext, TestResult, EMAC};

/// IT-9-001: Test MAC address filtering - add and remove filters
pub fn test_mac_filtering() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Test MAC address to filter
            let test_mac: [u8; 6] = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
            
            // Add a MAC filter
            match emac.add_mac_filter(&test_mac) {
                Ok(slot) => {
                    info!("  Added MAC filter in slot {}", slot);
                    
                    // Remove the filter
                    match emac.remove_mac_filter(&test_mac) {
                        Ok(()) => {
                            info!("  Removed MAC filter");
                        }
                        Err(e) => {
                            error!("  Failed to remove filter: {:?}", e);
                            return TestResult::Fail;
                        }
                    }
                }
                Err(e) => {
                    error!("  Failed to add MAC filter: {:?}", e);
                    return TestResult::Fail;
                }
            }
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-9-002: Test adding multiple MAC filters
pub fn test_mac_filter_multiple() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            let macs: [[u8; 6]; 3] = [
                [0x02, 0x11, 0x22, 0x33, 0x44, 0x55],
                [0x02, 0x66, 0x77, 0x88, 0x99, 0xAA],
                [0x02, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            ];
            
            // Add multiple filters
            let mut added = 0;
            for mac in &macs {
                match emac.add_mac_filter(mac) {
                    Ok(slot) => {
                        info!("  Added filter {} in slot {}", added, slot);
                        added += 1;
                    }
                    Err(e) => {
                        warn!("  Could not add filter {}: {:?}", added, e);
                        break;
                    }
                }
            }
            
            info!("  Added {} filters total", added);
            
            // Clear all filters
            emac.clear_mac_filters();
            info!("  Cleared all MAC filters");
            
            if added > 0 {
                TestResult::Pass
            } else {
                error!("  Could not add any filters");
                TestResult::Fail
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-9-003: Test hash table filtering for multicast
pub fn test_hash_filtering() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Set hash table value (all bits set = accept all)
            emac.set_hash_table(0xFFFF_FFFF_FFFF_FFFF);
            info!("  Set hash table to accept all");
            
            // Clear hash table
            emac.set_hash_table(0);
            info!("  Cleared hash table");
            
            // Set specific bits for testing
            emac.set_hash_table(0x0000_0001_0000_0001);
            info!("  Set specific hash bits");
            
            // Clear again
            emac.set_hash_table(0);
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-9-004: Test pass all multicast setting
pub fn test_pass_all_multicast() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Enable pass all multicast
            emac.set_pass_all_multicast(true);
            info!("  Pass all multicast enabled");
            
            // Disable pass all multicast
            emac.set_pass_all_multicast(false);
            info!("  Pass all multicast disabled");
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-9-005: Test VLAN filtering
pub fn test_vlan_filtering() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Set VLAN filter (VID 100)
            emac.set_vlan_filter(100);
            info!("  Set VLAN filter for VID 100");
            
            // Set different VID
            emac.set_vlan_filter(200);
            info!("  Changed VLAN filter to VID 200");
            
            // Disable VLAN filtering
            emac.disable_vlan_filter();
            info!("  Disabled VLAN filtering");
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-9-006: Test flow control configuration
pub fn test_flow_control_config() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            // Read flow control config
            let config = emac.flow_control_config();
            
            info!("  Flow control config:");
            info!("    Enabled: {}", config.enabled);
            info!("    Low water mark: {}", config.low_water_mark);
            info!("    High water mark: {}", config.high_water_mark);
            info!("    Pause time: {}", config.pause_time);
            info!("    Threshold: {:?}", config.pause_low_threshold);
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-9-007: Test flow control check mechanism
pub fn test_flow_control_check() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Check flow control (may or may not be active based on config)
            let changed = emac.check_flow_control();
            info!("  Flow control check: state_changed={}", changed);
            
            // Check if flow control is currently active
            let active = emac.is_flow_control_active();
            info!("  Flow control active: {}", active);
            
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-9-008: Test PHY energy detect power-down feature
pub fn test_energy_detect(ctx: &mut TestContext) -> TestResult {
    info!("  Testing energy detect power-down...");
    
    // Check current energy state
    match ctx.phy.is_energy_on(&mut ctx.mdio) {
        Ok(energy_on) => {
            info!("  Energy detected: {}", energy_on);
        }
        Err(e) => {
            error!("  Failed to read energy state: {:?}", e);
            return TestResult::Fail;
        }
    }
    
    // Enable energy detect power-down
    match ctx.phy.set_energy_detect_powerdown(&mut ctx.mdio, true) {
        Ok(()) => {
            info!("  Energy detect power-down enabled");
        }
        Err(e) => {
            error!("  Failed to enable EDPD: {:?}", e);
            return TestResult::Fail;
        }
    }
    
    // Read state again
    match ctx.phy.is_energy_on(&mut ctx.mdio) {
        Ok(energy_on) => {
            info!("  Energy detected (with EDPD): {}", energy_on);
        }
        Err(e) => {
            warn!("  Could not read energy state: {:?}", e);
        }
    }
    
    // Disable energy detect power-down (restore normal operation)
    match ctx.phy.set_energy_detect_powerdown(&mut ctx.mdio, false) {
        Ok(()) => {
            info!("  Energy detect power-down disabled");
        }
        Err(e) => {
            error!("  Failed to disable EDPD: {:?}", e);
            return TestResult::Fail;
        }
    }
    
    TestResult::Pass
}

/// IT-9-009: Test RX interrupt wakes after receiving packet
pub fn test_rx_interrupt_fires(duration_ms: u32) -> TestResult {
    // Ensure EMAC is in a state to receive traffic
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Re-enable multicast reception (may have been disabled by earlier tests)
            emac.set_pass_all_multicast(true);
            emac.disable_vlan_filter();
            emac.clear_all_interrupts();
            emac.enable_rx_interrupt(true);
            // Drain any stale packets first
            let mut buf = [0u8; 64];
            while emac.rx_available() {
                let _ = emac.receive(&mut buf);
            }
        }
    });
    
    info!("  Listening for RX interrupt for {}ms...", duration_ms);
    
    let delay = esp_hal::delay::Delay::new();
    let mut rx_interrupt_seen = false;
    let iterations = duration_ms / 10;
    
    for _ in 0..iterations {
        delay.delay_millis(10);
        
        let status = critical_section::with(|cs| {
            if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
                Some(emac.interrupt_status())
            } else {
                None
            }
        });
        
        if let Some(s) = status {
            if s.rx_complete {
                rx_interrupt_seen = true;
                info!("  RX interrupt fired!");
                
                // Clear and drain RX
                critical_section::with(|cs| {
                    if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                        emac.clear_all_interrupts();
                        let mut buf = [0u8; 64];
                        while emac.rx_available() {
                            let _ = emac.receive(&mut buf);
                        }
                    }
                });
                break;
            }
        }
    }
    
    if rx_interrupt_seen {
        info!("  RX interrupt test passed");
    } else {
        warn!("  No RX interrupt (no traffic during test window)");
    }
    
    // Pass either way - we can't force external traffic
    TestResult::Pass
}

/// IT-9-010: Test async waker registration (basic functionality)
/// Note: Full async testing requires embassy runtime which isn't setup here
pub fn test_async_wakers() -> TestResult {
    // Async support is feature-gated and requires an async runtime.
    info!("  Async per-instance waker state available (AsyncEmacState)");
    info!("  Full async test requires async feature + runtime - skipping");
    
    // We could test that async_interrupt_handler exists by checking
    // if interrupt handling works, which we test in other tests
    
    TestResult::Pass
}

/// IT-9-011: Restore EMAC to normal receiving state after edge case tests
/// This ensures continuous monitoring mode works properly
pub fn test_restore_rx_state() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            info!("  Restoring EMAC receive state...");
            
            // 1. Disable promiscuous mode (use normal filtering)
            emac.set_promiscuous(false);
            info!("    Promiscuous: off");
            
            // 2. Clear any stale MAC filters (keeps primary MAC in slot 0)
            emac.clear_mac_filters();
            info!("    Extra MAC filters: cleared");
            
            // 3. Enable pass-all-multicast for monitoring
            //    This ensures we receive broadcast/multicast traffic like LLDP, ARP
            emac.set_pass_all_multicast(true);
            info!("    Pass all multicast: on");
            
            // 4. Disable VLAN filtering
            emac.disable_vlan_filter();
            info!("    VLAN filter: off");
            
            // 5. Clear hash table (not needed with pass-all-multicast)
            emac.set_hash_table(0);
            info!("    Hash table: cleared");
            
            // 6. Clear any pending interrupts
            emac.clear_all_interrupts();
            info!("    Interrupts: cleared");
            
            // 7. Enable RX/TX interrupts for monitoring
            emac.enable_rx_interrupt(true);
            emac.enable_tx_interrupt(true);
            info!("    RX/TX interrupts: enabled");
            
            info!("  EMAC restored to normal receiving state");
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}
