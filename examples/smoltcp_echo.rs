//! smoltcp Network Stack Integration Example
//!
//! This example demonstrates using ph-esp32-mac with the smoltcp TCP/IP stack.
//! It creates a simple TCP echo server that responds to connections on port 7.
//!
//! # Features
//!
//! - Full TCP/IP networking via smoltcp
//! - DHCP IPv4 configuration
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
//! 2. Find its IP address from the DHCP log output
//! 3. Test with: `nc <assigned-ip> 7` or `telnet <assigned-ip> 7`
//! 4. Type text and see it echoed back

#![no_std]
#![no_main]

use core::cell::RefCell;
use critical_section::Mutex;

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    rng::Rng,
    time::Instant,
};
use log::{debug, info, warn};

use smoltcp::{
    iface::{Config, Interface, SocketSet},
    socket::dhcpv4,
    socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer},
    time::Instant as SmolInstant,
    wire::{EthernetAddress, IpCidr, Ipv4Cidr},
};

use ph_esp32_mac::{
    Duplex, Emac, EmacConfig, Lan8720a, MdioController, PhyDriver, PhyInterface, RmiiClockMode,
    Speed,
};

// =============================================================================
// Network Configuration
// =============================================================================

/// TCP echo server port
const ECHO_PORT: u16 = 7;

/// MAC address (locally administered)
const MAC_ADDR: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x34, 0x56];

// =============================================================================
// Board Configuration
// =============================================================================

const PHY_ADDR: u8 = 1;
#[allow(dead_code)]
const CLK_EN_GPIO: u8 = 16;

// =============================================================================
// Static EMAC Instance
// =============================================================================

/// Static EMAC instance
static EMAC: Mutex<RefCell<Option<Emac<10, 10, 1600>>>> = Mutex::new(RefCell::new(None));

// =============================================================================
// Main Entry Point
// =============================================================================

esp_app_desc!();

#[main]
fn main() -> ! {
    // Initialize esp-hal
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize logging
    esp_println::logger::init_logger_from_env();
    info!("smoltcp TCP Echo Server starting...");

    let mut delay = Delay::new();
    let mut mdio = MdioController::new(Delay::new());

    // Enable external oscillator
    let mut clk_en = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    clk_en.set_high();
    delay.delay_millis(10);

    // Place EMAC in static storage before init (required for DMA descriptors).
    critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs).replace(Emac::new());
    });

    // Configure EMAC
    let config = EmacConfig::new()
        .with_mac_address(MAC_ADDR)
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(RmiiClockMode::ExternalInput { gpio: 0 });

    info!("Initializing EMAC...");
    critical_section::with(|cs| {
        let mut emac_ref = EMAC.borrow_ref_mut(cs);
        let emac = emac_ref.as_mut().expect("EMAC static unavailable");
        emac.init(config, &mut delay).expect("EMAC init failed");
    });

    // Initialize PHY
    info!("Initializing PHY...");
    let mut phy = Lan8720a::new(PHY_ADDR);
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
            critical_section::with(|cs| {
                let mut emac_ref = EMAC.borrow_ref_mut(cs);
                let emac = emac_ref.as_mut().expect("EMAC static unavailable");
                emac.set_speed(status.speed);
                emac.set_duplex(status.duplex);
            });
            break;
        }
    }

    // Start EMAC
    critical_section::with(|cs| {
        let mut emac_ref = EMAC.borrow_ref_mut(cs);
        let emac = emac_ref.as_mut().expect("EMAC static unavailable");
        emac.start().expect("EMAC start failed");
    });
    info!("EMAC started");

    // ==========================================================================
    // smoltcp Interface Setup
    // ==========================================================================

    // Create smoltcp configuration
    let hw_addr = EthernetAddress(MAC_ADDR);
    let mut smol_config = Config::new(hw_addr.into());
    let rng = Rng::new();
    smol_config.random_seed =
        ((rng.random() as u64) << 32) | (rng.random() as u64);

    // Create the network interface
    let mut iface = critical_section::with(|cs| {
        let mut emac_ref = EMAC.borrow_ref_mut(cs);
        let emac = emac_ref.as_mut().expect("EMAC static unavailable");
        Interface::new(smol_config, emac, SmolInstant::from_millis(0))
    });
    iface.set_any_ip(true);

    // ==========================================================================
    // Socket Setup
    // ==========================================================================

    // Create socket storage
    let mut socket_storage = [smoltcp::iface::SocketStorage::EMPTY; 5];
    let mut sockets = SocketSet::new(&mut socket_storage[..]);

    // Create TCP socket buffers
    let mut tcp_rx_buffer = [0u8; 1024];
    let mut tcp_tx_buffer = [0u8; 1024];
    let tcp_socket = TcpSocket::new(
        TcpSocketBuffer::new(&mut tcp_rx_buffer[..]),
        TcpSocketBuffer::new(&mut tcp_tx_buffer[..]),
    );
    let tcp_handle = sockets.add(tcp_socket);

    // Create DHCP socket
    let dhcp_handle = sockets.add(dhcpv4::Socket::new());

    // Start listening on echo port
    {
        let socket = sockets.get_mut::<TcpSocket>(tcp_handle);
        socket.listen(ECHO_PORT).unwrap();
        info!("TCP echo server listening on port {}", ECHO_PORT);
    }

    // ==========================================================================
    // Main Network Loop
    // ==========================================================================

    let mut last_status_time = Instant::now();
    let mut connections = 0u32;
    let mut bytes_echoed = 0u64;
    let mut echo_buf = [0u8; 1024];
    let mut dhcp_configured = false;
    let mut dhcp_last_address: Option<Ipv4Cidr> = None;

    info!("Entering main loop...");
    info!("Waiting for DHCP...");

    loop {
        // Get current timestamp for smoltcp
        let now = Instant::now();
        let smol_now =
            SmolInstant::from_millis(now.duration_since_epoch().as_millis() as i64);

        // Poll the interface (handles ARP, ICMP, etc.)
        critical_section::with(|cs| {
            let mut emac_ref = EMAC.borrow_ref_mut(cs);
            let emac = emac_ref.as_mut().expect("EMAC static unavailable");
            let _activity = iface.poll(smol_now, emac, &mut sockets);
        });

        // Handle DHCP events
        if let Some(event) = sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll() {
            match event {
                dhcpv4::Event::Configured(config) => {
                    iface.update_ip_addrs(|addrs| {
                        addrs.clear();
                        addrs.push(IpCidr::Ipv4(config.address)).unwrap();
                    });
                    iface.set_any_ip(false);

                    if let Some(router) = config.router {
                        iface.routes_mut().add_default_ipv4_route(router).ok();
                    } else {
                        iface.routes_mut().remove_default_ipv4_route();
                    }

                    let address_changed = dhcp_last_address.map_or(true, |addr| addr != config.address);
                    if address_changed {
                        info!("DHCP address: {}", config.address);
                        if let Some(router) = config.router {
                            info!("DHCP gateway: {}", router);
                        }
                        info!("Test with: nc {} {}", config.address.address(), ECHO_PORT);
                    } else {
                        info!("DHCP renewed: {}", config.address);
                    }

                    dhcp_last_address = Some(config.address);
                    dhcp_configured = true;
                }
                dhcpv4::Event::Deconfigured => {
                    iface.update_ip_addrs(|addrs| addrs.clear());
                    iface.routes_mut().remove_default_ipv4_route();
                    iface.set_any_ip(true);
                    dhcp_last_address = None;
                    if dhcp_configured {
                        warn!("DHCP deconfigured");
                    }
                    dhcp_configured = false;
                }
            }
        }

        // Handle TCP socket
        {
            let socket = sockets.get_mut::<TcpSocket>(tcp_handle);

            // Check for new connections
            if socket.is_active() && socket.may_recv() {
                // Echo received data back
                if socket.can_recv() {
                    match socket.recv_slice(&mut echo_buf) {
                        Ok(len) if len > 0 => {
                            debug!("Received {} bytes", len);
                            if socket.can_send() {
                                match socket.send_slice(&echo_buf[..len]) {
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
        if (now - last_status_time).as_secs() >= 30 {
            info!(
                "Status: {} connections, {} bytes echoed",
                connections, bytes_echoed
            );

            // Check link status
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
