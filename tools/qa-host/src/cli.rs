//! Command-line surface exposed through `cargo xtask qa`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::{
    orchestrator::{CargoFirmwareBuilder, FirmwareBuilder, RepositoryState},
    suite::Suite,
};

/// Host QA command line.
#[derive(Debug, Parser)]
#[command(name = "qa-host", version, about)]
struct Cli {
    #[command(subcommand)]
    command: QaCommand,
}

/// Supported QA workflows.
#[derive(Debug, Subcommand)]
enum QaCommand {
    /// Build one locked ESP32 firmware suite.
    Build {
        /// Dedicated firmware suite.
        #[arg(long, value_enum)]
        suite: Suite,
        /// Build an unoptimized developer image.
        #[arg(long)]
        debug: bool,
    },
    /// Build, flash, stimulate, and validate one suite.
    Run {
        /// Dedicated firmware suite.
        #[arg(long, value_enum)]
        suite: Suite,
        /// Machine-local lab definition.
        #[arg(long)]
        lab: PathBuf,
        /// Permit dirty-tree sandbox evidence.
        #[arg(long)]
        allow_dirty: bool,
    },
    /// Run the reset suite across cold and software-reset cycles.
    Matrix {
        /// Machine-local lab definition.
        #[arg(long)]
        lab: PathBuf,
        /// Number of relay-controlled cold boots.
        #[arg(long, default_value_t = 20)]
        cold: u32,
        /// Number of firmware software resets.
        #[arg(long, default_value_t = 20)]
        warm: u32,
        /// Permit dirty-tree sandbox evidence.
        #[arg(long)]
        allow_dirty: bool,
    },
}

/// Parses process arguments and executes the selected workflow.
pub fn run_cli() -> Result<()> {
    run(Cli::parse())
}

fn run(cli: Cli) -> Result<()> {
    let repo = repository_root()?;
    match cli.command {
        QaCommand::Build { suite, debug } => {
            let state = RepositoryState::discover(&repo)?;
            let mut builder = CargoFirmwareBuilder::new(&repo, state.commit);
            let firmware = builder.build(suite, !debug)?;
            println!(
                "{} (run={}, {} bytes of build log)",
                firmware.executable.display(),
                firmware.run_id,
                firmware.build_log.len()
            );
            Ok(())
        }
        QaCommand::Run {
            suite,
            lab,
            allow_dirty,
        } => run_hardware_command(&repo, suite, &lab, allow_dirty, None),
        QaCommand::Matrix {
            lab,
            cold,
            warm,
            allow_dirty,
        } => {
            if cold == 0 || warm == 0 {
                bail!("matrix requires at least one cold and one warm iteration");
            }
            run_hardware_command(&repo, Suite::Reset, &lab, allow_dirty, Some((cold, warm)))
        }
    }
}

fn repository_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("qa-host manifest is not under <repo>/tools/qa-host")
}

#[cfg(all(windows, feature = "windows-hardware"))]
fn run_hardware_command(
    repo: &Path,
    suite: Suite,
    lab_path: &Path,
    allow_dirty: bool,
    matrix: Option<(u32, u32)>,
) -> Result<()> {
    hardware::run(repo, suite, lab_path, allow_dirty, matrix)
}

#[cfg(not(all(windows, feature = "windows-hardware")))]
fn run_hardware_command(
    _repo: &Path,
    _suite: Suite,
    _lab_path: &Path,
    _allow_dirty: bool,
    _matrix: Option<(u32, u32)>,
) -> Result<()> {
    bail!(
        "hardware execution requires Windows and the `windows-hardware` feature; \
         default builds intentionally require no Npcap SDK"
    )
}

#[cfg(all(windows, feature = "windows-hardware"))]
mod hardware {
    use std::{path::Path, thread, time::Duration};

    use anyhow::{Context, Result, bail};

    use crate::{
        adapters::{
            CommandActionRunner, EspflashFlasher, espflash_version,
            windows::{NpcapPacket, WindowsSerial},
        },
        config::LabConfig,
        evidence::{
            ActionRecord, EvidenceBundle, FailureRecord, FinalizeEvidence, NormalizedResult,
            collect_lock_hashes,
        },
        orchestrator::{
            ActionRunner, CargoFirmwareBuilder, FirmwareBootMode, FirmwareBuilder, Flasher,
            PacketIo, PacketStats, RepositoryState, SerialIo, collect_tool_versions,
            destination_for_action, drive_session, qa_action_code,
        },
        session::SessionReport,
        suite::Suite,
    };

    pub(super) fn run(
        repo: &Path,
        suite: Suite,
        lab_path: &Path,
        allow_dirty: bool,
        matrix: Option<(u32, u32)>,
    ) -> Result<()> {
        let lab = LabConfig::load(lab_path)?;
        let before = RepositoryState::discover(repo)?;
        if before.dirty && !allow_dirty {
            bail!(
                "release-grade QA requires a clean worktree; pass --allow-dirty for sandbox evidence"
            );
        }

        let mut evidence = EvidenceBundle::create(
            repo.join("target/qa-evidence"),
            suite,
            &before.commit,
            before.dirty,
            &lab.board_id,
        )?;
        let mut sessions = Vec::new();
        let mut failures = Vec::new();
        let mut actions = Vec::new();
        let mut tools = collect_tool_versions();
        tools.push(espflash_version(&lab));
        tools.push(NpcapPacket::version());

        let execution = execute(
            repo,
            suite,
            &lab,
            matrix,
            &before,
            &mut evidence,
            &mut sessions,
            &mut actions,
        );
        if let Err(error) = execution {
            failures.push(FailureRecord {
                category: "harness".to_owned(),
                message: format!("{error:#}"),
            });
        }

        let after = RepositoryState::discover(repo)?;
        if after.commit != before.commit || after.dirty != before.dirty {
            failures.push(FailureRecord {
                category: "source_changed".to_owned(),
                message: "repository state changed during QA execution".to_owned(),
            });
        }
        let lockfiles = collect_lock_hashes(repo).unwrap_or_else(|error| {
            failures.push(FailureRecord {
                category: "lock_hash".to_owned(),
                message: error.to_string(),
            });
            Vec::new()
        });
        let result = NormalizedResult::new(suite, sessions, failures);
        let passed = result.passed();
        let path = evidence.finalize(FinalizeEvidence {
            result,
            actions,
            lockfiles,
            tools,
        })?;
        println!("QA evidence: {}", path.display());
        if passed {
            Ok(())
        } else {
            bail!(
                "QA validation failed; evidence retained at {}",
                path.display()
            )
        }
    }

    fn execute(
        repo: &Path,
        suite: Suite,
        lab: &LabConfig,
        matrix: Option<(u32, u32)>,
        state: &RepositoryState,
        evidence: &mut EvidenceBundle,
        sessions: &mut Vec<SessionReport>,
        actions: &mut Vec<ActionRecord>,
    ) -> Result<()> {
        let mut builder = CargoFirmwareBuilder::new(repo, &state.commit);
        let firmware = builder.build(suite, true)?;
        let normal_run_id =
            u64::from_str_radix(&firmware.run_id, 16).context("parse compiled PHQA run ID")?;
        evidence.write_build_log(&firmware.build_log)?;
        if matrix.is_some() {
            evidence.copy_firmware_named(&firmware.executable, "reset-normal")?;
        } else {
            evidence.copy_firmware(&firmware.executable)?;
        }

        let mut packet = NpcapPacket::open(&lab.packet)?;
        let filter = format!(
            "(ether proto 0x88b5) or arp or (udp port 67 or 68) or (ether host {})",
            lab.packet.dut_mac
        );
        packet.start_capture(&evidence.packet_capture_path(), &filter)?;
        let mut flasher = EspflashFlasher;
        flasher.flash(&firmware.executable, lab)?;
        let mut action_runner = CommandActionRunner;

        let session_result = match matrix {
            None => {
                let mut serial = deliberate_power_boot(lab, &mut action_runner, actions)?;
                drive_session(
                    &mut serial,
                    &mut packet,
                    &mut action_runner,
                    lab,
                    suite,
                    &state.commit,
                    normal_run_id,
                    evidence,
                    actions,
                )
                .map(|report| sessions.push(report))
                .map_err(anyhow::Error::new)
            }
            Some((cold, warm)) => run_matrix(
                lab,
                &state.commit,
                cold,
                warm,
                normal_run_id,
                &mut builder,
                &mut flasher,
                evidence,
                &mut packet,
                &mut action_runner,
                sessions,
                actions,
            ),
        };
        let capture_result = packet.finish_capture();
        let captured = capture_result?;
        actions.push(ActionRecord {
            action: "capture_all".to_owned(),
            executable: "npcap".to_owned(),
            exit_code: Some(0),
            duration_ms: 0,
            success: true,
            suite: None,
            run_id: None,
            step: None,
            count: Some(captured.total_frames),
        });
        actions.push(ActionRecord {
            action: "capture_phqa".to_owned(),
            executable: "npcap".to_owned(),
            exit_code: Some(0),
            duration_ms: 0,
            success: true,
            suite: None,
            run_id: None,
            step: None,
            count: Some(captured.qa_frames),
        });
        actions.push(ActionRecord {
            action: "capture_arp_replies".to_owned(),
            executable: "npcap".to_owned(),
            exit_code: Some(0),
            duration_ms: 0,
            success: true,
            suite: None,
            run_id: None,
            step: None,
            count: Some(captured.arp_replies),
        });
        actions.push(ActionRecord {
            action: "capture_dhcp".to_owned(),
            executable: "npcap".to_owned(),
            exit_code: Some(0),
            duration_ms: 0,
            success: true,
            suite: None,
            run_id: None,
            step: None,
            count: Some(captured.dhcp_frames),
        });
        if captured.total_frames == 0 {
            bail!("Npcap captured no traffic");
        }
        validate_packet_evidence(&captured, actions, sessions, lab.packet.parsed_dut_mac()?)?;
        session_result?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_matrix(
        lab: &LabConfig,
        commit: &str,
        cold: u32,
        warm: u32,
        normal_run_id: u64,
        builder: &mut CargoFirmwareBuilder,
        flasher: &mut dyn Flasher,
        evidence: &mut EvidenceBundle,
        packet: &mut dyn PacketIo,
        action_runner: &mut dyn ActionRunner,
        sessions: &mut Vec<SessionReport>,
        actions: &mut Vec<ActionRecord>,
    ) -> Result<()> {
        for _ in 0..cold {
            let mut serial = deliberate_power_boot(lab, action_runner, actions)?;
            let report = drive_session(
                &mut serial,
                packet,
                action_runner,
                lab,
                Suite::Reset,
                commit,
                normal_run_id,
                evidence,
                actions,
            )?;
            require_reset_reason(&report, "power_on")?;
            sessions.push(report);
        }

        for iteration in 1..=warm {
            let firmware =
                builder.build_for_boot(Suite::Reset, true, FirmwareBootMode::Warm { iteration })?;
            evidence.append_build_log(&firmware.build_log)?;
            evidence
                .copy_firmware_named(&firmware.executable, &format!("reset-warm-{iteration:02}"))?;
            flasher.flash(&firmware.executable, lab)?;
            let expected_run_id =
                u64::from_str_radix(&firmware.run_id, 16).context("parse warm PHQA run ID")?;
            let mut serial = deliberate_power_boot(lab, action_runner, actions)?;
            let report = drive_session(
                &mut serial,
                packet,
                action_runner,
                lab,
                Suite::Reset,
                commit,
                expected_run_id,
                evidence,
                actions,
            )?;
            require_reset_reason(&report, "core_software")?;
            sessions.push(report);
        }
        Ok(())
    }

    fn deliberate_power_boot(
        lab: &LabConfig,
        action_runner: &mut dyn ActionRunner,
        actions: &mut Vec<ActionRecord>,
    ) -> Result<WindowsSerial> {
        let timeout = Duration::from_millis(lab.timeouts.action_ms);
        actions.push(action_runner.run(
            "power_off",
            &lab.actions.power_off,
            lab.base_dir(),
            timeout,
        )?);
        thread::sleep(Duration::from_millis(lab.settle.power_ms));
        let mut serial = WindowsSerial::open(&lab.serial).ok();
        if let Some(serial) = &mut serial {
            serial.discard_input().context("purge serial before boot")?;
        }
        actions.push(action_runner.run(
            "power_on",
            &lab.actions.power_on,
            lab.base_dir(),
            timeout,
        )?);
        match serial {
            Some(serial) => Ok(serial),
            None => WindowsSerial::open_bounded(
                &lab.serial,
                Duration::from_millis(lab.serial.boot_timeout_ms),
            ),
        }
    }

    fn require_reset_reason(report: &SessionReport, expected: &str) -> Result<()> {
        if report.reset_reason != expected {
            bail!(
                "reset reason mismatch: expected `{expected}`, received `{}`",
                report.reset_reason
            );
        }
        Ok(())
    }

    fn validate_packet_evidence(
        stats: &PacketStats,
        actions: &[ActionRecord],
        sessions: &[SessionReport],
        dut_mac: [u8; 6],
    ) -> Result<()> {
        for action in actions.iter().filter(|action| {
            matches!(
                action.executable.as_str(),
                "packet-adapter" | "udp-packet-adapter"
            )
        }) {
            let suite = action
                .suite
                .context("packet action omitted suite identity")?;
            let run_id = u64::from_str_radix(
                action
                    .run_id
                    .as_deref()
                    .context("packet action omitted run identity")?,
                16,
            )
            .context("parse packet action run ID")?;
            let step = action.step.context("packet action omitted READY step")?;
            let requested = action.count.context("packet action omitted frame count")?;
            if action.action == "flood_rx" && !(1_900..=2_250).contains(&action.duration_ms) {
                bail!(
                    "flood pacing duration {} ms fell outside 1900..=2250 ms",
                    action.duration_ms
                );
            }
            let code = qa_action_code(&action.action)
                .with_context(|| format!("unknown packet action `{}`", action.action))?;
            let destination = destination_for_action(&action.action, dut_mac)?;
            let udp_echo = action.executable == "udp-packet-adapter";
            let captured = if udp_echo {
                if stats.arp_replies == 0 {
                    bail!("captured no fixed-IP DUT ARP reply before UDP echo validation");
                }
                stats.unique_udp_echoes(suite, code, run_id, step)
            } else {
                stats.unique_sequences(suite, code, run_id, step)
            };
            if !stats.has_exact_sequences(
                udp_echo,
                suite,
                code,
                run_id,
                step,
                destination,
                requested,
            ) {
                bail!(
                    "captured {captured}/{requested} exact sequences for {} run={run_id:x} step={step}",
                    action.action
                );
            }
        }

        for report in sessions
            .iter()
            .filter(|report| report.suite == Suite::RxEmbassy)
        {
            let mut ready_events = report.ready.iter().filter(|ready| ready.action == "dhcp");
            let ready = ready_events
                .next()
                .context("rx-embassy session omitted its DHCP READY record")?;
            if ready_events.next().is_some() {
                bail!("rx-embassy session emitted duplicate DHCP READY records");
            }
            let mut transactions = actions.iter().filter(|action| {
                action.executable == "npcap-dhcp-transaction"
                    && action.suite == Some(report.suite)
                    && action.run_id.as_deref() == Some(report.run_id.as_str())
                    && action.step == Some(ready.step)
            });
            let transaction = transactions
                .next()
                .context("DHCP READY had no scoped Request/ACK capture evidence")?;
            if transactions.next().is_some() {
                bail!("DHCP READY had duplicate scoped transaction evidence");
            }

            let mut addresses = report
                .observations
                .iter()
                .filter(|observation| observation.name == "rx-embassy.dhcp-address");
            let firmware_address = addresses
                .next()
                .context("firmware omitted rx-embassy.dhcp-address")?
                .value
                .parse::<u64>()
                .context("parse firmware DHCP address observation")?;
            if addresses.next().is_some() {
                bail!("firmware emitted duplicate rx-embassy.dhcp-address observations");
            }
            if firmware_address == 0 || transaction.count != Some(firmware_address) {
                bail!(
                    "captured DHCP ACK address {:?} did not match firmware address {firmware_address}",
                    transaction.count
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, QaCommand};
    use crate::suite::Suite;

    #[test]
    fn parses_documented_build_surface() {
        let cli = Cli::try_parse_from(["qa-host", "build", "--suite", "rx-async"]).unwrap();
        assert!(matches!(
            cli.command,
            QaCommand::Build {
                suite: Suite::RxAsync,
                debug: false
            }
        ));
    }

    #[test]
    fn parses_documented_matrix_surface() {
        let cli = Cli::try_parse_from([
            "qa-host",
            "matrix",
            "--lab",
            "qa/lab.toml",
            "--cold",
            "20",
            "--warm",
            "20",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            QaCommand::Matrix {
                cold: 20,
                warm: 20,
                ..
            }
        ));
    }
}
