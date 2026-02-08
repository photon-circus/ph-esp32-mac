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
use core::marker::PhantomData;
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
    phy::{Device, RxToken, TxToken},
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
/// Delay before starting DHCP after link-up (seconds).
const DHCP_START_DELAY_SECS: u64 = 2;

/// Enable DHCP TX/RX logging.
const DHCP_LOG: bool = true;
/// Temporarily enable promiscuous mode while waiting for DHCP.
const DHCP_PROMISCUOUS: bool = true;

// =============================================================================
// Static EMAC Instance
// =============================================================================

/// Static EMAC instance
static EMAC: Mutex<RefCell<Option<Emac<10, 10, 1600>>>> = Mutex::new(RefCell::new(None));

// =============================================================================
// DHCP Logging Helpers
// =============================================================================

struct DhcpInfo {
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    msg_type: Option<u8>,
    len: usize,
}

fn parse_dhcp(buf: &[u8]) -> Option<DhcpInfo> {
    if buf.len() < 14 + 20 + 8 {
        return None;
    }

    let ethertype = u16::from_be_bytes([buf[12], buf[13]]);
    if ethertype != 0x0800 {
        return None;
    }

    let ip = 14usize;
    let ihl = (buf[ip] & 0x0F) as usize * 4;
    if ihl < 20 || buf.len() < ip + ihl + 8 {
        return None;
    }

    if buf[ip + 9] != 17 {
        return None;
    }

    let src_ip = [buf[ip + 12], buf[ip + 13], buf[ip + 14], buf[ip + 15]];
    let dst_ip = [buf[ip + 16], buf[ip + 17], buf[ip + 18], buf[ip + 19]];

    let udp = ip + ihl;
    let src_port = u16::from_be_bytes([buf[udp], buf[udp + 1]]);
    let dst_port = u16::from_be_bytes([buf[udp + 2], buf[udp + 3]]);
    let is_dhcp = (src_port == 67 && dst_port == 68) || (src_port == 68 && dst_port == 67);
    if !is_dhcp {
        return None;
    }

    let payload = &buf[udp + 8..];
    let msg_type = parse_dhcp_msg_type(payload);

    Some(DhcpInfo {
        src_ip,
        dst_ip,
        src_port,
        dst_port,
        msg_type,
        len: buf.len(),
    })
}

fn parse_dhcp_msg_type(payload: &[u8]) -> Option<u8> {
    const DHCP_FIXED_LEN: usize = 236;
    const COOKIE: [u8; 4] = [99, 130, 83, 99];
    if payload.len() < DHCP_FIXED_LEN + 4 {
        return None;
    }
    if payload[DHCP_FIXED_LEN..DHCP_FIXED_LEN + 4] != COOKIE {
        return None;
    }

    let mut idx = DHCP_FIXED_LEN + 4;
    while idx < payload.len() {
        let opt = payload[idx];
        idx += 1;
        match opt {
            0 => continue, // Pad
            255 => break,  // End
            _ => {
                if idx >= payload.len() {
                    break;
                }
                let len = payload[idx] as usize;
                idx += 1;
                if idx + len > payload.len() {
                    break;
                }
                if opt == 53 && len >= 1 {
                    return Some(payload[idx]);
                }
                idx += len;
            }
        }
    }
    None
}

fn msg_type_name(msg_type: Option<u8>) -> &'static str {
    match msg_type {
        Some(1) => "Discover",
        Some(2) => "Offer",
        Some(3) => "Request",
        Some(4) => "Decline",
        Some(5) => "Ack",
        Some(6) => "Nak",
        Some(7) => "Release",
        Some(8) => "Inform",
        _ => "Unknown",
    }
}

fn log_dhcp(direction: &str, info: &DhcpInfo) {
    info!(
        "{} DHCP {}->{} {}.{}.{}.{}:{} -> {}.{}.{}.{}:{} len={}",
        direction,
        msg_type_name(info.msg_type),
        if info.src_port == 68 {
            "client"
        } else {
            "server"
        },
        info.src_ip[0],
        info.src_ip[1],
        info.src_ip[2],
        info.src_ip[3],
        info.src_port,
        info.dst_ip[0],
        info.dst_ip[1],
        info.dst_ip[2],
        info.dst_ip[3],
        info.dst_port,
        info.len
    );
}

// =============================================================================
// smoltcp Logging Device Wrapper
// =============================================================================

struct LoggingEmac<'a, const RX: usize, const TX: usize, const BUF: usize> {
    emac: *mut Emac<RX, TX, BUF>,
    _marker: PhantomData<&'a mut Emac<RX, TX, BUF>>,
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> LoggingEmac<'a, RX, TX, BUF> {
    fn new(emac: &'a mut Emac<RX, TX, BUF>) -> Self {
        Self {
            emac: emac as *mut Emac<RX, TX, BUF>,
            _marker: PhantomData,
        }
    }
}

struct LoggingRxToken<T> {
    inner: T,
}

struct LoggingTxToken<T> {
    inner: T,
}

impl<T: RxToken> RxToken for LoggingRxToken<T> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        self.inner.consume(|buf| {
            if DHCP_LOG {
                if let Some(info) = parse_dhcp(buf) {
                    log_dhcp("RX", &info);
                }
            }
            f(buf)
        })
    }
}

impl<T: TxToken> TxToken for LoggingTxToken<T> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.inner.consume(len, |buf| {
            let result = f(buf);
            if DHCP_LOG {
                if let Some(info) = parse_dhcp(buf) {
                    log_dhcp("TX", &info);
                }
            }
            result
        })
    }
}

impl<const RX: usize, const TX: usize, const BUF: usize> Device for LoggingEmac<'_, RX, TX, BUF> {
    type RxToken<'a>
        = LoggingRxToken<<Emac<RX, TX, BUF> as Device>::RxToken<'a>>
    where
        Self: 'a;
    type TxToken<'a>
        = LoggingTxToken<<Emac<RX, TX, BUF> as Device>::TxToken<'a>>
    where
        Self: 'a;

    fn receive(
        &mut self,
        timestamp: SmolInstant,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // SAFETY: emac points to a static EMAC instance.
        let emac = unsafe { &mut *self.emac };
        <Emac<RX, TX, BUF> as Device>::receive(emac, timestamp).map(|(rx, tx)| {
            (
                LoggingRxToken { inner: rx },
                LoggingTxToken { inner: tx },
            )
        })
    }

    fn transmit(&mut self, timestamp: SmolInstant) -> Option<Self::TxToken<'_>> {
        // SAFETY: emac points to a static EMAC instance.
        let emac = unsafe { &mut *self.emac };
        <Emac<RX, TX, BUF> as Device>::transmit(emac, timestamp)
            .map(|tx| LoggingTxToken { inner: tx })
    }

    fn capabilities(&self) -> smoltcp::phy::DeviceCapabilities {
        // SAFETY: emac points to a static EMAC instance.
        let emac = unsafe { &mut *self.emac };
        <Emac<RX, TX, BUF> as Device>::capabilities(emac)
    }
}

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

    critical_section::with(|cs| {
        let mut emac_ref = EMAC.borrow_ref_mut(cs);
        let emac = emac_ref.as_mut().expect("EMAC static unavailable");
        emac.set_broadcast_enabled(true);
        emac.set_pass_all_multicast(true);
        emac.set_promiscuous(false);
    });
    info!("Broadcast + multicast filters relaxed for DHCP");
    let mut dhcp_promisc_enabled = false;
    if DHCP_PROMISCUOUS {
        critical_section::with(|cs| {
            let mut emac_ref = EMAC.borrow_ref_mut(cs);
            let emac = emac_ref.as_mut().expect("EMAC static unavailable");
            emac.set_promiscuous(true);
        });
        dhcp_promisc_enabled = true;
        info!("Promiscuous mode enabled for DHCP");
    }

    if DHCP_START_DELAY_SECS > 0 {
        delay.delay_millis((DHCP_START_DELAY_SECS * 1000) as u32);
    }

    // ==========================================================================
    // smoltcp Interface Setup
    // ==========================================================================

    // Create smoltcp configuration
    let hw_addr = critical_section::with(|cs| {
        let emac_ref = EMAC.borrow_ref_mut(cs);
        let emac = emac_ref.as_ref().expect("EMAC static unavailable");
        EthernetAddress(*emac.mac_address())
    });
    let mut smol_config = Config::new(hw_addr.into());
    let rng = Rng::new();
    smol_config.random_seed = ((rng.random() as u64) << 32) | (rng.random() as u64);

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
        let smol_now = SmolInstant::from_millis(now.duration_since_epoch().as_millis() as i64);

        // Poll the interface (handles ARP, ICMP, etc.)
        critical_section::with(|cs| {
            let mut emac_ref = EMAC.borrow_ref_mut(cs);
            let emac = emac_ref.as_mut().expect("EMAC static unavailable");
            let mut device = LoggingEmac::new(emac);
            let _activity = iface.poll(smol_now, &mut device, &mut sockets);
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

                    let address_changed =
                        dhcp_last_address.map_or(true, |addr| addr != config.address);
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
                    if DHCP_PROMISCUOUS && dhcp_promisc_enabled {
                        critical_section::with(|cs| {
                            let mut emac_ref = EMAC.borrow_ref_mut(cs);
                            let emac = emac_ref.as_mut().expect("EMAC static unavailable");
                            emac.set_promiscuous(false);
                        });
                        dhcp_promisc_enabled = false;
                        info!("Promiscuous mode disabled after DHCP");
                    }

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
                    if DHCP_PROMISCUOUS && !dhcp_promisc_enabled {
                        critical_section::with(|cs| {
                            let mut emac_ref = EMAC.borrow_ref_mut(cs);
                            let emac = emac_ref.as_mut().expect("EMAC static unavailable");
                            emac.set_promiscuous(true);
                        });
                        dhcp_promisc_enabled = true;
                        info!("Promiscuous mode re-enabled for DHCP");
                    }
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
