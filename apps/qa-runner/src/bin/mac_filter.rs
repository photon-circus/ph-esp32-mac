//! Dedicated MAC-filter release validation firmware.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::delay::Delay;
use esp_hal::main;
use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01;
use ph_esp32_mac::esp_hal::{EmacExt, Priority};
use ph_esp32_mac::{Emac, State};
use ph_esp32_mac_qa_runner::frames::{
    ACTION_INJECT_ALIEN, ACTION_INJECT_BROADCAST, ACTION_INJECT_MULTICAST, ACTION_INJECT_NEW_MAC,
    ACTION_INJECT_OLD_MAC, ACTION_INJECT_UNICAST, SUITE_MAC_FILTER, StimulusIdentity,
    decode_stimulus,
};
use ph_esp32_mac_qa_runner::interrupts::INTERRUPTS;
use ph_esp32_mac_qa_runner::snapshots::MacSnapshot;
use ph_esp32_mac_qa_runner::{
    BUILD_GIT_COMMIT, BUILD_RUN_ID, EMAC, Reporter, RunMode, TestResult, enable_wt32_oscillator,
    reset_reason_token, terminal_idle,
};

esp_app_desc!();

const MAC_A: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x01, 0x02];
const MAC_B: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x01, 0x03];
const MULTICAST: [u8; 6] = [0x01, 0x00, 0x5e, 0x00, 0x00, 0x01];
const BROADCAST: [u8; 6] = [0xff; 6];
const ALIEN: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x01, 0xfe];
const EXPECTED_SEQUENCE_BITS: u32 = u32::MAX;
const ACCEPT_TIMEOUT_MS: u32 = 3_000;
const REJECT_WINDOW_MS: u32 = 1_000;
const FRAME_FILTER_PROMISCUOUS: u32 = 1;
const FRAME_FILTER_PASS_ALL_MULTICAST: u32 = 1 << 4;

ph_esp32_mac::emac_isr!(MAC_FILTER_IRQ, Priority::Priority1, {
    ph_esp32_mac_qa_runner::interrupts::service_sync_interrupt();
});

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _clock_enable = enable_wt32_oscillator(peripherals.GPIO16);
    let mut reporter = Reporter::new(
        BUILD_RUN_ID,
        "mac-filter",
        BUILD_GIT_COMMIT,
        RunMode::Release,
        reset_reason_token(),
    );
    reporter.start();

    let configured = configure_mac();
    let initial = configured.then(MacSnapshot::capture);
    reporter.observation(
        "mac.address-a",
        initial.map_or(0, |snapshot| mac_to_u64(snapshot.address)),
    );
    reporter.observation(
        "mac.address0-high-a",
        initial.map_or(0, |snapshot| snapshot.address_high),
    );
    reporter.observation(
        "mac.address0-low-a",
        initial.map_or(0, |snapshot| snapshot.address_low),
    );
    reporter.observation(
        "mac.frame-filter-a",
        initial.map_or(0, |snapshot| snapshot.frame_filter),
    );
    reporter.observation(
        "mac.additional-filter-count-a",
        initial.map_or(0, |snapshot| snapshot.enabled_additional_filters),
    );
    reporter.observation(
        "mac.address1-high-a",
        initial.map_or(0, |snapshot| snapshot.additional_address_high[0]),
    );
    reporter.observation(
        "mac.address1-low-a",
        initial.map_or(0, |snapshot| snapshot.additional_address_low[0]),
    );
    record_gate(
        &mut reporter,
        "mac.readback",
        initial.is_some_and(|snapshot| {
            snapshot.primary_address_matches(MAC_A)
                && snapshot.enabled_additional_filters == 1
                && snapshot.additional_destination_filter_matches(1, MULTICAST)
        }),
        "macaddr0-mismatch",
    );
    record_gate(
        &mut reporter,
        "mac.promiscuous-off",
        initial.is_some_and(|snapshot| {
            (snapshot.frame_filter & (FRAME_FILTER_PROMISCUOUS | FRAME_FILTER_PASS_ALL_MULTICAST))
                == 0
        }),
        "promiscuous-enabled",
    );

    let started = configured && start_mac();
    if !started {
        fail_traffic_tests(&mut reporter, "emac-start");
        let _ = reporter.finish();
        terminal_idle();
    }

    let device = request_traffic(
        &mut reporter,
        "inject_unicast",
        1,
        ACTION_INJECT_UNICAST,
        MAC_A,
        ACCEPT_TIMEOUT_MS,
        true,
        "mac.device-unicast.seen",
    );
    record_gate(
        &mut reporter,
        "mac.device-unicast",
        device.complete(),
        "unicast-missing",
    );

    let broadcast = request_traffic(
        &mut reporter,
        "inject_broadcast",
        2,
        ACTION_INJECT_BROADCAST,
        BROADCAST,
        ACCEPT_TIMEOUT_MS,
        true,
        "mac.broadcast.seen",
    );
    record_gate(
        &mut reporter,
        "mac.broadcast",
        broadcast.complete(),
        "broadcast-missing",
    );

    let multicast = request_traffic(
        &mut reporter,
        "inject_multicast",
        3,
        ACTION_INJECT_MULTICAST,
        MULTICAST,
        ACCEPT_TIMEOUT_MS,
        true,
        "mac.multicast.seen",
    );
    record_gate(
        &mut reporter,
        "mac.multicast",
        multicast.complete(),
        "multicast-missing",
    );

    let alien = request_traffic(
        &mut reporter,
        "inject_alien",
        4,
        ACTION_INJECT_ALIEN,
        ALIEN,
        REJECT_WINDOW_MS,
        false,
        "mac.alien.seen",
    );
    record_gate(
        &mut reporter,
        "mac.alien-rejected",
        alien.rejected(),
        "alien-accepted",
    );

    let changed = set_mac_b();
    let second = MacSnapshot::capture();
    reporter.observation("mac.address-b", mac_to_u64(second.address));
    reporter.observation("mac.address0-high-b", second.address_high);
    reporter.observation("mac.address0-low-b", second.address_low);
    reporter.observation("mac.frame-filter-b", second.frame_filter);
    reporter.observation(
        "mac.additional-filter-count-b",
        second.enabled_additional_filters,
    );
    reporter.observation("mac.address1-high-b", second.additional_address_high[0]);
    reporter.observation("mac.address1-low-b", second.additional_address_low[0]);

    let old = request_traffic(
        &mut reporter,
        "inject_old_mac",
        5,
        ACTION_INJECT_OLD_MAC,
        MAC_A,
        REJECT_WINDOW_MS,
        false,
        "mac.old.seen",
    );
    let new = request_traffic(
        &mut reporter,
        "inject_new_mac",
        6,
        ACTION_INJECT_NEW_MAC,
        MAC_B,
        ACCEPT_TIMEOUT_MS,
        true,
        "mac.new.seen",
    );
    record_gate(
        &mut reporter,
        "mac.runtime-change",
        changed
            && second.primary_address_matches(MAC_B)
            && (second.frame_filter & (FRAME_FILTER_PROMISCUOUS | FRAME_FILTER_PASS_ALL_MULTICAST))
                == 0
            && second.enabled_additional_filters == 1
            && second.additional_destination_filter_matches(1, MULTICAST)
            && old.rejected()
            && new.complete(),
        "runtime-filter-failed",
    );

    let _ = critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs)
            .as_mut()
            .and_then(|emac| emac.stop().ok())
    });
    let _ = reporter.finish();
    terminal_idle()
}

fn configure_mac() -> bool {
    critical_section::with(|cs| {
        let mut slot = EMAC.borrow_ref_mut(cs);
        slot.replace(Emac::new());
        let Some(emac) = slot.as_mut() else {
            return false;
        };
        if emac
            .init(Wt32Eth01::emac_config_with_mac(MAC_A), &mut Delay::new())
            .is_err()
        {
            return false;
        }
        emac.set_promiscuous(false);
        emac.set_pass_all_multicast(false);
        emac.set_broadcast_enabled(true);
        emac.clear_mac_filters();
        if emac.add_mac_filter(&MULTICAST).is_err() || emac.mac_filter_count() != 1 {
            return false;
        }
        emac.bind_interrupt(MAC_FILTER_IRQ);
        true
    })
}

fn start_mac() -> bool {
    critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs)
            .as_mut()
            .is_some_and(|emac| emac.state() == State::Initialized && emac.start().is_ok())
    })
}

fn set_mac_b() -> bool {
    critical_section::with(|cs| {
        let mut slot = EMAC.borrow_ref_mut(cs);
        let Some(emac) = slot.as_mut() else {
            return false;
        };
        emac.set_mac_address(&MAC_B);
        *emac.mac_address() == MAC_B
    })
}

#[derive(Clone, Copy, Default)]
struct TrafficObservation {
    sequences: u32,
    duplicates: u32,
    out_of_range: u32,
    receive_errors: u32,
    ri_interrupts: u32,
}

impl TrafficObservation {
    const fn complete(self) -> bool {
        self.sequences == EXPECTED_SEQUENCE_BITS
            && self.duplicates == 0
            && self.out_of_range == 0
            && self.receive_errors == 0
            && self.ri_interrupts > 0
    }

    const fn rejected(self) -> bool {
        self.sequences == 0 && self.out_of_range == 0 && self.receive_errors == 0
    }
}

fn request_traffic(
    reporter: &mut Reporter<'_>,
    action_name: &'static str,
    step: u32,
    action_code: u8,
    destination: [u8; 6],
    timeout_ms: u32,
    stop_when_complete: bool,
    observation_name: &'static str,
) -> TrafficObservation {
    let before = INTERRUPTS.snapshot();
    reporter.ready(action_name);
    let mut observation = collect_traffic(
        step,
        action_code,
        destination,
        timeout_ms,
        stop_when_complete,
    );
    observation.ri_interrupts = INTERRUPTS.snapshot().since(before).ri;
    reporter.observation(observation_name, observation.sequences.count_ones());
    reporter.observation("mac.duplicates", observation.duplicates);
    reporter.observation("mac.out-of-range", observation.out_of_range);
    reporter.observation("mac.receive-errors", observation.receive_errors);
    reporter.observation("mac.ri-interrupts", observation.ri_interrupts);
    observation
}

fn collect_traffic(
    step: u32,
    action: u8,
    destination: [u8; 6],
    timeout_ms: u32,
    stop_when_complete: bool,
) -> TrafficObservation {
    let delay = Delay::new();
    let mut observation = TrafficObservation::default();
    let mut frame = [0u8; 1600];

    for _ in 0..timeout_ms {
        loop {
            match poll_frame(&mut frame) {
                FramePoll::Empty => break,
                FramePoll::Error => observation.receive_errors += 1,
                FramePoll::Frame(length) => {
                    let identity = StimulusIdentity {
                        destination,
                        suite: SUITE_MAC_FILTER,
                        action,
                        run_id: BUILD_RUN_ID,
                        step,
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
        if stop_when_complete && observation.sequences == EXPECTED_SEQUENCE_BITS {
            break;
        }
        delay.delay_millis(1);
    }
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

fn fail_traffic_tests(reporter: &mut Reporter<'_>, reason: &str) {
    for id in [
        "mac.device-unicast",
        "mac.broadcast",
        "mac.multicast",
        "mac.alien-rejected",
        "mac.runtime-change",
    ] {
        reporter.record_with_reason(id, TestResult::Fail, true, reason);
    }
}
