//! Embassy-net + esp-hal Example
//!
//! This example demonstrates using ph-esp32-mac with embassy-net on ESP32.
//! It uses the embassy executor via `esp-rtos` (recommended for esp-hal 1.0.0)
//! and integrates the EMAC driver using `embassy-net-driver`.
//!
//! # Hardware
//!
//! Tested on WT32-ETH01 board with:
//! - ESP32 (WT32-S1 module)
//! - LAN8720A PHY at address 1
//! - External 50 MHz oscillator (enabled via GPIO16)
//!
//! # Notes
//!
//! - This is a reference example. For a complete application, copy this into
//!   a standalone ESP32 project with the proper Cargo.toml dependencies.
//! - The embassy time driver must be started before using `embassy_time::Timer`.
//! - Link changes are handled by a periodic PHY polling task.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::{udp::UdpSocket, Config, ConfigV4, DhcpConfig};
use embassy_net_driver::{Capabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    interrupt::{InterruptHandler, Priority},
    rng::Rng,
    timer::timg::TimerGroup,
};
use log::{info, warn};
use static_cell::StaticCell;

use ph_esp32_mac::integration::esp_hal::{EmacBuilder, EmacExt, EmacPhyBundle, Wt32Eth01};
use ph_esp32_mac::{
    Emac, EmbassyEmac, EmbassyEmacState, MdioController,
};

// =============================================================================
// Board Configuration
// =============================================================================

/// UDP echo port.
const UDP_ECHO_PORT: u16 = 7;

/// UDP socket buffer size.
const UDP_BUF_SIZE: usize = 1024;
/// Delay before starting DHCP after link-up (seconds).
const DHCP_START_DELAY_SECS: u64 = 2;
/// Enable promiscuous mode for all traffic (debugging).
const DEBUG_PROMISCUOUS: bool = false;
/// Temporarily enable promiscuous mode while waiting for DHCP.
const DHCP_PROMISCUOUS: bool = true;

// =============================================================================
// Static EMAC Instance and Embassy Resources
// =============================================================================

ph_esp32_mac::embassy_net_statics!(EMAC, EMAC_STATE, RESOURCES, 10, 10, 1600, 4);
static UDP_RX_META: StaticCell<[embassy_net::udp::PacketMetadata; 4]> = StaticCell::new();
static UDP_TX_META: StaticCell<[embassy_net::udp::PacketMetadata; 4]> = StaticCell::new();
static UDP_RX_BUF: StaticCell<[u8; UDP_BUF_SIZE]> = StaticCell::new();
static UDP_TX_BUF: StaticCell<[u8; UDP_BUF_SIZE]> = StaticCell::new();

// =============================================================================
// DHCP Logging Driver Wrapper
// =============================================================================

struct LoggingDriver<D> {
    inner: D,
}

impl<D> LoggingDriver<D> {
    const fn new(inner: D) -> Self {
        Self { inner }
    }
}

struct LoggingRxToken<T> {
    inner: T,
}

struct LoggingTxToken<T> {
    inner: T,
}

impl<D: Driver> Driver for LoggingDriver<D> {
    type RxToken<'a>
        = LoggingRxToken<D::RxToken<'a>>
    where
        Self: 'a;
    type TxToken<'a>
        = LoggingTxToken<D::TxToken<'a>>
    where
        Self: 'a;

    fn receive(
        &mut self,
        cx: &mut core::task::Context<'_>,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.inner
            .receive(cx)
            .map(|(rx, tx)| (LoggingRxToken { inner: rx }, LoggingTxToken { inner: tx }))
    }

    fn transmit(&mut self, cx: &mut core::task::Context<'_>) -> Option<Self::TxToken<'_>> {
        self.inner
            .transmit(cx)
            .map(|tx| LoggingTxToken { inner: tx })
    }

    fn link_state(&mut self, cx: &mut core::task::Context<'_>) -> LinkState {
        self.inner.link_state(cx)
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn hardware_address(&self) -> HardwareAddress {
        self.inner.hardware_address()
    }
}

impl<T: RxToken> RxToken for LoggingRxToken<T> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.inner.consume(|buf| {
            if let Some(info) = parse_dhcp(buf) {
                log_dhcp("RX", &info);
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
            if let Some(info) = parse_dhcp(buf) {
                log_dhcp("TX", &info);
            }
            result
        })
    }
}

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
// Interrupt Handler
// =============================================================================

#[esp_hal::handler(priority = Priority::Priority1)]
fn emac_handler() {
    EMAC_STATE.handle_interrupt();
}

const EMAC_IRQ: InterruptHandler = emac_handler;

// =============================================================================
// Embassy Tasks
// =============================================================================

type EmacDriver = EmbassyEmac<'static, 10, 10, 1600>;
type LoggedEmacDriver = LoggingDriver<EmacDriver>;

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, LoggedEmacDriver>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn config_task(stack: embassy_net::Stack<'static>, emac: *mut Emac<10, 10, 1600>) -> ! {
    loop {
        stack.wait_link_up().await;
        info!("Link is up");
        if DHCP_PROMISCUOUS && !DEBUG_PROMISCUOUS {
            // Safety: EMAC is in static storage for the program lifetime and only
            // accessed here between await points.
            unsafe {
                (&mut *emac).set_promiscuous(true);
            }
            info!("Promiscuous mode enabled for DHCP");
        }
        Timer::after(Duration::from_secs(DHCP_START_DELAY_SECS)).await;
        stack.set_config_v4(ConfigV4::Dhcp(DhcpConfig::default()));
        info!("DHCP started after link-up delay");

        let mut waited_secs = 0u64;
        loop {
            if let Some(config) = stack.config_v4() {
                info!("DHCP address: {}", config.address);
                if let Some(gateway) = config.gateway {
                    info!("DHCP gateway: {}", gateway);
                }
                if DHCP_PROMISCUOUS && !DEBUG_PROMISCUOUS {
                    // Safety: EMAC is in static storage for the program lifetime and only
                    // accessed here between await points.
                    unsafe {
                        (&mut *emac).set_promiscuous(false);
                    }
                    info!("Promiscuous mode disabled after DHCP");
                }
                break;
            }

            info!("Waiting for DHCP... ({}s)", waited_secs);
            Timer::after(Duration::from_secs(2)).await;
            waited_secs += 2;
        }

        stack.wait_config_down().await;
        warn!("DHCP config lost");
    }
}

#[embassy_executor::task]
async fn udp_echo_task(stack: embassy_net::Stack<'static>) -> ! {
    let rx_meta = UDP_RX_META.init([embassy_net::udp::PacketMetadata::EMPTY; 4]);
    let tx_meta = UDP_TX_META.init([embassy_net::udp::PacketMetadata::EMPTY; 4]);
    let rx_buf = UDP_RX_BUF.init([0u8; UDP_BUF_SIZE]);
    let tx_buf = UDP_TX_BUF.init([0u8; UDP_BUF_SIZE]);

    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);

    loop {
        stack.wait_config_up().await;

        if socket.bind(UDP_ECHO_PORT).is_ok() {
            info!("UDP echo listening on port {}", UDP_ECHO_PORT);
        } else {
            warn!("UDP bind failed; retrying...");
            Timer::after(Duration::from_secs(1)).await;
            continue;
        }

        let mut buf = [0u8; UDP_BUF_SIZE];

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, meta)) => {
                    info!("UDP rx {} bytes from {}", len, meta);
                    if let Err(err) = socket.send_to(&buf[..len], meta).await {
                        warn!("UDP send error: {:?}", err);
                    }
                }
                Err(err) => warn!("UDP recv error: {:?}", err),
            }
        }
    }
}

#[embassy_executor::task]
async fn link_task(state: &'static EmbassyEmacState, emac: *mut Emac<10, 10, 1600>) -> ! {
    let mut mdio = MdioController::new(Delay::new());
    let mut phy = Wt32Eth01::lan8720a();
    let mut last_state = state.link_state();

    loop {
        match state.update_link_from_phy(&mut phy, &mut mdio) {
            Ok(status) => {
                let new_state = if status.is_some() {
                    LinkState::Up
                } else {
                    LinkState::Down
                };
                if let Some(status) = status {
                    // Safety: Emac is in static storage for the duration of the program.
                    // This task runs cooperatively, so no other task can access EMAC
                    // concurrently between await points.
                    unsafe {
                        let emac = &mut *emac;
                        emac.set_speed(status.speed);
                        emac.set_duplex(status.duplex);
                    }
                }

                if new_state != last_state {
                    if let Some(status) = status {
                        info!("Link up: {:?}", status);
                    } else {
                        warn!("Link down");
                    }
                    last_state = new_state;
                }
            }
            Err(e) => warn!("PHY poll error: {:?}", e),
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    info!("embassy-net example starting...");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Start the embassy time driver (required for embassy_time::Timer).
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // Enable external oscillator (WT32-ETH01 specific).
    let mut clk_en = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    clk_en.set_high();

    let mut delay = Delay::new();
    delay.delay_millis(10);
    info!(
        "External oscillator enabled (GPIO{} = HIGH)",
        Wt32Eth01::CLK_EN_GPIO
    );

    // Initialize EMAC.
    let emac_ptr = EMAC.init(Emac::new()) as *mut Emac<10, 10, 1600>;
    let config = Wt32Eth01::emac_config_with_mac([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .with_promiscuous(DEBUG_PROMISCUOUS);

    // Safety: EMAC is in static storage for the duration of the program.
    let emac = unsafe { &mut *emac_ptr };
    EmacBuilder::new(emac).with_config(config).init(&mut delay).unwrap();

    // Initialize PHY and set initial MAC speed/duplex.
    {
        let mut emac_phy = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
        emac_phy.init_phy().unwrap();

        match emac_phy.link_status().unwrap() {
            Some(status) => {
                info!("Link up: {:?}", status);
                EMAC_STATE.set_link_state(LinkState::Up);
            }
            None => {
                warn!("Link down");
                EMAC_STATE.set_link_state(LinkState::Down);
            }
        }
    }

    emac.start().unwrap();
    emac.bind_interrupt(EMAC_IRQ);
    info!("EMAC started");

    // Create embassy-net stack.
    let driver = LoggingDriver::new(ph_esp32_mac::embassy_net_driver!(emac_ptr, &EMAC_STATE));
    let config = Config::default();
    let rng = Rng::new();
    let seed = ((rng.random() as u64) << 32) | (rng.random() as u64);
    let (stack, runner) = ph_esp32_mac::embassy_net_stack!(driver, RESOURCES, config, seed);

    spawner.spawn(net_task(runner)).unwrap();
    spawner.spawn(config_task(stack, emac_ptr)).unwrap();
    spawner.spawn(udp_echo_task(stack)).unwrap();
    spawner.spawn(link_task(&EMAC_STATE, emac_ptr)).unwrap();

    // Keep the main task alive.
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
