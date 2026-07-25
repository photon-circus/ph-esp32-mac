//! Shared, allocation-free support for the dedicated RX validation suites.
//!
//! The release firmware uses this module only for board bring-up, raw PHQA
//! frame recognition, register/counter observations, and bounded synchronous
//! waits. It deliberately does not use the driver's removable debug helpers.

use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::Uart;
use ph_esp32_mac::esp_hal::{EmacBuilder, EmacPhyBundle};
use ph_esp32_mac::{Emac, Error};

use crate::frames::{StimulusIdentity, decode_stimulus};
use crate::interrupts::{INTERRUPTS, InterruptSnapshot};
use crate::snapshots::RxSnapshot;
use crate::{Reporter, TestResult};

/// RX descriptor count fixed by the release acceptance criteria.
pub const RX_DESCRIPTORS: usize = 4;
/// TX descriptor count used by the RX validation firmware.
pub const TX_DESCRIPTORS: usize = 4;
/// Per-descriptor buffer size used by the RX validation firmware.
pub const BUFFER_SIZE: usize = 1600;
/// Locally administered address configured in the example lab file.
pub const DUT_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x01, 0x02];
/// Maximum time allowed for a four-descriptor exhaustion transition.
pub const EXHAUSTION_TIMEOUT_MS: u32 = 2_000;
/// Required quiet observation window after descriptor exhaustion.
pub const QUIET_WINDOW_MS: u32 = 1_000;
/// Required flood observation window.
pub const FLOOD_WINDOW_MS: u32 = 2_000;
/// Deadline for the host's post-flood serial acknowledgement.
pub const FLOOD_ACK_TIMEOUT_MS: u32 = 3_000;
/// Host-declared frame count in the flood stimulus.
pub const FLOOD_FRAME_COUNT: u32 = 4_096;
/// Maximum permitted ISR entries above the host-declared flood frame count.
pub const FLOOD_ISR_SLACK: u32 = 8;
/// Maximum time allowed for a sentinel receive after descriptor recycling.
pub const SENTINEL_TIMEOUT_MS: u32 = 1_000;

/// Numeric suite discriminator embedded in every host stimulus frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RxSuiteCode {
    /// Synchronous RX suite.
    Sync = 4,
    /// Raw async RX suite.
    Async = 5,
    /// embassy-net RX suite.
    Embassy = 6,
}

/// Numeric action discriminator embedded in every RX host stimulus frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RxActionCode {
    /// Fill the four-entry receive ring without consuming it.
    Fill = 9,
    /// Send the controlled 4096-frame flood.
    Flood = 10,
    /// Send the first recovery sentinel.
    Sentinel = 11,
    /// Send the second raw-async wake sentinel.
    SecondSentinel = 12,
    /// Send a fixed-IP UDP echo challenge.
    UdpEcho = 13,
    /// Enable the lab's DHCP service.
    Dhcp = 14,
}

/// Validated metadata carried by a fixed-format PHQA Ethernet frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QaFrameTag {
    /// Host sequence number for this READY action.
    pub sequence: u32,
}

/// Results from recycling the exhausted ring and validating its four frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrainEvidence {
    /// Total complete descriptors/frames recycled.
    pub drained: u32,
    /// Number of frames carrying the expected suite/action/run/step tag.
    pub tagged: u32,
    /// Bit set for each unique expected sequence in the range zero through three.
    pub sequence_mask: u8,
}

impl DrainEvidence {
    /// Whether the exhausted ring contained exactly the four requested frames.
    pub const fn fill_passed(self) -> bool {
        self.drained == RX_DESCRIPTORS as u32
            && self.tagged == RX_DESCRIPTORS as u32
            && self.sequence_mask == 0x0f
    }
}

/// Error stage reported when deterministic WT32-ETH01 bring-up fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BringUpError {
    /// EMAC initialization failed.
    EmacInit,
    /// PHY initialization or link negotiation failed.
    PhyLink,
    /// Starting the MAC/DMA engines failed.
    EmacStart,
}

impl BringUpError {
    /// Stable protocol reason token for the failed stage.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::EmacInit => "emac-init",
            Self::PhyLink => "phy-link",
            Self::EmacStart => "emac-start",
        }
    }
}

/// Combined DMA-register and real-ISR evidence for an exhaustion transition.
#[derive(Clone, Copy, Debug)]
pub struct ExhaustionEvidence {
    /// Latest raw DMA receive snapshot.
    pub rx: RxSnapshot,
    /// Interrupt deltas since the suite armed the host stimulus.
    pub interrupts: InterruptSnapshot,
}

impl ExhaustionEvidence {
    /// Whether all descriptor-exhaustion release gates were observed.
    pub const fn passed(self) -> bool {
        self.rx.descriptor_sample_valid
            && self.rx.dma_owned_descriptors == 0
            && self.rx.process_state == 4
            && self.rx.buffer_unavailable_interrupt_enabled
            && self.rx.abnormal_summary_interrupt_enabled
            && self.interrupts.ru > 0
            && self.interrupts.ais > 0
            && self.interrupts.fbi == 0
    }
}

/// Combined interrupt evidence collected over a quiet or flood window.
#[derive(Clone, Copy, Debug)]
pub struct WindowEvidence {
    /// Number of main-loop heartbeat iterations completed in the window.
    pub heartbeats: u32,
    /// Interrupt deltas across the window.
    pub interrupts: InterruptSnapshot,
    /// Whether the host acknowledged completion after its final injection.
    pub acknowledged: bool,
    /// Hardware-timer duration from READY until the acknowledgement.
    pub elapsed_ms: u32,
}

/// Kind of bounded interrupt observation window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    /// One second without host traffic.
    Quiet,
    /// Two-second, 4096-frame host flood with a completion acknowledgement.
    Flood,
}

/// Result of a deadline-bounded, tagged frame wait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionEvidence {
    /// Whether the expected frame arrived.
    pub matched: bool,
    /// Hardware-timer elapsed milliseconds.
    pub elapsed_ms: u32,
}

/// Initialize a four-descriptor WT32-ETH01 EMAC and require a valid PHY link.
///
/// The oscillator-enable GPIO must already be held high by the caller.
pub fn bring_up(
    emac: &mut Emac<RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>,
    delay: &mut Delay,
) -> Result<(), BringUpError> {
    EmacBuilder::wt32_eth01_with_mac(emac, DUT_MAC)
        .init(delay)
        .map_err(|_error| BringUpError::EmacInit)?;

    {
        let mut bundle = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
        bundle
            .init_and_wait_link_up(delay, 10_000, 100)
            .map_err(|_error| BringUpError::PhyLink)?;
    }

    emac.set_promiscuous(false);
    emac.start().map_err(|_error| BringUpError::EmacStart)
}

/// Emit a required failure for every suite ID after a shared setup failure.
pub fn fail_all(reporter: &mut Reporter<'_>, ids: &[&str], reason: &str) {
    for id in ids {
        reporter.record_with_reason(id, TestResult::Fail, true, reason);
    }
}

/// Emit one required PASS/FAIL condition without an implicit success path.
pub fn record_condition(reporter: &mut Reporter<'_>, id: &str, passed: bool, failure_reason: &str) {
    if passed {
        reporter.record(id, TestResult::Pass, true);
    } else {
        reporter.record_with_reason(id, TestResult::Fail, true, failure_reason);
    }
}

/// Wait for the hardware and ISR to prove RX-ring exhaustion.
pub fn wait_for_exhaustion(delay: &mut Delay, before: InterruptSnapshot) -> ExhaustionEvidence {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(u64::from(EXHAUSTION_TIMEOUT_MS));
    let mut evidence = ExhaustionEvidence {
        rx: RxSnapshot::capture(),
        interrupts: INTERRUPTS.snapshot().since(before),
    };

    while Instant::now() < deadline {
        evidence = ExhaustionEvidence {
            rx: RxSnapshot::capture(),
            interrupts: INTERRUPTS.snapshot().since(before),
        };
        if evidence.passed() {
            break;
        }
        delay.delay_millis(1);
    }

    evidence
}

/// Observe the one-second no-traffic RU interrupt bound.
pub fn observe_quiet_window(delay: &mut Delay) -> WindowEvidence {
    let before = INTERRUPTS.snapshot();
    delay.delay_millis(QUIET_WINDOW_MS);
    WindowEvidence {
        heartbeats: 1,
        interrupts: INTERRUPTS.snapshot().since(before),
        acknowledged: false,
        elapsed_ms: QUIET_WINDOW_MS,
    }
}

/// Observe the flood until its exact post-injection acknowledgement arrives.
///
/// The controller writes the acknowledgement only after all 4096 frames have
/// been injected over two seconds and the NIC transmit path has been allowed
/// to drain. Keeping the receive ring exhausted until that serial message is
/// recognized closes the READY-to-injection timing gap.
pub fn observe_flood_window(
    reporter: &Reporter<'_>,
    delay: &mut Delay,
    control: &mut Uart<'_, Blocking>,
    heartbeat_name: &str,
    before: InterruptSnapshot,
    run_id: &str,
    ready_step: u32,
) -> WindowEvidence {
    const TICK_MS: u32 = 100;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(u64::from(FLOOD_ACK_TIMEOUT_MS));
    let mut line = [0u8; CONTROL_LINE_CAPACITY];
    let mut line_len = 0usize;
    let mut overflow = false;
    let mut heartbeats = 0u32;

    while Instant::now() < deadline {
        if control.read_ready() {
            let mut bytes = [0u8; 32];
            match control.read_buffered(&mut bytes) {
                Ok(read) => {
                    for byte in &bytes[..read] {
                        match *byte {
                            b'\r' => {}
                            b'\n' => {
                                let acknowledged = !overflow
                                    && control_ack_matches(
                                        &line[..line_len],
                                        run_id,
                                        ready_step,
                                        "flood_rx",
                                    );
                                line_len = 0;
                                overflow = false;
                                if acknowledged {
                                    return WindowEvidence {
                                        heartbeats,
                                        interrupts: INTERRUPTS.snapshot().since(before),
                                        acknowledged: true,
                                        elapsed_ms: elapsed_millis(started),
                                    };
                                }
                            }
                            byte if !overflow && line_len < line.len() => {
                                line[line_len] = byte;
                                line_len += 1;
                            }
                            _ => overflow = true,
                        }
                    }
                }
                Err(_error) => overflow = true,
            }
        } else {
            delay.delay_millis(TICK_MS);
            heartbeats = heartbeats.wrapping_add(1);
            reporter.observation(heartbeat_name, heartbeats);
        }
    }

    WindowEvidence {
        heartbeats,
        interrupts: INTERRUPTS.snapshot().since(before),
        acknowledged: false,
        elapsed_ms: elapsed_millis(started),
    }
}

/// Discard stale host-control bytes before emitting a new READY record.
pub fn drain_control_input(control: &mut Uart<'_, Blocking>) {
    let mut bytes = [0u8; 32];
    while control.read_ready() {
        if control.read_buffered(&mut bytes).is_err() {
            break;
        }
    }
}

/// Wait for an exact host acknowledgement without allocating.
pub fn wait_for_control_ack(
    control: &mut Uart<'_, Blocking>,
    delay: &mut Delay,
    run_id: &str,
    ready_step: u32,
    action: &str,
    timeout_ms: u32,
) -> ActionEvidence {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(u64::from(timeout_ms));
    let mut line = [0u8; CONTROL_LINE_CAPACITY];
    let mut line_len = 0usize;
    let mut overflow = false;

    while Instant::now() < deadline {
        if !control.read_ready() {
            delay.delay_millis(1);
            continue;
        }
        let mut bytes = [0u8; 32];
        match control.read_buffered(&mut bytes) {
            Ok(read) => {
                for byte in &bytes[..read] {
                    match *byte {
                        b'\r' => {}
                        b'\n' => {
                            let matched = !overflow
                                && control_ack_matches(
                                    &line[..line_len],
                                    run_id,
                                    ready_step,
                                    action,
                                );
                            line_len = 0;
                            overflow = false;
                            if matched {
                                return ActionEvidence {
                                    matched: true,
                                    elapsed_ms: elapsed_millis(started),
                                };
                            }
                        }
                        byte if !overflow && line_len < line.len() => {
                            line[line_len] = byte;
                            line_len += 1;
                        }
                        _ => overflow = true,
                    }
                }
            }
            Err(_error) => overflow = true,
        }
    }

    ActionEvidence {
        matched: false,
        elapsed_ms: elapsed_millis(started),
    }
}

/// Emit the raw register and ISR values used by the exhaustion assertion.
pub fn observe_exhaustion(reporter: &Reporter<'_>, evidence: ExhaustionEvidence) {
    reporter.observation(
        "rx.descriptor_sample_valid",
        u8::from(evidence.rx.descriptor_sample_valid),
    );
    reporter.observation("rx.dma_owned", evidence.rx.dma_owned_descriptors);
    reporter.observation("rx.process_state", evidence.rx.process_state);
    reporter.observation("rx.ru_pending", u8::from(evidence.rx.buffer_unavailable));
    reporter.observation("rx.ru_observed", u8::from(evidence.interrupts.ru > 0));
    reporter.observation(
        "rx.rue_enabled",
        u8::from(evidence.rx.buffer_unavailable_interrupt_enabled),
    );
    reporter.observation(
        "rx.aie_enabled",
        u8::from(evidence.rx.abnormal_summary_interrupt_enabled),
    );
    reporter.observation("rx.status_raw", evidence.rx.status);
    reporter.observation("rx.interrupt_enable_raw", evidence.rx.interrupt_enable);
    reporter.observation("rx.isr_total", evidence.interrupts.total);
    reporter.observation("rx.isr_ri", evidence.interrupts.ri);
    reporter.observation("rx.isr_ru", evidence.interrupts.ru);
    reporter.observation("rx.isr_ais", evidence.interrupts.ais);
    reporter.observation("rx.isr_fbi", evidence.interrupts.fbi);
}

/// Emit interrupt deltas from a quiet or flood observation window.
pub fn observe_window(
    reporter: &Reporter<'_>,
    heartbeat_name: &str,
    kind: WindowKind,
    evidence: WindowEvidence,
) {
    reporter.observation(heartbeat_name, evidence.heartbeats);
    let names = match kind {
        WindowKind::Quiet => [
            "rx.quiet_isr_total",
            "rx.quiet_isr_ri",
            "rx.quiet_isr_ru",
            "rx.quiet_isr_ais",
            "rx.quiet_isr_fbi",
        ],
        WindowKind::Flood => [
            "rx.flood_isr_total",
            "rx.flood_isr_ri",
            "rx.flood_isr_ru",
            "rx.flood_isr_ais",
            "rx.flood_isr_fbi",
        ],
    };
    reporter.observation(names[0], evidence.interrupts.total);
    reporter.observation(names[1], evidence.interrupts.ri);
    reporter.observation(names[2], evidence.interrupts.ru);
    reporter.observation(names[3], evidence.interrupts.ais);
    reporter.observation(names[4], evidence.interrupts.fbi);
    if kind == WindowKind::Flood {
        reporter.observation("rx.flood_ack", u8::from(evidence.acknowledged));
        reporter.observation("rx.flood_elapsed_ms", evidence.elapsed_ms);
    }
}

/// Drain all complete frames currently held by the four-entry RX ring.
///
/// The returned count includes errored frames whose descriptors the driver
/// recycled while reporting an error.
pub fn drain_ready(
    emac: &mut Emac<RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>,
    buffer: &mut [u8; BUFFER_SIZE],
) -> u32 {
    let mut drained = 0u32;
    while emac.rx_available() && drained < FLOOD_FRAME_COUNT + RX_DESCRIPTORS as u32 {
        let _result: Result<usize, Error> = emac.receive(buffer);
        drained = drained.wrapping_add(1);
    }
    drained
}

/// Drain and authenticate the four frames used to exhaust the RX ring.
pub fn drain_fill_frames(
    emac: &mut Emac<RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>,
    buffer: &mut [u8; BUFFER_SIZE],
    suite: RxSuiteCode,
    run_id: &str,
    ready_step: u32,
) -> DrainEvidence {
    let mut evidence = DrainEvidence::default();
    while emac.rx_available() && evidence.drained < RX_DESCRIPTORS as u32 + 1 {
        evidence.drained = evidence.drained.wrapping_add(1);
        if let Ok(length) = emac.receive(buffer)
            && let Some(tag) = parse_qa_frame(
                &buffer[..length],
                suite,
                RxActionCode::Fill,
                run_id,
                ready_step,
            )
            && tag.sequence < RX_DESCRIPTORS as u32
        {
            evidence.tagged = evidence.tagged.wrapping_add(1);
            evidence.sequence_mask |= 1u8 << tag.sequence;
        }
    }
    evidence
}

/// Wait for one valid, action-tagged PHQA Ethernet frame.
pub fn wait_for_action(
    emac: &mut Emac<RX_DESCRIPTORS, TX_DESCRIPTORS, BUFFER_SIZE>,
    buffer: &mut [u8; BUFFER_SIZE],
    delay: &mut Delay,
    suite: RxSuiteCode,
    action: RxActionCode,
    run_id: &str,
    ready_step: u32,
    timeout_ms: u32,
) -> ActionEvidence {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(u64::from(timeout_ms));
    while Instant::now() < deadline {
        if emac.rx_available() {
            if let Ok(length) = emac.receive(buffer)
                && parse_qa_frame(&buffer[..length], suite, action, run_id, ready_step)
                    .is_some_and(|tag| tag.sequence == 0)
            {
                return ActionEvidence {
                    matched: true,
                    elapsed_ms: elapsed_millis(started),
                };
            }
        } else {
            delay.delay_millis(1);
        }
    }
    ActionEvidence {
        matched: false,
        elapsed_ms: elapsed_millis(started),
    }
}

/// Validate the fixed 64-byte PHQA Ethernet stimulus schema.
pub fn parse_qa_frame(
    frame: &[u8],
    expected_suite: RxSuiteCode,
    expected_action: RxActionCode,
    expected_run_id: &str,
    expected_step: u32,
) -> Option<QaFrameTag> {
    Some(QaFrameTag {
        sequence: decode_stimulus(
            frame,
            StimulusIdentity {
                destination: DUT_MAC,
                suite: expected_suite as u8,
                action: expected_action as u8,
                run_id: expected_run_id,
                step: expected_step,
            },
        )?,
    })
}

/// Check that every descriptor has been recycled back to DMA ownership.
pub fn ring_restored() -> bool {
    let snapshot = RxSnapshot::capture();
    snapshot.descriptor_sample_valid
        && usize::from(snapshot.dma_owned_descriptors) == RX_DESCRIPTORS
}

fn elapsed_millis(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// Check the acceptance bound on ISR growth during the host flood.
pub const fn flood_interrupt_bound(evidence: WindowEvidence) -> bool {
    evidence.acknowledged
        && evidence.elapsed_ms >= FLOOD_WINDOW_MS - 100
        && evidence.elapsed_ms <= FLOOD_ACK_TIMEOUT_MS
        && evidence.interrupts.total <= FLOOD_FRAME_COUNT + FLOOD_ISR_SLACK
        && evidence.interrupts.fbi == 0
}

const CONTROL_LINE_CAPACITY: usize = 96;
const CONTROL_PREFIX: &[u8] = b"PHQA|1|ACK|run=";

fn control_ack_matches(line: &[u8], run_id: &str, step: u32, action: &str) -> bool {
    let mut expected = [0u8; CONTROL_LINE_CAPACITY];
    let Some(length) = build_control_ack(&mut expected, run_id, step, action) else {
        return false;
    };
    line == &expected[..length]
}

fn build_control_ack(
    output: &mut [u8; CONTROL_LINE_CAPACITY],
    run_id: &str,
    step: u32,
    action: &str,
) -> Option<usize> {
    let mut length = 0usize;
    append_control(output, &mut length, CONTROL_PREFIX)?;
    append_control(output, &mut length, run_id.as_bytes())?;
    append_control(output, &mut length, b"|step=")?;

    let mut digits = [0u8; 10];
    let mut value = step;
    let mut digit_count = 0usize;
    loop {
        digits[digit_count] = b'0' + u8::try_from(value % 10).ok()?;
        digit_count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for digit in digits[..digit_count].iter().rev() {
        append_control(output, &mut length, core::slice::from_ref(digit))?;
    }

    append_control(output, &mut length, b"|action=")?;
    append_control(output, &mut length, action.as_bytes())?;
    Some(length)
}

fn append_control(
    output: &mut [u8; CONTROL_LINE_CAPACITY],
    length: &mut usize,
    bytes: &[u8],
) -> Option<()> {
    let end = length.checked_add(bytes.len())?;
    output.get_mut(*length..end)?.copy_from_slice(bytes);
    *length = end;
    Some(())
}
