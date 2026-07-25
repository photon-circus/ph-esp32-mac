//! Dedicated MDIO release validation firmware.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::delay::Delay;
use esp_hal::main;
use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01;
use ph_esp32_mac::hal::{MdioBus, MdioController};
use ph_esp32_mac::{Duplex, Emac, Lan8720a, LinkStatus, Speed};
use ph_esp32_mac_qa_runner::snapshots::MdioSnapshot;
use ph_esp32_mac_qa_runner::{
    BUILD_GIT_COMMIT, BUILD_RUN_ID, EMAC, Reporter, RunMode, TestResult, enable_wt32_oscillator,
    reset_reason_token, terminal_idle,
};

esp_app_desc!();

const BMCR: u8 = 0;
const BMSR: u8 = 1;
const PHYID1: u8 = 2;
const PHYID2: u8 = 3;
const ANAR: u8 = 4;
const ANAR_PAUSE: u16 = 1 << 10;
const BMSR_LINK_STATUS: u16 = 1 << 2;
const SAMPLE_COUNT: u32 = 100;
const LINK_WAIT_MS: u32 = 15_000;
const DUT_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x01, 0x02];

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _clock_enable = enable_wt32_oscillator(peripherals.GPIO16);
    let mut reporter = Reporter::new(
        BUILD_RUN_ID,
        "mdio",
        BUILD_GIT_COMMIT,
        RunMode::Release,
        reset_reason_token(),
    );
    reporter.start();

    if !initialize_emac() {
        fail_all(&mut reporter, "emac-init");
        let _ = reporter.finish();
        terminal_idle();
    }

    let mut mdio = MdioController::new(Delay::new());
    let mut completed = 0u32;
    let mut valid_reads = true;
    let mut valid_ids = true;
    let mut double_reads = 0u32;
    let mut latch_differences = 0u32;
    let mut last_bmcr = 0u16;
    let mut last_bmsr = 0u16;
    let mut last_id1 = 0u16;
    let mut last_id2 = 0u16;

    for _ in 0..SAMPLE_COUNT {
        let Ok([bmcr, bmsr_first, bmsr_second, id1, id2]) = sample(&mut mdio) else {
            valid_reads = false;
            break;
        };

        completed += 1;
        double_reads += 1;
        latch_differences += u32::from(bmsr_first != bmsr_second);
        valid_reads &= [bmcr, bmsr_first, bmsr_second, id1, id2]
            .into_iter()
            .all(valid_phy_value);
        valid_ids &= id1 == 0x0007 && (id2 & 0xfff0) == 0xc0f0;
        last_bmcr = bmcr;
        last_bmsr = bmsr_second;
        last_id1 = id1;
        last_id2 = id2;
    }

    reporter.observation("mdio.samples", completed);
    reporter.observation("mdio.bmcr", last_bmcr);
    reporter.observation("mdio.bmsr", last_bmsr);
    reporter.observation("mdio.phyid1", last_id1);
    reporter.observation("mdio.phyid2", last_id2);
    reporter.observation("mdio.bmsr-double-reads", double_reads);
    reporter.observation("mdio.bmsr-latch-differences", latch_differences);
    let snapshot = MdioSnapshot::capture();
    reporter.observation("mdio.gpio18-iomux", snapshot.gpio18_iomux);
    reporter.observation("mdio.gpio18-output-select", snapshot.gpio18_output_select);
    reporter.observation("mdio.mdi-input-select", snapshot.mdi_input_select);

    record_gate(
        &mut reporter,
        "mdio.read-repeat",
        completed == SAMPLE_COUNT && valid_reads,
        "invalid-or-timeout",
    );
    record_gate(
        &mut reporter,
        "mdio.phy-id",
        completed == SAMPLE_COUNT && valid_ids,
        "unexpected-phy",
    );
    record_gate(
        &mut reporter,
        "mdio.bmsr-latch",
        double_reads == SAMPLE_COUNT && valid_reads,
        "double-read-failed",
    );

    let anar_ok = verify_anar_round_trip(&mut mdio, &reporter);
    record_gate(
        &mut reporter,
        "mdio.anar-roundtrip",
        anar_ok,
        "anar-restore-failed",
    );

    let phy = Lan8720a::new(Wt32Eth01::PHY_ADDR);
    let initial_link = wait_for_link(&mut mdio, &phy, true);
    let initial_up = if let LinkWait::Up(status) = initial_link {
        reporter.observation(
            "mdio.initial-speed-mbps",
            match status.speed {
                Speed::Mbps10 => 10,
                Speed::Mbps100 => 100,
            },
        );
        reporter.observation(
            "mdio.initial-duplex",
            match status.duplex {
                Duplex::Half => 0,
                Duplex::Full => 1,
            },
        );
        true
    } else {
        false
    };
    reporter.observation("mdio.initial-link-up", u8::from(initial_up));

    reporter.ready("link_down");
    let down_seen = matches!(wait_for_link(&mut mdio, &phy, false), LinkWait::Down);
    reporter.observation("mdio.link-down-seen", u8::from(down_seen));

    reporter.ready("link_up");
    let link = wait_for_link(&mut mdio, &phy, true);
    let link_valid = if let LinkWait::Up(status) = link {
        reporter.observation(
            "mdio.link-speed-mbps",
            match status.speed {
                Speed::Mbps10 => 10,
                Speed::Mbps100 => 100,
            },
        );
        reporter.observation(
            "mdio.link-duplex",
            match status.duplex {
                Duplex::Half => 0,
                Duplex::Full => 1,
            },
        );
        true
    } else {
        false
    };
    record_gate(
        &mut reporter,
        "mdio.link-transition",
        initial_up && down_seen && link_valid,
        "link-transition-failed",
    );

    let _ = reporter.finish();
    terminal_idle()
}

fn initialize_emac() -> bool {
    critical_section::with(|cs| {
        let mut slot = EMAC.borrow_ref_mut(cs);
        slot.replace(Emac::new());
        let Some(emac) = slot.as_mut() else {
            return false;
        };
        emac.init(Wt32Eth01::emac_config_with_mac(DUT_MAC), &mut Delay::new())
            .is_ok()
    })
}

fn sample(mdio: &mut MdioController<Delay>) -> ph_esp32_mac::Result<[u16; 5]> {
    let bmcr = mdio.read(Wt32Eth01::PHY_ADDR, BMCR)?;
    let bmsr_first = mdio.read(Wt32Eth01::PHY_ADDR, BMSR)?;
    let bmsr_second = mdio.read(Wt32Eth01::PHY_ADDR, BMSR)?;
    let id1 = mdio.read(Wt32Eth01::PHY_ADDR, PHYID1)?;
    let id2 = mdio.read(Wt32Eth01::PHY_ADDR, PHYID2)?;
    Ok([bmcr, bmsr_first, bmsr_second, id1, id2])
}

const fn valid_phy_value(value: u16) -> bool {
    value != 0 && value != u16::MAX
}

fn verify_anar_round_trip(mdio: &mut MdioController<Delay>, reporter: &Reporter<'_>) -> bool {
    let Ok(original) = mdio.read(Wt32Eth01::PHY_ADDR, ANAR) else {
        return false;
    };
    if !valid_phy_value(original) {
        return false;
    }

    let toggled = original ^ ANAR_PAUSE;
    let toggle_written = mdio.write(Wt32Eth01::PHY_ADDR, ANAR, toggled).is_ok();
    let toggle_verified = toggle_written && mdio.read(Wt32Eth01::PHY_ADDR, ANAR) == Ok(toggled);

    // Always attempt restoration once the original value has been captured.
    let restore_written = mdio.write(Wt32Eth01::PHY_ADDR, ANAR, original).is_ok();
    let restored = mdio.read(Wt32Eth01::PHY_ADDR, ANAR);
    reporter.observation("mdio.anar-original", original);
    reporter.observation("mdio.anar-toggled", toggled);
    if let Ok(value) = restored {
        reporter.observation("mdio.anar-restored", value);
    }

    toggle_verified && restore_written && restored == Ok(original)
}

enum LinkWait {
    Down,
    Up(LinkStatus),
    TimeoutOrError,
}

fn wait_for_link(mdio: &mut MdioController<Delay>, phy: &Lan8720a, expected_up: bool) -> LinkWait {
    let delay = Delay::new();
    for _ in 0..LINK_WAIT_MS {
        let Ok(latched) = mdio.read(Wt32Eth01::PHY_ADDR, BMSR) else {
            return LinkWait::TimeoutOrError;
        };
        let Ok(current) = mdio.read(Wt32Eth01::PHY_ADDR, BMSR) else {
            return LinkWait::TimeoutOrError;
        };
        if !valid_phy_value(latched) || !valid_phy_value(current) {
            return LinkWait::TimeoutOrError;
        }
        let link_up = (current & BMSR_LINK_STATUS) != 0;

        if !expected_up && !link_up {
            return LinkWait::Down;
        }
        if expected_up && link_up {
            match phy.read_speed_indication(mdio) {
                Ok(Some(status)) => return LinkWait::Up(status),
                Ok(None) => {}
                Err(_) => return LinkWait::TimeoutOrError,
            }
        }
        delay.delay_millis(1);
    }
    LinkWait::TimeoutOrError
}

fn record_gate(reporter: &mut Reporter<'_>, id: &str, passed: bool, reason: &str) {
    if passed {
        reporter.record(id, TestResult::Pass, true);
    } else {
        reporter.record_with_reason(id, TestResult::Fail, true, reason);
    }
}

fn fail_all(reporter: &mut Reporter<'_>, reason: &str) {
    for id in [
        "mdio.read-repeat",
        "mdio.phy-id",
        "mdio.bmsr-latch",
        "mdio.anar-roundtrip",
        "mdio.link-transition",
    ] {
        reporter.record_with_reason(id, TestResult::Fail, true, reason);
    }
}
