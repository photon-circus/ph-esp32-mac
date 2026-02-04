//! Group 4: EMAC Operations Tests
//!
//! End-to-end packet TX/RX verification.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-4-001 | EMAC start | Start EMAC for TX/RX |
//! | IT-4-002 | Packet TX | Transmit broadcast frame |
//! | IT-4-003 | Packet RX | Receive packets (timed) |
//! | IT-4-004 | EMAC stop/start | Stop and restart cycle |

use log::{error, info, warn};

use super::framework::{TestResult, EMAC};

/// IT-4-001: Test EMAC can be started
pub fn test_emac_start() -> TestResult {
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            match emac.start() {
                Ok(()) => {
                    info!("  EMAC started");
                    TestResult::Pass
                }
                Err(e) => {
                    error!("  EMAC start failed: {:?}", e);
                    TestResult::Fail
                }
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    })
}

/// IT-4-002: Test packet transmission
pub fn test_packet_tx() -> TestResult {
    // Build a broadcast test frame
    let mut frame = [0u8; 64];
    frame[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Dst: broadcast
    frame[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x12, 0x34, 0x56]); // Src: our MAC
    frame[12..14].copy_from_slice(&[0x88, 0xB5]); // EtherType: local experimental
    for i in 14..64 {
        frame[i] = (i - 14) as u8; // Payload: incrementing pattern
    }
    
    let result = critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            match emac.transmit(&frame) {
                Ok(_len) => {
                    info!("  Transmitted 64-byte broadcast frame");
                    TestResult::Pass
                }
                Err(e) => {
                    error!("  TX failed: {:?}", e);
                    TestResult::Fail
                }
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    });
    
    esp_hal::delay::Delay::new().delay_millis(10);
    result
}

/// IT-4-003: Test packet reception
pub fn test_packet_rx(duration_secs: u32) -> TestResult {
    info!("  Listening for {} seconds...", duration_secs);
    
    let mut rx_buffer = [0u8; 1600];
    let mut packet_count = 0u32;
    let delay = esp_hal::delay::Delay::new();
    let iterations = duration_secs * 1000;
    
    for _ in 0..iterations {
        critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                while emac.rx_available() {
                    if let Ok(len) = emac.receive(&mut rx_buffer) {
                        packet_count += 1;
                        if packet_count <= 3 && len >= 14 {
                            let etype = u16::from_be_bytes([rx_buffer[12], rx_buffer[13]]);
                            info!("    Packet #{}: {} bytes, EtherType=0x{:04X}", 
                                  packet_count, len, etype);
                        }
                    }
                }
            }
        });
        delay.delay_millis(1);
    }
    
    info!("  Received {} packets", packet_count);
    
    if packet_count > 0 {
        TestResult::Pass
    } else {
        warn!("  No packets received");
        TestResult::Skip
    }
}

/// IT-4-004: Test EMAC can be stopped and restarted
pub fn test_emac_stop_start() -> TestResult {
    let stop_result = critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            let _ = emac.stop();
            true
        } else {
            false
        }
    });
    
    if !stop_result {
        error!("  EMAC not available");
        return TestResult::Fail;
    }
    
    info!("  EMAC stopped");
    esp_hal::delay::Delay::new().delay_millis(100);
    
    let start_result = critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            emac.start()
        } else {
            Err(ph_esp32_mac::Error::Config(ph_esp32_mac::ConfigError::InvalidConfig))
        }
    });
    
    match start_result {
        Ok(()) => {
            info!("  EMAC restarted");
            TestResult::Pass
        }
        Err(e) => {
            error!("  EMAC restart failed: {:?}", e);
            TestResult::Fail
        }
    }
}
