//! Dedicated raw-async RX-exhaustion release validation firmware.

#![no_std]
#![no_main]

use core::future::Future;
use core::sync::atomic::{AtomicU32, Ordering};
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    main,
    time::{Duration, Instant},
    uart::{Config as UartConfig, Uart},
};
use ph_esp32_mac::esp_hal::{EmacExt, Priority};
use ph_esp32_mac::unsafe_registers::DmaRegs;
use ph_esp32_mac::{AsyncEmacExt, Emac, InterruptStatus};
use ph_esp32_mac_qa_runner::interrupts::INTERRUPTS;
use ph_esp32_mac_qa_runner::rx_harness::{
    BUFFER_SIZE, DUT_MAC, FLOOD_WINDOW_MS, RX_DESCRIPTORS, RxActionCode, RxSuiteCode,
    SENTINEL_TIMEOUT_MS, TX_DESCRIPTORS, WindowKind, bring_up, drain_control_input,
    drain_fill_frames, fail_all, flood_interrupt_bound, observe_exhaustion, observe_flood_window,
    observe_quiet_window, observe_window, parse_qa_frame, record_condition, wait_for_exhaustion,
};
use ph_esp32_mac_qa_runner::snapshots::{MacSnapshot, RxSnapshot};
use ph_esp32_mac_qa_runner::{
    BUILD_GIT_COMMIT, BUILD_RUN_ID, Reporter, RunMode, enable_wt32_oscillator, reset_reason_token,
    terminal_idle,
};

const TEST_IDS: &[&str] = &[
    "rx-async.exhausted",
    "rx-async.quiet",
    "rx-async.flood",
    "rx-async.heartbeat",
    "rx-async.recovered",
    "rx-async.second-wake",
];

static WAKE_COUNT: AtomicU32 = AtomicU32::new(0);

ph_esp32_mac::emac_static_async!(
    EMAC,
    ASYNC_STATE,
    RX_DESCRIPTORS,
    TX_DESCRIPTORS,
    BUFFER_SIZE
);

ph_esp32_mac::emac_isr!(RX_ASYNC_EMAC_IRQ, Priority::Priority1, {
    let raw = DmaRegs::status();
    INTERRUPTS.observe(raw);
    let status = InterruptStatus::from_raw(raw);
    ASYNC_STATE.on_interrupt(status);
    DmaRegs::set_status(status.to_raw());
});

esp_app_desc!();

#[derive(Clone, Copy, Debug, Default)]
struct AsyncReceiveEvidence {
    initially_pending: bool,
    woke: bool,
    tagged_frame: bool,
    wake_events: u32,
    elapsed_ms: u32,
}

impl AsyncReceiveEvidence {
    const fn passed(self) -> bool {
        self.initially_pending && self.woke && self.tagged_frame
    }
}

#[main]
fn main() -> ! {
    let mut reporter = Reporter::new(
        BUILD_RUN_ID,
        "rx-async",
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
    emac.bind_interrupt(RX_ASYNC_EMAC_IRQ);
    let initial_mac = MacSnapshot::capture();
    let strict_filter =
        initial_mac.primary_address_matches(DUT_MAC) && (initial_mac.frame_filter & 1) == 0;
    reporter.observation(
        "rx-async.promiscuous-before",
        u8::from((initial_mac.frame_filter & 1) != 0),
    );
    reporter.observation("rx-async.mac-high-before", initial_mac.address_high);
    reporter.observation("rx-async.mac-low-before", initial_mac.address_low);

    let exhaustion_start = INTERRUPTS.snapshot();
    reporter.ready("fill_rx");
    let exhausted = wait_for_exhaustion(&mut delay, exhaustion_start);
    observe_exhaustion(&reporter, exhausted);

    let quiet = observe_quiet_window(&mut delay);
    observe_window(
        &reporter,
        "rx-async.quiet_heartbeat",
        WindowKind::Quiet,
        quiet,
    );
    record_condition(
        &mut reporter,
        "rx-async.quiet",
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
        "rx-async.flood_heartbeat",
        flood_start,
        BUILD_RUN_ID,
        2,
    );
    observe_window(
        &reporter,
        "rx-async.flood_heartbeats",
        WindowKind::Flood,
        flood,
    );
    record_condition(
        &mut reporter,
        "rx-async.flood",
        flood_interrupt_bound(flood),
        "isr-growth",
    );
    record_condition(
        &mut reporter,
        "rx-async.heartbeat",
        flood.heartbeats >= FLOOD_WINDOW_MS / 100,
        "heartbeat-stalled",
    );

    let mut receive_buffer = [0u8; BUFFER_SIZE];
    let drained = drain_fill_frames(
        emac,
        &mut receive_buffer,
        RxSuiteCode::Async,
        BUILD_RUN_ID,
        1,
    );
    reporter.observation("rx-async.drained", drained.drained);
    reporter.observation("rx-async.tagged", drained.tagged);
    reporter.observation("rx-async.sequence_mask", drained.sequence_mask);
    record_condition(
        &mut reporter,
        "rx-async.exhausted",
        strict_filter && exhausted.passed() && drained.fill_passed(),
        "exhaustion-or-tag",
    );
    let after_drain = RxSnapshot::capture();
    observe_ring(
        &reporter,
        [
            "rx-async.after-drain-restored",
            "rx-async.after-drain-status",
            "rx-async.after-drain-descriptor-valid",
            "rx-async.after-drain-dma-owned",
        ],
        after_drain,
    );

    let first = wait_on_receive_future(
        emac,
        &mut receive_buffer,
        &mut reporter,
        &mut delay,
        "send_sentinel",
        RxActionCode::Sentinel,
        3,
    );
    reporter.observation("rx-async.first_pending", u8::from(first.initially_pending));
    reporter.observation("rx-async.first_woke", u8::from(first.woke));
    reporter.observation("rx-async.first-wake-events", first.wake_events);
    reporter.observation("rx-async.first-elapsed-ms", first.elapsed_ms);
    let after_first = RxSnapshot::capture();
    observe_ring(
        &reporter,
        [
            "rx-async.after-first-restored",
            "rx-async.after-first-status",
            "rx-async.after-first-descriptor-valid",
            "rx-async.after-first-dma-owned",
        ],
        after_first,
    );
    record_condition(
        &mut reporter,
        "rx-async.recovered",
        ring_is_restored(after_drain)
            && first.passed()
            && first.elapsed_ms <= SENTINEL_TIMEOUT_MS
            && ring_is_restored(after_first)
            && INTERRUPTS.snapshot().fbi == 0,
        "async-recovery",
    );

    let second = wait_on_receive_future(
        emac,
        &mut receive_buffer,
        &mut reporter,
        &mut delay,
        "send_second_sentinel",
        RxActionCode::SecondSentinel,
        4,
    );
    reporter.observation(
        "rx-async.second_pending",
        u8::from(second.initially_pending),
    );
    reporter.observation("rx-async.second_woke", u8::from(second.woke));
    reporter.observation("rx-async.second-wake-events", second.wake_events);
    reporter.observation("rx-async.second-elapsed-ms", second.elapsed_ms);
    let after_second = RxSnapshot::capture();
    observe_ring(
        &reporter,
        [
            "rx-async.after-second-restored",
            "rx-async.after-second-status",
            "rx-async.after-second-descriptor-valid",
            "rx-async.after-second-dma-owned",
        ],
        after_second,
    );
    record_condition(
        &mut reporter,
        "rx-async.second-wake",
        second.passed()
            && second.elapsed_ms <= SENTINEL_TIMEOUT_MS
            && ring_is_restored(after_second)
            && INTERRUPTS.snapshot().fbi == 0,
        "async-second-wake",
    );

    let _ = reporter.finish();
    terminal_idle()
}

fn wait_on_receive_future(
    emac: &mut Emac<RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>,
    buffer: &mut [u8; BUFFER_SIZE],
    reporter: &mut Reporter<'_>,
    delay: &mut Delay,
    ready_action: &str,
    action_code: RxActionCode,
    ready_step: u32,
) -> AsyncReceiveEvidence {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(u64::from(SENTINEL_TIMEOUT_MS));
    let waker = counting_waker();
    let mut context = Context::from_waker(&waker);
    let (initially_pending, completed, completion_woke, wake_events) = {
        let mut future = core::pin::pin!(emac.receive_async(&ASYNC_STATE, buffer));
        let initially_pending = matches!(future.as_mut().poll(&mut context), Poll::Pending);

        if !initially_pending {
            return AsyncReceiveEvidence::default();
        }

        let mut wake_generation = WAKE_COUNT.load(Ordering::Acquire);
        let mut wake_events = 0u32;
        reporter.ready(ready_action);
        let mut completed = None;
        let mut completion_woke = false;
        while Instant::now() < deadline {
            let current_generation = WAKE_COUNT.load(Ordering::Acquire);
            if current_generation == wake_generation {
                delay.delay_millis(1);
                continue;
            }
            wake_events =
                wake_events.wrapping_add(current_generation.wrapping_sub(wake_generation));
            wake_generation = current_generation;
            match future.as_mut().poll(&mut context) {
                Poll::Ready(Ok(length)) => {
                    completed = Some(Ok(length));
                    completion_woke = true;
                    break;
                }
                Poll::Ready(Err(_error)) => {
                    completed = Some(Err(()));
                    completion_woke = true;
                    break;
                }
                Poll::Pending => {}
            }
        }
        (initially_pending, completed, completion_woke, wake_events)
    };
    let tagged_frame = completed
        .and_then(Result::ok)
        .and_then(|length| {
            parse_qa_frame(
                &buffer[..length],
                RxSuiteCode::Async,
                action_code,
                BUILD_RUN_ID,
                ready_step,
            )
        })
        .is_some_and(|tag| tag.sequence == 0);

    AsyncReceiveEvidence {
        initially_pending,
        woke: completion_woke,
        tagged_frame,
        wake_events,
        elapsed_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
    }
}

fn ring_is_restored(snapshot: RxSnapshot) -> bool {
    snapshot.descriptor_sample_valid && snapshot.dma_owned_descriptors == RX_DESCRIPTORS as u8
}

fn observe_ring(reporter: &Reporter<'_>, names: [&str; 4], snapshot: RxSnapshot) {
    reporter.observation(names[0], u8::from(ring_is_restored(snapshot)));
    reporter.observation(names[1], snapshot.status);
    reporter.observation(names[2], u8::from(snapshot.descriptor_sample_valid));
    reporter.observation(names[3], snapshot.dma_owned_descriptors);
}

fn counting_waker() -> Waker {
    // SAFETY: The vtable never dereferences the null data pointer, owns no
    // resources, and every clone returns the same static behavior.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &COUNTING_WAKER_VTABLE)) }
}

unsafe fn clone_counting_waker(_data: *const ()) -> RawWaker {
    RawWaker::new(core::ptr::null(), &COUNTING_WAKER_VTABLE)
}

unsafe fn wake_counting_waker(_data: *const ()) {
    WAKE_COUNT.fetch_add(1, Ordering::Release);
}

unsafe fn wake_counting_waker_by_ref(_data: *const ()) {
    WAKE_COUNT.fetch_add(1, Ordering::Release);
}

unsafe fn drop_counting_waker(_data: *const ()) {}

static COUNTING_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    clone_counting_waker,
    wake_counting_waker,
    wake_counting_waker_by_ref,
    drop_counting_waker,
);
