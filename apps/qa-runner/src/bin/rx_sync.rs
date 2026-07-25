//! Dedicated synchronous RX-exhaustion release validation firmware.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    main,
    uart::{Config as UartConfig, Uart},
};
use ph_esp32_mac::Emac;
use ph_esp32_mac::esp_hal::{EmacExt, Priority};
use ph_esp32_mac_qa_runner::interrupts::INTERRUPTS;
use ph_esp32_mac_qa_runner::rx_harness::{
    BUFFER_SIZE, DUT_MAC, FLOOD_WINDOW_MS, QUIET_WINDOW_MS, RX_DESCRIPTORS, RxActionCode,
    RxSuiteCode, SENTINEL_TIMEOUT_MS, TX_DESCRIPTORS, WindowKind, bring_up, drain_control_input,
    drain_fill_frames, fail_all, flood_interrupt_bound, observe_exhaustion, observe_flood_window,
    observe_quiet_window, observe_window, record_condition, wait_for_action, wait_for_exhaustion,
};
use ph_esp32_mac_qa_runner::snapshots::{MacSnapshot, RxSnapshot};
use ph_esp32_mac_qa_runner::{
    BUILD_GIT_COMMIT, BUILD_RUN_ID, Reporter, RunMode, enable_wt32_oscillator, reset_reason_token,
    terminal_idle,
};
use static_cell::StaticCell;

const TEST_IDS: &[&str] = &[
    "rx-sync.exhausted",
    "rx-sync.quiet",
    "rx-sync.flood",
    "rx-sync.heartbeat",
    "rx-sync.recovered",
];

#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".dram1"))]
static EMAC: StaticCell<Emac<RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>> = StaticCell::new();

ph_esp32_mac::emac_isr!(RX_SYNC_EMAC_IRQ, Priority::Priority1, {
    ph_esp32_mac_qa_runner::interrupts::service_sync_interrupt();
});

esp_app_desc!();

#[main]
fn main() -> ! {
    let mut reporter = Reporter::new(
        BUILD_RUN_ID,
        "rx-sync",
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
    let _clock_enable = enable_wt32_oscillator(peripherals.GPIO16);
    let mut delay = Delay::new();
    let emac = EMAC.init(Emac::new());

    if let Err(error) = bring_up(emac, &mut delay) {
        fail_all(&mut reporter, TEST_IDS, error.reason());
        let _ = reporter.finish();
        terminal_idle();
    }
    emac.bind_interrupt(RX_SYNC_EMAC_IRQ);

    reporter.observation(
        "rx.expected_dut_mac",
        u64::from_be_bytes([
            0, 0, DUT_MAC[0], DUT_MAC[1], DUT_MAC[2], DUT_MAC[3], DUT_MAC[4], DUT_MAC[5],
        ]),
    );
    let initial_mac = MacSnapshot::capture();
    let strict_filter =
        initial_mac.primary_address_matches(DUT_MAC) && (initial_mac.frame_filter & 1) == 0;
    reporter.observation(
        "rx-sync.promiscuous-before",
        u8::from((initial_mac.frame_filter & 1) != 0),
    );
    reporter.observation("rx-sync.mac-high-before", initial_mac.address_high);
    reporter.observation("rx-sync.mac-low-before", initial_mac.address_low);

    let exhaustion_start = INTERRUPTS.snapshot();
    reporter.ready("fill_rx");
    let exhausted = wait_for_exhaustion(&mut delay, exhaustion_start);
    observe_exhaustion(&reporter, exhausted);

    let quiet = observe_quiet_window(&mut delay);
    observe_window(
        &reporter,
        "rx-sync.quiet_heartbeat",
        WindowKind::Quiet,
        quiet,
    );
    record_condition(
        &mut reporter,
        "rx-sync.quiet",
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
        "rx-sync.flood_heartbeat",
        flood_start,
        BUILD_RUN_ID,
        2,
    );
    observe_window(
        &reporter,
        "rx-sync.flood_heartbeats",
        WindowKind::Flood,
        flood,
    );
    record_condition(
        &mut reporter,
        "rx-sync.flood",
        flood_interrupt_bound(flood),
        "isr-growth",
    );
    record_condition(
        &mut reporter,
        "rx-sync.heartbeat",
        flood.heartbeats >= FLOOD_WINDOW_MS / 100,
        "heartbeat-stalled",
    );

    let mut receive_buffer = [0u8; BUFFER_SIZE];
    let drained = drain_fill_frames(
        emac,
        &mut receive_buffer,
        RxSuiteCode::Sync,
        BUILD_RUN_ID,
        1,
    );
    reporter.observation("rx-sync.drained", drained.drained);
    reporter.observation("rx-sync.tagged", drained.tagged);
    reporter.observation("rx-sync.sequence_mask", drained.sequence_mask);
    record_condition(
        &mut reporter,
        "rx-sync.exhausted",
        strict_filter && exhausted.passed() && drained.fill_passed(),
        "exhaustion-or-tag",
    );
    let restored_before_sentinel = RxSnapshot::capture();
    observe_recovery_snapshot(
        &reporter,
        [
            "rx-sync.after-drain-restored",
            "rx-sync.after-drain-status",
            "rx-sync.after-drain-descriptor-valid",
            "rx-sync.after-drain-dma-owned",
            "rx-sync.after-drain-process-state",
        ],
        restored_before_sentinel,
    );

    reporter.ready("send_sentinel");
    let sentinel = wait_for_action(
        emac,
        &mut receive_buffer,
        &mut delay,
        RxSuiteCode::Sync,
        RxActionCode::Sentinel,
        BUILD_RUN_ID,
        3,
        SENTINEL_TIMEOUT_MS,
    );
    reporter.observation("rx-sync.sentinel-elapsed-ms", sentinel.elapsed_ms);
    let final_rx = RxSnapshot::capture();
    observe_recovery_snapshot(
        &reporter,
        [
            "rx-sync.after-sentinel-restored",
            "rx-sync.after-sentinel-status",
            "rx-sync.after-sentinel-descriptor-valid",
            "rx-sync.after-sentinel-dma-owned",
            "rx-sync.after-sentinel-process-state",
        ],
        final_rx,
    );
    record_condition(
        &mut reporter,
        "rx-sync.recovered",
        ring_is_restored(restored_before_sentinel)
            && sentinel.matched
            && sentinel.elapsed_ms <= SENTINEL_TIMEOUT_MS
            && ring_is_restored(final_rx)
            && INTERRUPTS.snapshot().fbi == 0,
        "sentinel-timeout",
    );

    reporter.observation("rx-sync.quiet_window_ms", QUIET_WINDOW_MS);
    let _ = reporter.finish();
    terminal_idle()
}

fn ring_is_restored(snapshot: RxSnapshot) -> bool {
    snapshot.descriptor_sample_valid && snapshot.dma_owned_descriptors == RX_DESCRIPTORS as u8
}

fn observe_recovery_snapshot(reporter: &Reporter<'_>, names: [&str; 5], snapshot: RxSnapshot) {
    reporter.observation(names[0], u8::from(ring_is_restored(snapshot)));
    reporter.observation(names[1], snapshot.status);
    reporter.observation(names[2], u8::from(snapshot.descriptor_sample_valid));
    reporter.observation(names[3], snapshot.dma_owned_descriptors);
    reporter.observation(names[4], snapshot.process_state);
}
