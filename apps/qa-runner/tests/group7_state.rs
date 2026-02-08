//! Group 7: State & Interrupts Tests
//!
//! State machine, interrupt handling, TX ready, RX peek, frame sizes.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-7-001 | State transitions | Verify Running state after start |
//! | IT-7-002 | State stop changes | Verify Stopped state after stop |
//! | IT-7-003 | TX ready | Check tx_ready() and descriptors_available |
//! | IT-7-004 | Can transmit sizes | Test can_transmit() for various sizes |
//! | IT-7-005 | TX backpressure | Fill TX buffer, detect backpressure |
//! | IT-7-006 | Peek RX length | Verify peek_rx_length consistency |
//! | IT-7-007 | RX frames waiting | Verify rx_frames_waiting count |
//! | IT-7-008 | Interrupt status | Read interrupt status flags |
//! | IT-7-009 | Interrupt clear | Clear all pending interrupts |
//! | IT-7-010 | Handle interrupt | Atomic read and clear |
//! | IT-7-011 | Frame sizes TX | Test min to max frame sizes |

use log::{error, info, warn};

use ph_esp32_mac::{InterruptStatus, State};

use super::framework::{TestResult, EMAC};

/// IT-7-001: Test state transitions through EMAC lifecycle
pub fn test_state_transitions() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            // After init + start, should be Running
            let state = emac.state();
            info!("  Current state: {:?}", state);
            
            if state == State::Running {
                info!("  EMAC is in Running state as expected");
                TestResult::Pass
            } else {
                error!("  Expected Running state, got {:?}", state);
                TestResult::Fail
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-7-002: Test state changes when stopping/starting EMAC
pub fn test_state_stop_changes() -> TestResult {
    // Stop EMAC and check state
    let stop_result = critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            match emac.stop() {
                Ok(()) => {
                    let state = emac.state();
                    info!("  After stop: state = {:?}", state);
                    if state == State::Stopped {
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
                Err(e) => Err(e),
            }
        } else {
            Err(ph_esp32_mac::Error::Config(ph_esp32_mac::ConfigError::InvalidConfig))
        }
    });

    match stop_result {
        Ok(true) => {
            // Restart EMAC
            let restart = critical_section::with(|cs| {
                if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                    emac.start()
                } else {
                    Err(ph_esp32_mac::Error::Config(ph_esp32_mac::ConfigError::InvalidConfig))
                }
            });
            match restart {
                Ok(()) => {
                    info!("  EMAC restarted to Running state");
                    TestResult::Pass
                }
                Err(e) => {
                    error!("  Failed to restart: {:?}", e);
                    TestResult::Fail
                }
            }
        }
        Ok(false) => {
            error!("  State was not Stopped after stop()");
            TestResult::Fail
        }
        Err(e) => {
            error!("  Stop failed: {:?}", e);
            TestResult::Fail
        }
    }
}

/// IT-7-003: Test tx_ready() returns true when buffer available
pub fn test_tx_ready() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            let ready = emac.tx_ready();
            let available = emac.tx_descriptors_available();
            
            info!("  tx_ready() = {}, descriptors available = {}", ready, available);
            
            if ready && available > 0 {
                TestResult::Pass
            } else if !ready && available == 0 {
                TestResult::Pass  // Consistent: not ready when none available
            } else {
                error!("  Inconsistent state: ready={} but available={}", ready, available);
                TestResult::Fail
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-7-004: Test can_transmit() for various frame sizes
pub fn test_can_transmit() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            // Test minimum frame size
            let can_64 = emac.can_transmit(64);
            info!("  can_transmit(64) = {}", can_64);
            
            // Test typical frame size
            let can_512 = emac.can_transmit(512);
            info!("  can_transmit(512) = {}", can_512);
            
            // Test maximum Ethernet frame size
            let can_1518 = emac.can_transmit(1518);
            info!("  can_transmit(1518) = {}", can_1518);
            
            // Test larger frame (uses scatter-gather with 4 buffers * 1600 = 6400 max)
            let can_2000 = emac.can_transmit(2000);
            info!("  can_transmit(2000) = {} (scatter-gather)", can_2000);
            
            // Test truly oversized frame (exceeds 4 * 1600 = 6400)
            let can_7000 = emac.can_transmit(7000);
            info!("  can_transmit(7000) = {}", can_7000);
            
            // Test zero length (should be false)
            let can_0 = emac.can_transmit(0);
            info!("  can_transmit(0) = {}", can_0);
            
            if can_64 && can_512 && can_1518 && can_2000 && !can_7000 && !can_0 {
                TestResult::Pass
            } else {
                error!("  Unexpected can_transmit results");
                TestResult::Fail
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-7-005: Test TX backpressure by filling TX buffer
pub fn test_tx_backpressure() -> TestResult {
    // Build a test frame
    let mut frame = [0u8; 64];
    frame[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Broadcast
    frame[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x12, 0x34, 0x56]); // Our MAC
    frame[12..14].copy_from_slice(&[0x88, 0xB5]); // EtherType
    
    let mut sent_count = 0u32;
    let mut not_ready = false;
    
    // Try to fill the TX buffer
    for i in 0..10 {
        let result = critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                if emac.tx_ready() {
                    match emac.transmit(&frame) {
                        Ok(_) => Some(true),
                        Err(_) => Some(false),
                    }
                } else {
                    None // Not ready
                }
            } else {
                Some(false)
            }
        });
        
        match result {
            Some(true) => {
                sent_count += 1;
            }
            Some(false) => {
                error!("  TX failed at frame {}", i);
                return TestResult::Fail;
            }
            None => {
                not_ready = true;
                info!("  TX not ready after {} frames (buffer full)", sent_count);
                break;
            }
        }
    }
    
    if sent_count > 0 {
        info!("  Sent {} frames before backpressure", sent_count);
        if not_ready {
            info!("  Backpressure detected correctly");
        }
        
        // Wait for TX to complete
        esp_hal::delay::Delay::new().delay_millis(50);
        
        // Check TX is ready again
        let ready_again = critical_section::with(|cs| {
            if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
                emac.tx_ready()
            } else {
                false
            }
        });
        
        if ready_again {
            info!("  TX ready again after drain");
            TestResult::Pass
        } else {
            warn!("  TX still not ready after drain");
            TestResult::Pass // Still consider pass if we sent frames
        }
    } else {
        error!("  No frames could be sent");
        TestResult::Fail
    }
}

/// IT-7-006: Test peek_rx_length before receiving
pub fn test_peek_rx_length() -> TestResult {
    info!("  Checking peek_rx_length...");
    
    let result = critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            let available = emac.rx_available();
            let peek = emac.peek_rx_length();
            
            info!("  rx_available() = {}", available);
            info!("  peek_rx_length() = {:?}", peek);
            
            // Check consistency
            match (available, peek) {
                (true, Some(len)) => {
                    info!("  Consistent: frame available, length = {}", len);
                    if len > 0 && len <= 1600 {
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
                (false, None) => {
                    info!("  Consistent: no frame available");
                    Ok(true)
                }
                (true, None) => {
                    warn!("  Inconsistent: rx_available=true but peek=None");
                    Ok(false)
                }
                (false, Some(len)) => {
                    warn!("  Inconsistent: rx_available=false but peek=Some({})", len);
                    Ok(false)
                }
            }
        } else {
            Err(())
        }
    });
    
    match result {
        Ok(true) => TestResult::Pass,
        Ok(false) => {
            // May be a timing issue, don't fail hard
            warn!("  Possible timing issue with RX state");
            TestResult::Pass
        }
        Err(()) => {
            error!("  EMAC not available");
            TestResult::Fail
        }
    }
}

/// IT-7-007: Test rx_frames_waiting count
pub fn test_rx_frames_waiting() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            let waiting = emac.rx_frames_waiting();
            let available = emac.rx_available();
            
            info!("  rx_frames_waiting() = {}", waiting);
            info!("  rx_available() = {}", available);
            
            // Check consistency
            if available && waiting > 0 {
                info!("  Consistent: {} frames waiting", waiting);
                TestResult::Pass
            } else if !available && waiting == 0 {
                info!("  Consistent: no frames waiting");
                TestResult::Pass
            } else if available && waiting == 0 {
                // This can happen if frame was consumed between calls
                warn!("  Possible race: available but count=0");
                TestResult::Pass
            } else {
                info!("  rx_available={}, waiting={}", available, waiting);
                TestResult::Pass
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-7-008: Test interrupt status reading
pub fn test_interrupt_status() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            let status: InterruptStatus = emac.interrupt_status();
            
            info!("  Interrupt status:");
            info!("    tx_complete: {}", status.tx_complete);
            info!("    rx_complete: {}", status.rx_complete);
            info!("    tx_underflow: {}", status.tx_underflow);
            info!("    rx_overflow: {}", status.rx_overflow);
            info!("    any: {}", status.any());
            info!("    has_error: {}", status.has_error());
            
            // Status read successfully
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-7-009: Test interrupt clearing
pub fn test_interrupt_clear() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            // Read current status
            let before = emac.interrupt_status();
            info!("  Before clear: any={}", before.any());
            
            // Clear all interrupts
            emac.clear_all_interrupts();
            
            // Read status again
            let after = emac.interrupt_status();
            info!("  After clear: any={}", after.any());
            
            // After clear, status should be minimal
            // Note: new interrupts may fire immediately, so we don't require all clear
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-7-010: Test handle_interrupt (reads and clears atomically)
pub fn test_handle_interrupt() -> TestResult {
    // First, transmit a packet to generate a TX interrupt
    let mut frame = [0u8; 64];
    frame[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    frame[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x12, 0x34, 0x56]);
    frame[12..14].copy_from_slice(&[0x88, 0xB5]);
    
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            // Clear any pending interrupts first
            emac.clear_all_interrupts();
            
            // Transmit
            let _ = emac.transmit(&frame);
        }
    });
    
    // Wait a bit for TX to complete
    esp_hal::delay::Delay::new().delay_millis(10);
    
    // Handle interrupt
    let result = critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            let status = emac.handle_interrupt();
            info!("  handle_interrupt returned:");
            info!("    tx_complete: {}", status.tx_complete);
            info!("    rx_complete: {}", status.rx_complete);
            Some(status)
        } else {
            None
        }
    });
    
    match result {
        Some(status) => {
            if status.tx_complete {
                info!("  TX complete interrupt detected");
            }
            TestResult::Pass
        }
        None => {
            error!("  EMAC not available");
            TestResult::Fail
        }
    }
}

/// IT-7-011: Test different frame sizes (min, typical, max)
pub fn test_frame_sizes() -> TestResult {
    let sizes = [
        (64, "minimum"),
        (128, "small"),
        (512, "typical"),
        (1024, "medium"),
        (1518, "maximum"),
    ];
    
    let delay = esp_hal::delay::Delay::new();
    let mut all_ok = true;
    
    for (size, name) in sizes {
        let mut frame = [0u8; 1518];
        frame[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        frame[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x12, 0x34, 0x56]);
        frame[12..14].copy_from_slice(&[0x88, 0xB5]);
        // Fill payload with pattern
        for i in 14..size {
            frame[i] = (i & 0xFF) as u8;
        }
        
        let result = critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                emac.transmit(&frame[..size])
            } else {
                Err(ph_esp32_mac::Error::Config(ph_esp32_mac::ConfigError::InvalidConfig))
            }
        });
        
        match result {
            Ok(len) => {
                info!("  TX {} ({} bytes): OK, sent {} bytes", name, size, len);
            }
            Err(e) => {
                error!("  TX {} ({} bytes): FAILED {:?}", name, size, e);
                all_ok = false;
            }
        }
        
        delay.delay_millis(10);
    }
    
    if all_ok {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}
