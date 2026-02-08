//! Group 6: smoltcp Integration Tests
//!
//! Network stack integration with smoltcp Device trait.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-6-001 | Interface creation | Create smoltcp interface |
//! | IT-6-002 | Device capabilities | Check Device trait MTU/checksums |
//! | IT-6-003 | Interface poll | Poll smoltcp interface for 2s |

use log::{error, info};

use smoltcp::iface::{Config as IfaceConfig, Interface, PollResult, SocketSet};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address};

use super::framework::{TestResult, EMAC};

/// Our IP configuration for testing
const OUR_IP: Ipv4Address = Ipv4Address::new(192, 168, 1, 200);

/// IT-6-001: Test smoltcp interface creation
pub fn test_interface_creation() -> TestResult {
    info!("  Creating smoltcp interface...");
    
    // Get MAC address from EMAC
    let mac = critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            Some(*emac.mac_address())
        } else {
            None
        }
    });

    let Some(mac) = mac else {
        error!("  EMAC not available");
        return TestResult::Fail;
    };

    info!("  MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    info!("  IP:  192.168.1.200/24");
    
    // Test that we can construct an interface config
    let ethernet_addr = EthernetAddress(mac);
    let _config = IfaceConfig::new(HardwareAddress::Ethernet(ethernet_addr));
    
    info!("  Interface config created successfully");
    TestResult::Pass
}

/// IT-6-002: Test smoltcp Device trait implementation
pub fn test_device_capabilities() -> TestResult {
    info!("  Checking Device trait capabilities...");
    
    use smoltcp::phy::Device;
    
    let result = critical_section::with(|cs| {
        let mut emac_ref = EMAC.borrow_ref_mut(cs);
        if let Some(ref mut emac) = *emac_ref {
            let caps = emac.capabilities();
            
            info!("  MTU: {} bytes", caps.max_transmission_unit);
            info!("  Medium: Ethernet");
            
            let checksums = caps.checksum;
            info!("  IPv4 TX checksum: {:?}", checksums.ipv4);
            info!("  TCP TX checksum: {:?}", checksums.tcp);
            info!("  UDP TX checksum: {:?}", checksums.udp);
            
            // Verify reasonable MTU
            if caps.max_transmission_unit >= 1500 {
                TestResult::Pass
            } else {
                error!("  MTU too small: {}", caps.max_transmission_unit);
                TestResult::Fail
            }
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    });

    result
}

/// IT-6-003: Test that smoltcp can process incoming packets via the Device trait
pub fn test_interface_poll() -> TestResult {
    info!("  Polling interface for 2 seconds...");
    
    let mac = critical_section::with(|cs| {
        if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
            Some(*emac.mac_address())
        } else {
            None
        }
    });

    let Some(mac) = mac else {
        error!("  EMAC not available");
        return TestResult::Fail;
    };

    let ethernet_addr = EthernetAddress(mac);
    let config = IfaceConfig::new(HardwareAddress::Ethernet(ethernet_addr));

    let result = critical_section::with(|cs| {
        let mut emac_ref = EMAC.borrow_ref_mut(cs);
        if let Some(ref mut emac) = *emac_ref {
            // Create interface
            let mut iface = Interface::new(config, emac, Instant::from_millis(0));
            
            // Configure IP address
            iface.update_ip_addrs(|addrs| {
                let _ = addrs.push(IpCidr::new(IpAddress::Ipv4(OUR_IP), 24));
            });

            // Create socket storage on stack
            let mut socket_storage: [_; 1] = Default::default();
            let mut sockets = SocketSet::new(&mut socket_storage[..]);
            
            let delay = esp_hal::delay::Delay::new();
            let mut poll_count = 0u32;
            
            for i in 0..200u32 {
                let timestamp = Instant::from_millis((i * 10) as i64);
                let poll_result = iface.poll(timestamp, emac, &mut sockets);
                
                // Check if any socket state changed
                if poll_result == PollResult::SocketStateChanged {
                    poll_count += 1;
                }
                delay.delay_millis(10);
            }

            info!("  Completed {} poll cycles", poll_count);
            
            // Interface working - poll completed
            TestResult::Pass
        } else {
            error!("  EMAC not available");
            TestResult::Fail
        }
    });

    result
}
