//! Dedicated reset lifecycle release validation firmware.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::main;
use esp_hal::rtc_cntl::SocResetReason;
use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01;
use ph_esp32_mac::esp_hal::{EmacExt, Priority};
use ph_esp32_mac::{ConfigError, Emac, Error, State};
use ph_esp32_mac_qa_runner::frames::{
    ACTION_INJECT_UNICAST, SUITE_RESET, StimulusIdentity, decode_stimulus,
};
use ph_esp32_mac_qa_runner::interrupts::INTERRUPTS;
use ph_esp32_mac_qa_runner::snapshots::{MacSnapshot, RccSnapshot, RxSnapshot};
use ph_esp32_mac_qa_runner::{
    BUILD_GIT_COMMIT, BUILD_RUN_ID, EMAC, Reporter, RunMode, TestResult, reset_reason_token,
    terminal_idle,
};

esp_app_desc!();

const DUT_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x01, 0x02];
const LIFECYCLE_CYCLES: u32 = 20;
const EXPECTED_SEQUENCE_BITS: u32 = u32::MAX;
const UNICAST_TIMEOUT_MS: u32 = 3_000;
const FRAME_FILTER_PROMISCUOUS: u32 = 1;
const EXT_CLOCK_REQUIRED: u32 = (1 << 0) | (1 << 3) | (1 << 4) | (1 << 5);
const EXT_CLOCK_INTERNAL: u32 = 1 << 1;
const PHY_INTERFACE_MASK: u32 = 0x7 << 13;
const PHY_INTERFACE_RMII: u32 = 4 << 13;
const RAM_POWER_DOWN_MASK: u32 = 0x03;

const WARM_MARKER_MAGIC: u32 = 0x5048_5157;
const WARM_MARKER_SALT: u32 = 0xa17e_5e7a;
const WARM_MARKER_WORDS: usize = 5;

#[esp_hal::ram(unstable(rtc_fast, persistent))]
static mut WARM_MARKER: [u32; WARM_MARKER_WORDS] = [0; WARM_MARKER_WORDS];

ph_esp32_mac::emac_isr!(RESET_IRQ, Priority::Priority1, {
    ph_esp32_mac_qa_runner::interrupts::service_sync_interrupt();
});

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let boot = prepare_boot();
    let mut reporter = Reporter::new(
        BUILD_RUN_ID,
        "reset",
        BUILD_GIT_COMMIT,
        RunMode::Release,
        reset_reason_token(),
    );
    reporter.start();
    reporter.observation("reset.warm-mode", u8::from(boot.warm));
    reporter.observation("reset.boot-iteration", boot.iteration);
    reporter.observation("reset.marker-valid", u8::from(boot.valid));

    // Keep the reference oscillator off for the failure/retry sequence. Nothing
    // above this point enables the EMAC DPORT clock or accesses EMAC registers.
    let mut clock_enable = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    Delay::new().delay_millis(Wt32Eth01::OSC_STARTUP_MS);

    let before_init = RccSnapshot::capture();
    reporter.observation(
        "reset.clock-before-init",
        before_init.peripheral_clock_enable,
    );
    reporter.observation(
        "reset.reset-before-init",
        before_init.peripheral_reset_enable,
    );

    let precondition_valid = !before_init.emac_clock_enabled && !before_init.emac_reset_asserted;
    let (missing_clock_failed, retry_succeeded) = missing_clock_retry(&mut clock_enable);
    reporter.observation("reset.precondition-valid", u8::from(precondition_valid));
    reporter.observation("reset.missing-clock-failed", u8::from(missing_clock_failed));
    reporter.observation("reset.retry-succeeded", u8::from(retry_succeeded));
    record_gate(
        &mut reporter,
        "reset.missing-clock-retry",
        precondition_valid && missing_clock_failed && retry_succeeded,
        "missing-clock-sequence",
    );

    let (cycles_ok, cycles_complete, second_init_rejected) =
        lifecycle_matrix(&mut reporter, boot.valid);
    reporter.observation("reset.cycles-complete", cycles_complete);
    record_gate(
        &mut reporter,
        "reset.lifecycle",
        cycles_ok && cycles_complete == LIFECYCLE_CYCLES,
        "lifecycle-readback",
    );
    record_gate(
        &mut reporter,
        "reset.second-init",
        second_init_rejected,
        "second-init-accepted",
    );

    let unicast = validate_boot_unicast(&mut reporter);
    reporter.observation("reset.unicast-seen", unicast.sequences.count_ones());
    reporter.observation("reset.unicast-duplicates", unicast.duplicates);
    reporter.observation("reset.unicast-out-of-range", unicast.out_of_range);
    reporter.observation("reset.unicast-receive-errors", unicast.receive_errors);
    reporter.observation("reset.unicast-ri-interrupts", unicast.ri_interrupts);
    record_gate(
        &mut reporter,
        "reset.unicast",
        unicast.complete(),
        "unicast-challenge-failed",
    );

    // Lifecycle behavior cannot establish cross-core atomicity of the DPORT
    // read-modify-write sequence. Keep this release gate fail-closed until the
    // protected implementation and scripted-MMIO regression land together.
    reporter.record_with_reason(
        "reset.atomicity",
        TestResult::Blocked,
        true,
        "reset-atomicity",
    );
    let _ = reporter.finish();
    terminal_idle()
}

#[derive(Clone, Copy)]
struct BootValidation {
    warm: bool,
    valid: bool,
    iteration: u32,
}

fn prepare_boot() -> BootValidation {
    let iteration = boot_iteration();
    if option_env!("PHQA_BOOT_MODE") != Some("warm") {
        clear_warm_marker();
        return BootValidation {
            warm: false,
            valid: true,
            iteration,
        };
    }

    let expected = warm_marker(iteration);
    let observed = read_warm_marker();
    if observed != expected {
        write_warm_marker(expected);
        esp_hal::system::software_reset();
    }

    let valid_reset = matches!(
        esp_hal::system::reset_reason(),
        Some(SocResetReason::CoreSw)
    );
    clear_warm_marker();
    BootValidation {
        warm: true,
        valid: valid_reset,
        iteration,
    }
}

fn boot_iteration() -> u32 {
    let Some(value) = option_env!("PHQA_BOOT_ITERATION") else {
        return 0;
    };
    let mut parsed = 0u32;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return u32::MAX;
        }
        parsed = match parsed
            .checked_mul(10)
            .and_then(|number| number.checked_add(u32::from(byte - b'0')))
        {
            Some(number) => number,
            None => return u32::MAX,
        };
    }
    parsed
}

fn warm_marker(iteration: u32) -> [u32; WARM_MARKER_WORDS] {
    let run_hash = run_id_hash();
    let inverse = !iteration;
    let checksum = WARM_MARKER_MAGIC ^ run_hash ^ iteration ^ inverse ^ WARM_MARKER_SALT;
    [WARM_MARKER_MAGIC, run_hash, iteration, inverse, checksum]
}

fn run_id_hash() -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in BUILD_RUN_ID.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn read_warm_marker() -> [u32; WARM_MARKER_WORDS] {
    let mut marker = [0u32; WARM_MARKER_WORDS];
    let source = core::ptr::addr_of_mut!(WARM_MARKER).cast::<u32>();
    for (index, word) in marker.iter_mut().enumerate() {
        // SAFETY: `source` addresses the five-word RTC-fast static. Volatile
        // access is required because startup deliberately preserves it.
        *word = unsafe { core::ptr::read_volatile(source.add(index)) };
    }
    marker
}

fn write_warm_marker(marker: [u32; WARM_MARKER_WORDS]) {
    let destination = core::ptr::addr_of_mut!(WARM_MARKER).cast::<u32>();
    for (index, word) in marker.into_iter().enumerate() {
        // SAFETY: `destination` addresses the five-word RTC-fast static.
        // Each in-bounds word is written once before the system reset.
        unsafe { core::ptr::write_volatile(destination.add(index), word) };
    }
}

fn clear_warm_marker() {
    write_warm_marker([0; WARM_MARKER_WORDS]);
}

fn missing_clock_retry(clock_enable: &mut Output<'_>) -> (bool, bool) {
    let first = critical_section::with(|cs| {
        let mut slot = EMAC.borrow_ref_mut(cs);
        slot.replace(Emac::new());
        slot.as_mut()
            .map(|emac| emac.init(Wt32Eth01::emac_config_with_mac(DUT_MAC), &mut Delay::new()))
    });
    let missing_clock_failed = matches!(first, Some(Err(Error::Config(ConfigError::ResetFailed))));

    clock_enable.set_high();
    Delay::new().delay_millis(Wt32Eth01::OSC_STARTUP_MS);
    let retry_succeeded = critical_section::with(|cs| {
        let mut slot = EMAC.borrow_ref_mut(cs);
        slot.replace(Emac::new());
        slot.as_mut().is_some_and(|emac| {
            emac.init(Wt32Eth01::emac_config_with_mac(DUT_MAC), &mut Delay::new())
                .is_ok()
        })
    });
    (missing_clock_failed, retry_succeeded)
}

fn lifecycle_matrix(reporter: &mut Reporter<'_>, boot_valid: bool) -> (bool, u32, bool) {
    let mut lifecycle_ok = boot_valid;
    let mut completed = 0u32;
    let mut second_init_rejected = false;

    for cycle in 0..LIFECYCLE_CYCLES {
        let Some((second_rejected, rcc, mac, rx)) = run_lifecycle_cycle(cycle) else {
            lifecycle_ok = false;
            break;
        };
        if cycle == 0 {
            second_init_rejected = second_rejected;
        }

        reporter.observation("reset.cycle", cycle);
        reporter.observation("reset.rcc-clock", rcc.peripheral_clock_enable);
        reporter.observation("reset.rcc-reset", rcc.peripheral_reset_enable);
        reporter.observation(
            "reset.rcc-emac-clock-enabled",
            u8::from(rcc.emac_clock_enabled),
        );
        reporter.observation(
            "reset.rcc-emac-reset-asserted",
            u8::from(rcc.emac_reset_asserted),
        );
        reporter.observation("reset.ext-clock", rcc.extension_clock_control);
        reporter.observation("reset.phy-interface", rcc.phy_interface_config);
        reporter.observation("reset.power-down-select", rcc.power_down_select);
        reporter.observation("reset.mac-address", mac_to_u64(mac.address));
        reporter.observation("reset.mac-address-high", mac.address_high);
        reporter.observation("reset.mac-address-low", mac.address_low);
        reporter.observation("reset.mac-config", mac.config);
        reporter.observation("reset.mac-filter", mac.frame_filter);
        reporter.observation("reset.dma-status", rx.status);
        reporter.observation("reset.rx-descriptor-base", rx.descriptor_base);
        reporter.observation("reset.rx-current-descriptor", rx.current_descriptor);
        reporter.observation("reset.rx-current-buffer", rx.current_buffer);
        reporter.observation("reset.rx-dma-owned", rx.dma_owned_descriptors);

        lifecycle_ok &= rcc.emac_clock_enabled
            && !rcc.emac_reset_asserted
            && rcc.extension_valid
            && (rcc.extension_clock_control & EXT_CLOCK_REQUIRED) == EXT_CLOCK_REQUIRED
            && (rcc.extension_clock_control & EXT_CLOCK_INTERNAL) == 0
            && (rcc.phy_interface_config & PHY_INTERFACE_MASK) == PHY_INTERFACE_RMII
            && (rcc.power_down_select & RAM_POWER_DOWN_MASK) == 0
            && mac.primary_address_matches(DUT_MAC)
            && (mac.frame_filter & FRAME_FILTER_PROMISCUOUS) == 0
            && rx.descriptor_sample_valid
            && rx.dma_owned_descriptors == 4;
        completed += 1;
    }

    (lifecycle_ok, completed, second_init_rejected)
}

fn run_lifecycle_cycle(cycle: u32) -> Option<(bool, RccSnapshot, MacSnapshot, RxSnapshot)> {
    critical_section::with(|cs| {
        let mut slot = EMAC.borrow_ref_mut(cs);
        slot.replace(Emac::new());
        let emac = slot.as_mut()?;
        emac.init(Wt32Eth01::emac_config_with_mac(DUT_MAC), &mut Delay::new())
            .ok()?;

        let second_rejected = if cycle == 0 {
            matches!(
                emac.init(Wt32Eth01::emac_config_with_mac(DUT_MAC), &mut Delay::new()),
                Err(Error::Config(ConfigError::AlreadyInitialized))
            )
        } else {
            false
        };

        emac.bind_interrupt(RESET_IRQ);
        emac.start().ok()?;
        if emac.state() != State::Running {
            return None;
        }
        let snapshots = (
            second_rejected,
            RccSnapshot::capture(),
            MacSnapshot::capture(),
            RxSnapshot::capture(),
        );
        emac.stop().ok()?;
        (emac.state() == State::Stopped).then_some(snapshots)
    })
}

#[derive(Clone, Copy, Default)]
struct UnicastObservation {
    sequences: u32,
    duplicates: u32,
    out_of_range: u32,
    receive_errors: u32,
    ri_interrupts: u32,
    ring_restored: bool,
}

impl UnicastObservation {
    const fn complete(self) -> bool {
        self.sequences == EXPECTED_SEQUENCE_BITS
            && self.duplicates == 0
            && self.out_of_range == 0
            && self.receive_errors == 0
            && self.ri_interrupts > 0
            && self.ring_restored
    }
}

fn validate_boot_unicast(reporter: &mut Reporter<'_>) -> UnicastObservation {
    let started = critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs)
            .as_mut()
            .is_some_and(|emac| emac.state() == State::Stopped && emac.start().is_ok())
    });
    if !started {
        return UnicastObservation {
            receive_errors: 1,
            ..UnicastObservation::default()
        };
    }

    let before = INTERRUPTS.snapshot();
    reporter.ready("inject_unicast");
    let delay = Delay::new();
    let mut observation = UnicastObservation::default();
    let mut frame = [0u8; 1600];

    for _ in 0..UNICAST_TIMEOUT_MS {
        loop {
            match poll_frame(&mut frame) {
                FramePoll::Empty => break,
                FramePoll::Error => observation.receive_errors += 1,
                FramePoll::Frame(length) => {
                    let identity = StimulusIdentity {
                        destination: DUT_MAC,
                        suite: SUITE_RESET,
                        action: ACTION_INJECT_UNICAST,
                        run_id: BUILD_RUN_ID,
                        step: 1,
                    };
                    let Some(sequence) = decode_stimulus(&frame[..length], identity) else {
                        continue;
                    };
                    if sequence >= 32 {
                        observation.out_of_range += 1;
                        continue;
                    }
                    let bit = 1u32 << sequence;
                    if (observation.sequences & bit) != 0 {
                        observation.duplicates += 1;
                    } else {
                        observation.sequences |= bit;
                    }
                }
            }
        }
        if observation.sequences == EXPECTED_SEQUENCE_BITS {
            break;
        }
        delay.delay_millis(1);
    }

    observation.ri_interrupts = INTERRUPTS.snapshot().since(before).ri;
    let after = RxSnapshot::capture();
    reporter.observation("reset.challenge-dma-status", after.status);
    reporter.observation("reset.challenge-rx-owned", after.dma_owned_descriptors);
    reporter.observation("reset.challenge-descriptor-base", after.descriptor_base);
    observation.ring_restored = after.descriptor_sample_valid && after.dma_owned_descriptors == 4;
    reporter.observation(
        "reset.challenge-ring-restored",
        u8::from(observation.ring_restored),
    );
    let _ = critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs)
            .as_mut()
            .and_then(|emac| emac.stop().ok())
    });
    observation
}

enum FramePoll {
    Empty,
    Frame(usize),
    Error,
}

fn poll_frame(buffer: &mut [u8]) -> FramePoll {
    critical_section::with(|cs| {
        let mut slot = EMAC.borrow_ref_mut(cs);
        let Some(emac) = slot.as_mut() else {
            return FramePoll::Error;
        };
        if !emac.rx_available() {
            return FramePoll::Empty;
        }
        match emac.receive(buffer) {
            Ok(length) => FramePoll::Frame(length),
            Err(_) => FramePoll::Error,
        }
    })
}

const fn mac_to_u64(mac: [u8; 6]) -> u64 {
    ((mac[0] as u64) << 40)
        | ((mac[1] as u64) << 32)
        | ((mac[2] as u64) << 24)
        | ((mac[3] as u64) << 16)
        | ((mac[4] as u64) << 8)
        | mac[5] as u64
}

fn record_gate(reporter: &mut Reporter<'_>, id: &str, passed: bool, reason: &str) {
    if passed {
        reporter.record(id, TestResult::Pass, true);
    } else {
        reporter.record_with_reason(id, TestResult::Fail, true, reason);
    }
}
