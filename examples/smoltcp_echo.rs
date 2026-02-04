//! smoltcp Network Stack Integration Example
//!
//! This example demonstrates using ph-esp32-mac with the smoltcp TCP/IP stack.
//! It creates a simple TCP echo server that responds to connections on port 7.
//!
//! # Features
//!
//! - Full TCP/IP networking via smoltcp
//! - DHCP or static IP configuration
//! - TCP echo server on port 7
//! - ARP, ICMP (ping) support
//!
//! # Hardware
//!
//! Tested on WT32-ETH01 board with LAN8720A PHY.
//!
//! # Building
//!
//! ```bash
//! cargo build --example smoltcp_echo --target xtensa-esp32-none-elf --release \
//!     --features "esp32,smoltcp,critical-section"
//! ```
//!
//! # Testing
//!
//! 1. Connect the board to your network
//! 2. Find its IP address (192.168.1.100 if static, or check DHCP logs)
//! 3. Test with: `nc 192.168.1.100 7` or `telnet 192.168.1.100 7`
//! 4. Type text and see it echoed back

#![no_std]
#![no_main]

use core::cell::RefCell;

use critical_section::Mutex;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main, time,
};
use log::{debug, error, info, warn};

use smoltcp::{
    iface::{Config, Interface, SocketSet},
    socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer},
    time::Instant as SmolInstant,
    wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address},
};

use ph_esp32_mac::{
    Duplex, Emac, EmacConfig, Lan8720a, MdioController, PhyDriver, PhyInterface, RmiiClockMode,
    Speed,
};

// =============================================================================
// Network Configuration
// =============================================================================

/// Static IP configuration (set to None for DHCP if supported)
const STATIC_IP: Option<IpCidr> = Some(IpCidr::new(IpAddress::v4(192, 168, 1, 100), 24));

/// Default gateway
const GATEWAY: Option<Ipv4Address> = Some(Ipv4Address::new(192, 168, 1, 1));

/// TCP echo server port
const ECHO_PORT: u16 = 7;

/// MAC address (locally administered)
const MAC_ADDR: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x34, 0x56];

// =============================================================================
// Board Configuration
// =============================================================================

const PHY_ADDR: u8 = 1;
const CLK_EN_GPIO: u8 = 16;

// =============================================================================
// Static EMAC Instance
// =============================================================================

/// Static EMAC instance
static mut EMAC: Emac<10, 10, 1600> = Emac::new();

// =============================================================================
// Main Entry Point
// =============================================================================

#[main]
fn main() -> ! {
    // Initialize esp-hal
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize logging
    esp_println::logger::init_logger_from_env();
    info!("smoltcp TCP Echo Server starting...");

    let mut delay = Delay::new();

    // Enable external oscillator
    let mut clk_en = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    clk_en.set_high();
    delay.delay_millis(10);

    // Get EMAC reference
    let emac = unsafe { &mut EMAC };

    // Configure EMAC
    let config = EmacConfig::new()
        .with_mac_address(MAC_ADDR)
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(RmiiClockMode::ExternalGpio0);

    info!("Initializing EMAC...");
    emac.init(config, &mut delay).expect("EMAC init failed");

    // Initialize PHY
    info!("Initializing PHY...");
    let mut phy = Lan8720a::new(PHY_ADDR);
    let mut mdio = MdioController::new(&mut delay);
    phy.init(&mut mdio).expect("PHY init failed");

    // Wait for link
    info!("Waiting for link...");
    loop {
        delay.delay_millis(100);
        if let Ok(Some(status)) = phy.poll_link(&mut mdio) {
            info!(
                "Link UP: {} {}",
                if matches!(status.speed, Speed::Mbps100) {
                    "100Mbps"
                } else {
                    "10Mbps"
                },
                if matches!(status.duplex, Duplex::Full) {
                    "FD"
                } else {
                    "HD"
                }
            );
            emac.set_speed(status.speed);
            emac.set_duplex(status.duplex);
            break;
        }
    }

    // Start EMAC
    emac.start().expect("EMAC start failed");
    info!("EMAC started");

    // ==========================================================================
    // smoltcp Interface Setup
    // ==========================================================================

    // Create smoltcp configuration
    let hw_addr = EthernetAddress(MAC_ADDR);
    let smol_config = Config::new(hw_addr.into());

    // Create the network interface
    let mut iface = Interface::new(smol_config, emac, SmolInstant::from_millis(0));

    // Configure IP address
    if let Some(ip) = STATIC_IP {
        iface.update_ip_addrs(|addrs| {
            addrs.push(ip).unwrap();
        });
        info!("Static IP: {}", ip);
    }

    // Configure default gateway
    if let Some(gw) = GATEWAY {
        iface.routes_mut().add_default_ipv4_route(gw).unwrap();
        info!("Gateway: {}", gw);
    }

    // ==========================================================================
    // Socket Setup
    // ==========================================================================

    // Create socket storage
    let mut socket_storage = [smoltcp::iface::SocketStorage::EMPTY; 4];
    let mut sockets = SocketSet::new(&mut socket_storage[..]);

    // Create TCP socket buffers
    let mut tcp_rx_buffer = [0u8; 1024];
    let mut tcp_tx_buffer = [0u8; 1024];
    let tcp_socket = TcpSocket::new(
        TcpSocketBuffer::new(&mut tcp_rx_buffer[..]),
        TcpSocketBuffer::new(&mut tcp_tx_buffer[..]),
    );
    let tcp_handle = sockets.add(tcp_socket);

    // Start listening on echo port
    {
        let socket = sockets.get_mut::<TcpSocket>(tcp_handle);
        socket.listen(ECHO_PORT).unwrap();
        info!("TCP echo server listening on port {}", ECHO_PORT);
    }

    // ==========================================================================
    // Main Network Loop
    // ==========================================================================

    let mut last_status_time = time::Instant::now();
    let mut connections = 0u32;
    let mut bytes_echoed = 0u64;

    info!("Entering main loop...");
    info!("Test with: nc {} {}", "192.168.1.100", ECHO_PORT);

    loop {
        // Get current timestamp for smoltcp
        let now = time::Instant::now();
        let smol_now = SmolInstant::from_millis(now.as_millis() as i64);

        // Poll the interface (handles ARP, ICMP, etc.)
        let _activity = iface.poll(smol_now, emac, &mut sockets);

        // Handle TCP socket
        {
            let socket = sockets.get_mut::<TcpSocket>(tcp_handle);

            // Check for new connections
            if socket.is_active() && socket.may_recv() {
                // Echo received data back
                if socket.can_recv() {
                    match socket.recv(|data| {
                        let len = data.len();
                        if len > 0 {
                            debug!("Received {} bytes", len);
                        }
                        (len, data.to_vec()) // Return data to echo
                    }) {
                        Ok(data) if !data.is_empty() => {
                            // Send data back
                            if socket.can_send() {
                                match socket.send_slice(&data) {
                                    Ok(sent) => {
                                        bytes_echoed += sent as u64;
                                        debug!("Echoed {} bytes", sent);
                                    }
                                    Err(e) => {
                                        warn!("Send error: {:?}", e);
                                    }
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warn!("Recv error: {:?}", e);
                        }
                    }
                }
            }

            // Handle connection close
            if !socket.is_active() && !socket.is_listening() {
                info!("Connection closed, re-listening...");
                socket.abort();
                socket.listen(ECHO_PORT).unwrap();
                connections += 1;
            }
        }

        // Periodic status update
        if now.duration_since(last_status_time).as_secs() >= 30 {
            info!(
                "Status: {} connections, {} bytes echoed",
                connections, bytes_echoed
            );

            // Check link status
            let mut mdio = MdioController::new(&mut delay);
            if let Ok(up) = phy.is_link_up(&mut mdio) {
                if !up {
                    warn!("Link is DOWN!");
                }
            }

            last_status_time = now;
        }

        // Small delay to prevent tight polling
        delay.delay_micros(10);
    }
}
