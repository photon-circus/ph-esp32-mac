//! Dedicated embassy-net RX-exhaustion release validation firmware.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::{
    Config, ConfigV4, DhcpConfig, Ipv4Address, Ipv4Cidr, Stack, StaticConfigV4, udp::UdpSocket,
};
use embassy_net_driver::LinkState;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    interrupt::Priority,
    timer::timg::TimerGroup,
    uart::{Config as UartConfig, Uart},
};
use ph_esp32_mac::esp_hal::EmacExt;
use ph_esp32_mac::unsafe_registers::DmaRegs;
use ph_esp32_mac::{Emac, EmbassyEmac, InterruptStatus};
use ph_esp32_mac_qa_runner::interrupts::INTERRUPTS;
use ph_esp32_mac_qa_runner::rx_harness::{
    BUFFER_SIZE, DUT_MAC, FLOOD_WINDOW_MS, RX_DESCRIPTORS, RxActionCode, RxSuiteCode,
    TX_DESCRIPTORS, WindowKind, bring_up, drain_control_input, fail_all, flood_interrupt_bound,
    observe_exhaustion, observe_flood_window, observe_quiet_window, observe_window, parse_qa_frame,
    record_condition, wait_for_control_ack, wait_for_exhaustion,
};
use ph_esp32_mac_qa_runner::snapshots::{MacSnapshot, RxSnapshot};
use ph_esp32_mac_qa_runner::{
    BUILD_GIT_COMMIT, BUILD_RUN_ID, Reporter, RunMode, enable_wt32_oscillator, reset_reason_token,
    terminal_idle,
};
use static_cell::StaticCell;

const TEST_IDS: &[&str] = &[
    "rx-embassy.exhausted",
    "rx-embassy.quiet",
    "rx-embassy.flood",
    "rx-embassy.heartbeat",
    "rx-embassy.udp-recovered",
    "rx-embassy.dhcp",
];

const FIXED_ADDRESS: Ipv4Address = Ipv4Address::new(192, 0, 2, 2);
const FIXED_PREFIX: u8 = 24;
const UDP_ECHO_PORT: u16 = 42_424;
const UDP_BUFFER_SIZE: usize = 256;
const MAC_FRAME_FILTER_PROMISCUOUS: u32 = 1;
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(2);
const DHCP_TIMEOUT: Duration = Duration::from_secs(10);
const CONTROL_ACK_TIMEOUT_MS: u32 = 2_000;

ph_esp32_mac::embassy_net_statics!(
    EMAC,
    EMAC_STATE,
    NET_RESOURCES,
    RX_DESCRIPTORS,
    TX_DESCRIPTORS,
    BUFFER_SIZE,
    4
);

static UDP_RX_META: StaticCell<[embassy_net::udp::PacketMetadata; 4]> = StaticCell::new();
static UDP_TX_META: StaticCell<[embassy_net::udp::PacketMetadata; 4]> = StaticCell::new();
static UDP_RX_BUFFER: StaticCell<[u8; UDP_BUFFER_SIZE]> = StaticCell::new();
static UDP_TX_BUFFER: StaticCell<[u8; UDP_BUFFER_SIZE]> = StaticCell::new();

ph_esp32_mac::emac_isr!(RX_EMBASSY_EMAC_IRQ, Priority::Priority1, {
    let raw = DmaRegs::status();
    INTERRUPTS.observe(raw);
    let status = InterruptStatus::from_raw(raw);
    EMAC_STATE.on_interrupt(status);
    DmaRegs::set_status(status.to_raw());
});

#[embassy_executor::task]
async fn network_task(
    mut runner: embassy_net::Runner<
        'static,
        EmbassyEmac<'static, RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>,
    >,
) -> ! {
    runner.run().await
}

esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let mut reporter = Reporter::new(
        BUILD_RUN_ID,
        "rx-embassy",
        BUILD_GIT_COMMIT,
        RunMode::Release,
        reset_reason_token(),
    );

    let peripherals = esp_hal::init(esp_hal::Config::default());
    let control = Uart::new(peripherals.UART0, UartConfig::default())
        .map(|uart| uart.with_rx(peripherals.GPIO3).with_tx(peripherals.GPIO1));
    reporter.start();
    let Ok(mut control) = control else {
        fail_all(&mut reporter, TEST_IDS, "control-uart");
        let _ = reporter.finish();
        terminal_idle();
    };
    let timer_group = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timer_group.timer0);
    let _clock_enable = enable_wt32_oscillator(peripherals.GPIO16);
    let mut delay = Delay::new();
    let emac = EMAC.init(Emac::new());

    if let Err(error) = bring_up(emac, &mut delay) {
        fail_all(&mut reporter, TEST_IDS, error.reason());
        let _ = reporter.finish();
        terminal_idle();
    }
    emac.bind_interrupt(RX_EMBASSY_EMAC_IRQ);
    EMAC_STATE.set_link_state(LinkState::Up);
    let initial_mac = MacSnapshot::capture();
    let strict_filter = initial_mac.primary_address_matches(DUT_MAC)
        && (initial_mac.frame_filter & MAC_FRAME_FILTER_PROMISCUOUS) == 0;
    reporter.observation(
        "rx-embassy.promiscuous-before",
        u8::from((initial_mac.frame_filter & MAC_FRAME_FILTER_PROMISCUOUS) != 0),
    );
    reporter.observation("rx-embassy.mac-high-before", initial_mac.address_high);
    reporter.observation("rx-embassy.mac-low-before", initial_mac.address_low);

    let emac_ptr = emac as *mut Emac<RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>;
    let driver = ph_esp32_mac::embassy_net_driver!(emac_ptr, &EMAC_STATE);
    let config = Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(FIXED_ADDRESS, FIXED_PREFIX),
        gateway: None,
        dns_servers: Default::default(),
    });
    let (stack, runner) =
        ph_esp32_mac::embassy_net_stack!(driver, NET_RESOURCES, config, 0x5048_5141_0001_0006);

    // Construct the real driver and stack before starvation, but deliberately
    // withhold runner polling. Interrupts stay active while all four
    // descriptors transition to CPU ownership.
    let exhaustion_start = INTERRUPTS.snapshot();
    reporter.ready("fill_rx");
    let exhausted = wait_for_exhaustion(&mut delay, exhaustion_start);
    observe_exhaustion(&reporter, exhausted);

    let quiet = observe_quiet_window(&mut delay);
    observe_window(
        &reporter,
        "rx-embassy.quiet_heartbeat",
        WindowKind::Quiet,
        quiet,
    );
    record_condition(
        &mut reporter,
        "rx-embassy.quiet",
        quiet.interrupts.ru <= 2 && quiet.interrupts.fbi == 0,
        "ru-storm-idle",
    );

    let flood_start = INTERRUPTS.snapshot();
    drain_control_input(&mut control);
    reporter.ready("flood_rx");
    let flood = observe_flood_window(
        &reporter,
        &mut delay,
        &mut control,
        "rx-embassy.flood_heartbeat",
        flood_start,
        BUILD_RUN_ID,
        2,
    );
    observe_window(
        &reporter,
        "rx-embassy.flood_heartbeats",
        WindowKind::Flood,
        flood,
    );
    record_condition(
        &mut reporter,
        "rx-embassy.flood",
        flood_interrupt_bound(flood),
        "isr-growth",
    );
    record_condition(
        &mut reporter,
        "rx-embassy.heartbeat",
        flood.heartbeats >= FLOOD_WINDOW_MS / 100,
        "heartbeat-stalled",
    );

    record_condition(
        &mut reporter,
        "rx-embassy.exhausted",
        strict_filter && exhausted.passed(),
        "exhaustion-or-filter",
    );

    let recovery_started = Instant::now();
    if spawner.spawn(network_task(runner)).is_err() {
        record_condition(
            &mut reporter,
            "rx-embassy.udp-recovered",
            false,
            "runner-spawn",
        );
        record_condition(&mut reporter, "rx-embassy.dhcp", false, "runner-spawn");
        let _ = reporter.finish();
        terminal_idle();
    }

    let resumed = wait_for_ring_restoration(recovery_started).await;
    observe_resume_snapshot(&reporter, resumed);
    reporter.observation("rx-embassy.fixed_ipv4", "192.0.2.2");
    reporter.observation("rx-embassy.udp_port", UDP_ECHO_PORT);
    let udp = run_udp_echo(stack, &mut reporter, recovery_started).await;
    reporter.observation("rx-embassy.udp-elapsed-ms", udp.elapsed_ms);
    let non_promiscuous_after_echo =
        (MacSnapshot::capture().frame_filter & MAC_FRAME_FILTER_PROMISCUOUS) == 0;
    record_condition(
        &mut reporter,
        "rx-embassy.udp-recovered",
        resumed.is_some()
            && udp.echoed
            && udp.elapsed_ms <= 2_000
            && non_promiscuous_after_echo
            && INTERRUPTS.snapshot().fbi == 0,
        "udp-echo-timeout",
    );

    stack.set_config_v4(ConfigV4::None);
    let config_stopped = with_timeout(Duration::from_secs(2), stack.wait_config_down())
        .await
        .is_ok();
    reporter.observation("rx-embassy.fixed-config-stopped", u8::from(config_stopped));
    drain_control_input(&mut control);
    reporter.ready("dhcp");
    let dhcp_ack = wait_for_control_ack(
        &mut control,
        &mut delay,
        BUILD_RUN_ID,
        4,
        "dhcp",
        CONTROL_ACK_TIMEOUT_MS,
    );
    reporter.observation("rx-embassy.dhcp-ack", u8::from(dhcp_ack.matched));
    reporter.observation("rx-embassy.dhcp-ack-elapsed-ms", dhcp_ack.elapsed_ms);
    let dhcp_address = if dhcp_ack.matched {
        stack.set_config_v4(ConfigV4::Dhcp(DhcpConfig::default()));
        wait_for_dhcp(stack).await
    } else {
        None
    };
    reporter.observation(
        "rx-embassy.dhcp-address",
        dhcp_address.map_or(0, |address| u32::from_be_bytes(address.octets())),
    );
    let non_promiscuous_after_dhcp =
        (MacSnapshot::capture().frame_filter & MAC_FRAME_FILTER_PROMISCUOUS) == 0;
    record_condition(
        &mut reporter,
        "rx-embassy.dhcp",
        config_stopped && dhcp_ack.matched && dhcp_address.is_some() && non_promiscuous_after_dhcp,
        "dhcp-timeout",
    );

    let _ = reporter.finish();
    terminal_idle()
}

#[derive(Clone, Copy, Debug, Default)]
struct UdpEchoEvidence {
    echoed: bool,
    elapsed_ms: u32,
}

async fn run_udp_echo(
    stack: Stack<'static>,
    reporter: &mut Reporter<'_>,
    recovery_started: Instant,
) -> UdpEchoEvidence {
    let rx_meta = UDP_RX_META.init([embassy_net::udp::PacketMetadata::EMPTY; 4]);
    let tx_meta = UDP_TX_META.init([embassy_net::udp::PacketMetadata::EMPTY; 4]);
    let rx_buffer = UDP_RX_BUFFER.init([0u8; UDP_BUFFER_SIZE]);
    let tx_buffer = UDP_TX_BUFFER.init([0u8; UDP_BUFFER_SIZE]);
    let mut socket = UdpSocket::new(stack, rx_meta, rx_buffer, tx_meta, tx_buffer);
    if socket.bind(UDP_ECHO_PORT).is_err() {
        return UdpEchoEvidence::default();
    }

    let Some(remaining) = RECOVERY_TIMEOUT.checked_sub(recovery_started.elapsed()) else {
        return UdpEchoEvidence {
            echoed: false,
            elapsed_ms: elapsed_millis(recovery_started),
        };
    };
    reporter.ready("udp_echo");
    let echoed = with_timeout(remaining, async {
        let mut payload = [0u8; UDP_BUFFER_SIZE];
        let Ok((length, endpoint)) = socket.recv_from(&mut payload).await else {
            return false;
        };
        let tagged = parse_qa_frame(
            &payload[..length],
            RxSuiteCode::Embassy,
            RxActionCode::UdpEcho,
            BUILD_RUN_ID,
            3,
        )
        .is_some_and(|tag| tag.sequence == 0);
        tagged && socket.send_to(&payload[..length], endpoint).await.is_ok()
    })
    .await
    .unwrap_or(false);

    UdpEchoEvidence {
        echoed,
        elapsed_ms: elapsed_millis(recovery_started),
    }
}

async fn wait_for_dhcp(stack: Stack<'static>) -> Option<Ipv4Address> {
    with_timeout(DHCP_TIMEOUT, async {
        loop {
            if let Some(config) = stack.config_v4()
                && config.address.address() != FIXED_ADDRESS
            {
                return config.address.address();
            }
            Timer::after(Duration::from_millis(100)).await;
        }
    })
    .await
    .ok()
}

async fn wait_for_ring_restoration(recovery_started: Instant) -> Option<RxSnapshot> {
    let remaining = RECOVERY_TIMEOUT.checked_sub(recovery_started.elapsed())?;
    with_timeout(remaining, async {
        loop {
            let snapshot = RxSnapshot::capture();
            if snapshot.descriptor_sample_valid
                && snapshot.dma_owned_descriptors == RX_DESCRIPTORS as u8
            {
                return snapshot;
            }
            Timer::after(Duration::from_millis(1)).await;
        }
    })
    .await
    .ok()
}

fn elapsed_millis(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

fn observe_resume_snapshot(reporter: &Reporter<'_>, snapshot: Option<RxSnapshot>) {
    reporter.observation("rx-embassy.runner-resumed", u8::from(snapshot.is_some()));
    reporter.observation(
        "rx-embassy.resume-status",
        snapshot.map_or(0, |value| value.status),
    );
    reporter.observation(
        "rx-embassy.resume-descriptor-valid",
        snapshot.map_or(0, |value| u8::from(value.descriptor_sample_valid)),
    );
    reporter.observation(
        "rx-embassy.resume-dma-owned",
        snapshot.map_or(0, |value| value.dma_owned_descriptors),
    );
}
