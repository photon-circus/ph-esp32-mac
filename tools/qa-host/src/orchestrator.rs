//! Hardware-independent orchestration and adapter contracts.

use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    config::{CommandArgv, LabConfig},
    evidence::{ActionRecord, EvidenceBundle, ToolVersion},
    protocol::{LineDecoder, Record, parse_line},
    session::{ReadyEvent, SessionEvent, SessionReport, SessionValidator},
    suite::Suite,
};

/// Result of building one firmware suite.
#[derive(Clone, Debug)]
pub struct BuiltFirmware {
    /// Path to the exact ELF produced by Cargo.
    pub executable: PathBuf,
    /// Lowercase run token compiled into firmware.
    pub run_id: String,
    /// Combined Cargo stdout and stderr retained as evidence.
    pub build_log: Vec<u8>,
}

/// Boot phase compiled into a QA firmware artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirmwareBootMode {
    /// Run the suite directly after a normal or relay-controlled boot.
    Normal,
    /// Perform the iteration-tagged RTC-marker software-reset handshake.
    Warm {
        /// One-based warm matrix iteration.
        iteration: u32,
    },
}

/// Source-control state attached to evidence.
#[derive(Clone, Debug)]
pub struct RepositoryState {
    /// Full checked-out commit.
    pub commit: String,
    /// Whether tracked or untracked source files differ from the commit.
    pub dirty: bool,
}

impl RepositoryState {
    /// Reads the current Git commit and worktree status.
    pub fn discover(repo: &Path) -> Result<Self> {
        let commit = command_text(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["rev-parse", "HEAD"]),
        )
        .context("read Git commit")?;
        let status = command_text(Command::new("git").arg("-C").arg(repo).args([
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
        ]))
        .context("read Git worktree status")?;
        Ok(Self {
            commit,
            dirty: !status.trim().is_empty(),
        })
    }
}

/// Builds one QA firmware target.
pub trait FirmwareBuilder {
    /// Builds `suite` and returns its exact ELF and build transcript.
    fn build(&mut self, suite: Suite, release: bool) -> Result<BuiltFirmware>;
}

/// Flashes one ELF to the configured board.
pub trait Flasher {
    /// Writes and boots `firmware`.
    fn flash(&mut self, firmware: &Path, lab: &LabConfig) -> Result<()>;
}

/// Bounded serial transport used by the protocol reader.
pub trait SerialIo {
    /// Reads at most `buffer.len()` bytes, returning zero on a read timeout.
    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize>;

    /// Writes a complete firmware control message.
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Discards bytes buffered before a deliberately initiated boot.
    fn discard_input(&mut self) -> io::Result<()>;
}

/// Packet capture and raw-frame injection boundary.
pub trait PacketIo {
    /// Starts capture into a classic pcap artifact.
    fn start_capture(&mut self, path: &Path, filter: &str) -> Result<()>;

    /// Injects one complete Ethernet frame.
    fn inject(&mut self, frame: &[u8]) -> Result<()>;

    /// Stops capture and returns independently counted packet totals.
    fn finish_capture(&mut self) -> Result<PacketStats>;
}

/// Packet counters produced by a completed capture.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PacketStats {
    /// All frames accepted by the configured capture filter.
    pub total_frames: u64,
    /// Frames carrying the QA experimental EtherType `0x88b5`.
    pub qa_frames: u64,
    /// Unique sequence counts grouped by exact suite, action, and run ID.
    pub classes: Vec<PacketClassStats>,
    /// Matching UDP echoes returned by the fixed-IP embassy-net scenario.
    pub udp_echoes: Vec<PacketClassStats>,
    /// ARP replies from the fixed DUT address to the fixed host address.
    pub arp_replies: u64,
    /// Captured DHCP client/server IPv4 datagrams.
    pub dhcp_frames: u64,
}

impl PacketStats {
    /// Returns the unique captured sequence count for one stimulus class.
    #[must_use]
    pub fn unique_sequences(&self, suite: Suite, action: u8, run_id: u64, step: u32) -> u64 {
        unique_sequences(&self.classes, suite, action, run_id, step)
    }

    /// Returns captured embassy UDP echo sequences for one exact identity.
    #[must_use]
    pub fn unique_udp_echoes(&self, suite: Suite, action: u8, run_id: u64, step: u32) -> u64 {
        unique_sequences(&self.udp_echoes, suite, action, run_id, step)
    }
}

fn unique_sequences(
    classes: &[PacketClassStats],
    suite: Suite,
    action: u8,
    run_id: u64,
    step: u32,
) -> u64 {
    classes
        .iter()
        .find(|class| {
            class.suite == suite
                && class.action == action
                && class.run_id == run_id
                && class.step == step
        })
        .map_or(0, |class| class.unique_sequences)
}

/// Capture count for one exact PHQA stimulus identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketClassStats {
    /// Encoded firmware suite.
    pub suite: Suite,
    /// Encoded action code.
    pub action: u8,
    /// Exact compiled run identifier.
    pub run_id: u64,
    /// Firmware READY step encoded into each frame.
    pub step: u32,
    /// Number of distinct sequence values captured.
    pub unique_sequences: u64,
}

/// Fixed QA Ethernet frame length.
pub const QA_FRAME_LEN: usize = 64;
/// Experimental QA Ethernet type.
pub const QA_ETHERTYPE: u16 = 0x88b5;
/// Ethernet type byte offset.
pub const QA_ETHERTYPE_OFFSET: usize = 12;
/// `PHQA` magic byte offset.
pub const QA_MAGIC_OFFSET: usize = 14;
/// QA packet protocol version byte offset.
pub const QA_VERSION_OFFSET: usize = 18;
/// Numeric suite-code byte offset.
pub const QA_SUITE_OFFSET: usize = 19;
/// Numeric action-code byte offset.
pub const QA_ACTION_OFFSET: usize = 20;
/// ASCII run-ID length byte offset.
pub const QA_RUN_LEN_OFFSET: usize = 21;
/// Start of the padded lowercase hexadecimal run-ID field.
pub const QA_RUN_OFFSET: usize = 22;
/// Capacity of the ASCII run-ID field.
pub const QA_RUN_CAPACITY: usize = 16;
/// Big-endian READY-step byte offset.
pub const QA_STEP_OFFSET: usize = 38;
/// Big-endian sequence byte offset.
pub const QA_SEQUENCE_OFFSET: usize = 42;

/// Fixed IPv4 address assigned to the Windows packet adapter for UDP QA.
pub const QA_HOST_IPV4: [u8; 4] = [192, 0, 2, 1];
/// Fixed IPv4 address configured by the embassy-net recovery suite.
pub const QA_DUT_IPV4: [u8; 4] = [192, 0, 2, 2];
/// UDP source port used by the host challenge.
pub const QA_HOST_UDP_PORT: u16 = 42_425;
/// UDP echo port used by the firmware.
pub const QA_DUT_UDP_PORT: u16 = 42_424;

/// Identity decoded from a valid PHQA Ethernet frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QaFrameTag {
    /// Encoded suite.
    pub suite: Suite,
    /// Encoded action.
    pub action: u8,
    /// Exact compiled run identifier.
    pub run_id: u64,
    /// Firmware READY step.
    pub step: u32,
    /// Frame sequence within the action.
    pub sequence: u32,
}

/// Returns the stable numeric code for a firmware READY action.
#[must_use]
pub fn qa_action_code(action: &str) -> Option<u8> {
    match action {
        "link_down" => Some(1),
        "link_up" => Some(2),
        "inject_unicast" => Some(3),
        "inject_broadcast" => Some(4),
        "inject_multicast" => Some(5),
        "inject_alien" => Some(6),
        "inject_old_mac" => Some(7),
        "inject_new_mac" => Some(8),
        "fill_rx" => Some(9),
        "flood_rx" => Some(10),
        "send_sentinel" => Some(11),
        "send_second_sentinel" => Some(12),
        "udp_echo" => Some(13),
        "dhcp" => Some(14),
        _ => None,
    }
}

/// Shell-free relay/link action boundary.
pub trait ActionRunner {
    /// Runs one executable-plus-argv command with a hard deadline.
    fn run(
        &mut self,
        action: &str,
        command: &CommandArgv,
        base_dir: &Path,
        timeout: Duration,
    ) -> Result<ActionRecord>;
}

/// Concrete locked firmware builder used by the CLI.
pub struct CargoFirmwareBuilder {
    repo: PathBuf,
    commit: String,
}

impl CargoFirmwareBuilder {
    /// Constructs a builder rooted at the repository checkout.
    #[must_use]
    pub fn new(repo: impl Into<PathBuf>, commit: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            commit: commit.into(),
        }
    }

    /// Builds firmware with an explicit compile-time boot contract.
    pub fn build_for_boot(
        &mut self,
        suite: Suite,
        release: bool,
        boot_mode: FirmwareBootMode,
    ) -> Result<BuiltFirmware> {
        let run_id = generate_run_id();
        let (boot_mode_token, boot_iteration) = match boot_mode {
            FirmwareBootMode::Normal => ("normal", 0),
            FirmwareBootMode::Warm { iteration } => {
                if iteration == 0 {
                    bail!("warm firmware iteration must be one-based");
                }
                ("warm", iteration)
            }
        };
        self.build_inner(suite, release, &run_id, boot_mode_token, boot_iteration)
    }

    fn build_inner(
        &self,
        suite: Suite,
        release: bool,
        run_id: &str,
        boot_mode: &str,
        boot_iteration: u32,
    ) -> Result<BuiltFirmware> {
        let manifest = self.repo.join("apps/qa-runner/Cargo.toml");
        let mut command = Command::new("rustup");
        command
            .current_dir(&self.repo)
            .args(["run", "esp", "cargo", "build", "--locked"])
            .arg("--manifest-path")
            .arg(&manifest)
            .args([
                "--target",
                "xtensa-esp32-none-elf",
                "-Zbuild-std=core",
                "--bin",
                suite.binary_name(),
                "--message-format=json-render-diagnostics",
                "--config",
                "target.xtensa-esp32-none-elf.rustflags=[\"-C\",\"link-arg=-nostartfiles\",\"-C\",\"link-arg=-Wl,-Tlinkall.x\"]",
            ])
            .env("CARGO_TARGET_DIR", self.repo.join("target"))
            .env("PHQA_RUN_ID", run_id)
            .env("PHQA_SUITE", suite.to_string())
            .env("PHQA_GIT_SHA", &self.commit)
            .env("PHQA_MODE", if release { "release" } else { "exploratory" })
            .env("PHQA_BOOT_MODE", boot_mode)
            .env("PHQA_BOOT_ITERATION", boot_iteration.to_string());
        if release {
            command.arg("--release");
        }

        let output = command.output().context("spawn ESP Cargo build")?;
        let mut build_log = output.stdout.clone();
        build_log.extend_from_slice(&output.stderr);
        if !output.status.success() {
            bail!("ESP firmware build failed with {}", output.status);
        }
        let executable = compiler_artifact_path(&output.stdout, suite.binary_name())?
            .ok_or_else(|| anyhow!("Cargo did not report an executable for {}", suite))?;
        Ok(BuiltFirmware {
            executable,
            run_id: run_id.to_owned(),
            build_log,
        })
    }
}

impl FirmwareBuilder for CargoFirmwareBuilder {
    fn build(&mut self, suite: Suite, release: bool) -> Result<BuiltFirmware> {
        self.build_for_boot(suite, release, FirmwareBootMode::Normal)
    }
}

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    target: Option<CargoTarget>,
    executable: Option<PathBuf>,
}

#[derive(Deserialize)]
struct CargoTarget {
    name: String,
}

fn compiler_artifact_path(output: &[u8], bin_name: &str) -> Result<Option<PathBuf>> {
    let mut executable = None;
    for line in output.split(|byte| *byte == b'\n') {
        let Ok(message) = serde_json::from_slice::<CargoMessage>(line) else {
            continue;
        };
        if message.reason == "compiler-artifact"
            && message.target.as_ref().map(|target| target.name.as_str()) == Some(bin_name)
            && message.executable.is_some()
        {
            executable = message.executable;
        }
    }
    Ok(executable)
}

/// Drives one strict session until `RUN_END` or a deadline.
pub fn drive_session(
    serial: &mut dyn SerialIo,
    packet: &mut dyn PacketIo,
    actions: &mut dyn ActionRunner,
    lab: &LabConfig,
    suite: Suite,
    expected_commit: &str,
    expected_run_id: u64,
    evidence: &mut EvidenceBundle,
    action_records: &mut Vec<ActionRecord>,
) -> Result<SessionReport, DriveError> {
    let boot_deadline = Instant::now() + Duration::from_millis(lab.serial.boot_timeout_ms);
    let mut suite_deadline = None;
    let mut decoder = LineDecoder::new();
    let mut validator =
        SessionValidator::new(suite, expected_commit, true).expect_run_id(expected_run_id);
    let mut buffer = [0_u8; 1_024];
    let mut active_run = false;

    loop {
        let deadline = suite_deadline.unwrap_or(boot_deadline);
        if Instant::now() >= deadline {
            return Err(if suite_deadline.is_some() {
                DriveError::Timeout
            } else {
                DriveError::BootTimeout
            });
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let read_timeout = remaining.min(Duration::from_millis(lab.serial.read_timeout_ms));
        let read = match serial.read(&mut buffer, read_timeout) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::TimedOut => 0,
            Err(error) => return Err(DriveError::Serial(error)),
        };
        if read == 0 {
            continue;
        }
        evidence
            .append_serial(&buffer[..read])
            .map_err(|error| DriveError::Evidence(error.to_string()))?;

        for line in decoder.push(&buffer[..read])? {
            if active_run && panic_or_reset_marker(&line) {
                return Err(DriveError::UnexpectedResetText(
                    String::from_utf8_lossy(&line).into_owned(),
                ));
            }
            let Some(record) = parse_line(&line)? else {
                continue;
            };
            if matches!(record, Record::RunStart { .. }) {
                active_run = true;
                suite_deadline =
                    Some(Instant::now() + Duration::from_millis(lab.timeouts.suite_ms));
            }
            match validator.consume(record)? {
                SessionEvent::Continue => {}
                SessionEvent::Ready(ready) => {
                    perform_ready(
                        &ready,
                        packet,
                        actions,
                        lab,
                        suite,
                        expected_run_id,
                        action_records,
                    )
                    .map_err(|error| DriveError::Action(error.to_string()))?;
                }
                SessionEvent::Complete(report) => return Ok(report),
            }
        }
    }
}

fn perform_ready(
    ready: &ReadyEvent,
    packet: &mut dyn PacketIo,
    actions: &mut dyn ActionRunner,
    lab: &LabConfig,
    suite: Suite,
    run_id: u64,
    action_records: &mut Vec<ActionRecord>,
) -> Result<()> {
    let timeout = Duration::from_millis(lab.timeouts.action_ms);
    match ready.action.as_str() {
        "power_off" => action_records.push(actions.run(
            "power_off",
            &lab.actions.power_off,
            lab.base_dir(),
            timeout,
        )?),
        "power_on" => action_records.push(actions.run(
            "power_on",
            &lab.actions.power_on,
            lab.base_dir(),
            timeout,
        )?),
        "link_down" => {
            action_records.push(actions.run(
                "link_down",
                &lab.actions.link_down,
                lab.base_dir(),
                timeout,
            )?);
            thread::sleep(Duration::from_millis(lab.settle.link_ms));
        }
        "link_up" => {
            action_records.push(actions.run(
                "link_up",
                &lab.actions.link_up,
                lab.base_dir(),
                timeout,
            )?);
            thread::sleep(Duration::from_millis(lab.settle.link_ms));
        }
        "udp_echo" => {
            let started = Instant::now();
            packet.inject(&arp_request(lab.packet.parsed_host_mac()?))?;
            thread::sleep(Duration::from_millis(100));
            packet.inject(&udp_echo_challenge(
                lab.packet.parsed_dut_mac()?,
                lab.packet.parsed_host_mac()?,
                run_id,
                ready.step,
            )?)?;
            action_records.push(ActionRecord {
                action: "udp_echo".to_owned(),
                executable: "udp-packet-adapter".to_owned(),
                exit_code: Some(0),
                duration_ms: started.elapsed().as_millis(),
                success: true,
                suite: Some(suite),
                run_id: Some(format!("{run_id:x}")),
                step: Some(ready.step),
                count: Some(1),
            });
        }
        action
            if action.starts_with("inject_")
                || matches!(
                    action,
                    "fill_rx" | "flood_rx" | "send_sentinel" | "send_second_sentinel"
                ) =>
        {
            let count = match action {
                "flood_rx" => 4_096,
                action if action.starts_with("inject_") => 32,
                "fill_rx" => 4,
                _ => 1,
            };
            let host = lab.packet.parsed_host_mac()?;
            let dut = destination_for_action(action, lab.packet.parsed_dut_mac()?)?;
            let started = Instant::now();
            let action_code = qa_action_code(action)
                .ok_or_else(|| anyhow!("READY action `{action}` has no packet wire code"))?;
            let pacing = stimulus_duration(action, count);
            for sequence in 0..count {
                wait_for_stimulus_slot(started, pacing, sequence, count);
                packet.inject(&qa_frame(
                    dut,
                    host,
                    suite,
                    action_code,
                    run_id,
                    ready.step,
                    sequence,
                )?)?;
            }
            if !pacing.is_zero() {
                wait_until(started + pacing);
            }
            action_records.push(ActionRecord {
                action: action.to_owned(),
                executable: "packet-adapter".to_owned(),
                exit_code: Some(0),
                duration_ms: started.elapsed().as_millis(),
                success: true,
                suite: Some(suite),
                run_id: Some(format!("{run_id:x}")),
                step: Some(ready.step),
                count: Some(u64::from(count)),
            });
        }
        "dhcp" => {
            // DHCP is supplied by the isolated lab network rather than by the
            // raw-packet adapter. Retain the request as explicit evidence;
            // firmware fails the required test if no lease arrives.
            action_records.push(ActionRecord {
                action: "dhcp".to_owned(),
                executable: "external-dhcp-fixture".to_owned(),
                exit_code: None,
                duration_ms: 0,
                success: true,
                suite: Some(suite),
                run_id: Some(format!("{run_id:x}")),
                step: Some(ready.step),
                count: None,
            });
        }
        action => bail!("unsupported READY action `{action}`"),
    }
    Ok(())
}

fn stimulus_duration(action: &str, count: u32) -> Duration {
    match action {
        "flood_rx" => Duration::from_secs(2),
        action if action.starts_with("inject_") => {
            Duration::from_millis(u64::from(count.saturating_sub(1)) * 2)
        }
        _ => Duration::ZERO,
    }
}

fn wait_for_stimulus_slot(started: Instant, duration: Duration, sequence: u32, count: u32) {
    if duration.is_zero() || count <= 1 {
        return;
    }
    let numerator = duration.as_nanos() * u128::from(sequence);
    let offset =
        Duration::from_nanos(u64::try_from(numerator / u128::from(count - 1)).unwrap_or(u64::MAX));
    wait_until(started + offset);
}

fn wait_until(deadline: Instant) {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if remaining > Duration::from_millis(1) {
            thread::sleep(remaining - Duration::from_micros(500));
        } else {
            std::hint::spin_loop();
        }
    }
}

fn destination_for_action(action: &str, dut: [u8; 6]) -> Result<[u8; 6]> {
    match action {
        "inject_broadcast" => Ok([0xff; 6]),
        "inject_multicast" => Ok([0x01, 0x00, 0x5e, 0x00, 0x00, 0x01]),
        "inject_old_mac" => Ok(dut),
        "inject_new_mac" => {
            let mut changed = dut;
            changed[5] = changed[5]
                .checked_add(1)
                .ok_or_else(|| anyhow!("cannot derive changed MAC from {dut:02x?}"))?;
            Ok(changed)
        }
        "inject_alien" => {
            let mut alien = dut;
            alien[5] = 0xfe;
            Ok(alien)
        }
        _ => Ok(dut),
    }
}

/// Constructs a deterministic fixed-layout PHQA Ethernet stimulus frame.
pub fn qa_frame(
    destination: [u8; 6],
    source: [u8; 6],
    suite: Suite,
    action: u8,
    run_id: u64,
    step: u32,
    sequence: u32,
) -> Result<Vec<u8>> {
    if action == 0 || action > 14 {
        bail!("invalid PHQA packet action code {action}");
    }
    let run_id = format!("{run_id:x}");
    if run_id.len() > QA_RUN_CAPACITY {
        bail!("PHQA run ID exceeds {QA_RUN_CAPACITY} ASCII hex bytes");
    }
    let mut frame = vec![0_u8; QA_FRAME_LEN];
    frame[..6].copy_from_slice(&destination);
    frame[6..12].copy_from_slice(&source);
    frame[QA_ETHERTYPE_OFFSET..QA_ETHERTYPE_OFFSET + 2]
        .copy_from_slice(&QA_ETHERTYPE.to_be_bytes());
    frame[QA_MAGIC_OFFSET..QA_MAGIC_OFFSET + 4].copy_from_slice(b"PHQA");
    frame[QA_VERSION_OFFSET] = 1;
    frame[QA_SUITE_OFFSET] = suite.wire_code();
    frame[QA_ACTION_OFFSET] = action;
    frame[QA_RUN_LEN_OFFSET] = u8::try_from(run_id.len()).unwrap_or(0);
    frame[QA_RUN_OFFSET..QA_RUN_OFFSET + run_id.len()].copy_from_slice(run_id.as_bytes());
    frame[QA_STEP_OFFSET..QA_STEP_OFFSET + 4].copy_from_slice(&step.to_be_bytes());
    frame[QA_SEQUENCE_OFFSET..QA_SEQUENCE_OFFSET + 4].copy_from_slice(&sequence.to_be_bytes());
    Ok(frame)
}

/// Constructs the fixed-IP UDP challenge used by the embassy-net suite.
pub fn udp_echo_challenge(
    destination: [u8; 6],
    source: [u8; 6],
    run_id: u64,
    step: u32,
) -> Result<Vec<u8>> {
    const ETHERNET_HEADER_LEN: usize = 14;
    const IPV4_HEADER_LEN: usize = 20;
    const UDP_HEADER_LEN: usize = 8;
    const IPV4_PROTOCOL_UDP: u8 = 17;

    let payload = qa_frame(
        destination,
        source,
        Suite::RxEmbassy,
        qa_action_code("udp_echo").unwrap_or(13),
        run_id,
        step,
        0,
    )?;
    let udp_len = UDP_HEADER_LEN + payload.len();
    let ipv4_len = IPV4_HEADER_LEN + udp_len;
    let mut frame = vec![0_u8; ETHERNET_HEADER_LEN + ipv4_len];

    frame[..6].copy_from_slice(&destination);
    frame[6..12].copy_from_slice(&source);
    frame[12..14].copy_from_slice(&0x0800_u16.to_be_bytes());

    let ip = &mut frame[ETHERNET_HEADER_LEN..ETHERNET_HEADER_LEN + IPV4_HEADER_LEN];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(
        &u16::try_from(ipv4_len)
            .map_err(|_| anyhow!("UDP QA packet exceeds IPv4 length"))?
            .to_be_bytes(),
    );
    ip[4..6].copy_from_slice(&(run_id as u16).to_be_bytes());
    ip[6..8].copy_from_slice(&0x4000_u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = IPV4_PROTOCOL_UDP;
    ip[12..16].copy_from_slice(&QA_HOST_IPV4);
    ip[16..20].copy_from_slice(&QA_DUT_IPV4);
    let ip_checksum = internet_checksum(ip);
    ip[10..12].copy_from_slice(&ip_checksum.to_be_bytes());

    let udp_start = ETHERNET_HEADER_LEN + IPV4_HEADER_LEN;
    let udp_end = udp_start + UDP_HEADER_LEN;
    let udp = &mut frame[udp_start..udp_end];
    udp[..2].copy_from_slice(&QA_HOST_UDP_PORT.to_be_bytes());
    udp[2..4].copy_from_slice(&QA_DUT_UDP_PORT.to_be_bytes());
    udp[4..6].copy_from_slice(
        &u16::try_from(udp_len)
            .map_err(|_| anyhow!("UDP QA payload exceeds UDP length"))?
            .to_be_bytes(),
    );
    frame[udp_end..].copy_from_slice(&payload);
    let udp_checksum = udp_checksum(
        QA_HOST_IPV4,
        QA_DUT_IPV4,
        &frame[udp_start..],
        IPV4_PROTOCOL_UDP,
    );
    frame[udp_start + 6..udp_start + 8].copy_from_slice(&udp_checksum.to_be_bytes());
    Ok(frame)
}

/// Constructs an ARP request that primes the embassy-net neighbor cache.
#[must_use]
pub fn arp_request(source: [u8; 6]) -> Vec<u8> {
    let mut frame = vec![0_u8; 60];
    frame[..6].fill(0xff);
    frame[6..12].copy_from_slice(&source);
    frame[12..14].copy_from_slice(&0x0806_u16.to_be_bytes());
    frame[14..16].copy_from_slice(&1_u16.to_be_bytes());
    frame[16..18].copy_from_slice(&0x0800_u16.to_be_bytes());
    frame[18] = 6;
    frame[19] = 4;
    frame[20..22].copy_from_slice(&1_u16.to_be_bytes());
    frame[22..28].copy_from_slice(&source);
    frame[28..32].copy_from_slice(&QA_HOST_IPV4);
    frame[38..42].copy_from_slice(&QA_DUT_IPV4);
    frame
}

/// Returns true for the expected fixed-IP DUT ARP reply.
#[must_use]
pub fn is_arp_reply(frame: &[u8]) -> bool {
    frame.get(12..14) == Some(&[0x08, 0x06])
        && frame.get(14..16) == Some(&[0, 1])
        && frame.get(16..18) == Some(&[0x08, 0])
        && frame.get(18) == Some(&6)
        && frame.get(19) == Some(&4)
        && frame.get(20..22) == Some(&[0, 2])
        && frame.get(28..32) == Some(&QA_DUT_IPV4)
        && frame.get(38..42) == Some(&QA_HOST_IPV4)
}

/// Returns true for a captured IPv4 DHCP client/server datagram.
#[must_use]
pub fn is_dhcp_frame(frame: &[u8]) -> bool {
    const ETHERNET_HEADER_LEN: usize = 14;

    if frame.get(12..14) != Some(&[0x08, 0x00]) {
        return false;
    }
    let Some(version_ihl) = frame.get(ETHERNET_HEADER_LEN).copied() else {
        return false;
    };
    if version_ihl >> 4 != 4 {
        return false;
    }
    let ip_header_len = usize::from(version_ihl & 0x0f) * 4;
    if ip_header_len < 20
        || frame.get(ETHERNET_HEADER_LEN + 9) != Some(&17)
        || frame.len() < ETHERNET_HEADER_LEN + ip_header_len + 4
    {
        return false;
    }
    let udp_start = ETHERNET_HEADER_LEN + ip_header_len;
    let source = u16::from_be_bytes([frame[udp_start], frame[udp_start + 1]]);
    let destination = u16::from_be_bytes([frame[udp_start + 2], frame[udp_start + 3]]);
    matches!((source, destination), (67, 68) | (68, 67))
}

/// Extracts a PHQA payload from the DUT-to-host UDP echo direction.
#[must_use]
pub fn parse_udp_echo(frame: &[u8]) -> Option<QaFrameTag> {
    const ETHERNET_HEADER_LEN: usize = 14;
    const UDP_HEADER_LEN: usize = 8;

    if frame.get(12..14) != Some(&[0x08, 0x00]) {
        return None;
    }
    let version_ihl = *frame.get(ETHERNET_HEADER_LEN)?;
    if version_ihl >> 4 != 4 {
        return None;
    }
    let ip_header_len = usize::from(version_ihl & 0x0f) * 4;
    if ip_header_len < 20 {
        return None;
    }
    let ip = frame.get(ETHERNET_HEADER_LEN..ETHERNET_HEADER_LEN + ip_header_len)?;
    if ip[9] != 17 || ip[12..16] != QA_DUT_IPV4 || ip[16..20] != QA_HOST_IPV4 {
        return None;
    }
    let udp_start = ETHERNET_HEADER_LEN + ip_header_len;
    let udp = frame.get(udp_start..udp_start + UDP_HEADER_LEN)?;
    if u16::from_be_bytes([udp[0], udp[1]]) != QA_DUT_UDP_PORT
        || u16::from_be_bytes([udp[2], udp[3]]) != QA_HOST_UDP_PORT
    {
        return None;
    }
    let udp_len = usize::from(u16::from_be_bytes([udp[4], udp[5]]));
    if udp_len != UDP_HEADER_LEN + QA_FRAME_LEN {
        return None;
    }
    parse_qa_frame(frame.get(udp_start + UDP_HEADER_LEN..udp_start + udp_len)?)
}

fn internet_checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0_u32;
    for pair in bytes.chunks(2) {
        let word = if pair.len() == 2 {
            u16::from_be_bytes([pair[0], pair[1]])
        } else {
            u16::from(pair[0]) << 8
        };
        sum = sum.wrapping_add(u32::from(word));
        while sum > u32::from(u16::MAX) {
            sum = (sum & u32::from(u16::MAX)) + (sum >> 16);
        }
    }
    !(sum as u16)
}

fn udp_checksum(source: [u8; 4], destination: [u8; 4], udp: &[u8], protocol: u8) -> u16 {
    let mut pseudo_header = [0_u8; 12];
    pseudo_header[..4].copy_from_slice(&source);
    pseudo_header[4..8].copy_from_slice(&destination);
    pseudo_header[9] = protocol;
    pseudo_header[10..12].copy_from_slice(&(udp.len() as u16).to_be_bytes());

    let mut sum = checksum_sum(&pseudo_header).wrapping_add(checksum_sum(udp));
    while sum > u32::from(u16::MAX) {
        sum = (sum & u32::from(u16::MAX)) + (sum >> 16);
    }
    let checksum = !(sum as u16);
    if checksum == 0 { u16::MAX } else { checksum }
}

fn checksum_sum(bytes: &[u8]) -> u32 {
    bytes
        .chunks(2)
        .map(|pair| {
            u32::from(if pair.len() == 2 {
                u16::from_be_bytes([pair[0], pair[1]])
            } else {
                u16::from(pair[0]) << 8
            })
        })
        .sum()
}

/// Decodes the identity fields of a valid fixed-layout PHQA frame.
#[must_use]
pub fn parse_qa_frame(frame: &[u8]) -> Option<QaFrameTag> {
    if frame.len() != QA_FRAME_LEN
        || frame.get(QA_ETHERTYPE_OFFSET..QA_ETHERTYPE_OFFSET + 2)
            != Some(QA_ETHERTYPE.to_be_bytes().as_slice())
        || frame.get(QA_MAGIC_OFFSET..QA_MAGIC_OFFSET + 4) != Some(b"PHQA")
        || frame[QA_VERSION_OFFSET] != 1
    {
        return None;
    }
    let suite = Suite::from_wire_code(frame[QA_SUITE_OFFSET])?;
    let action = frame[QA_ACTION_OFFSET];
    if !(1..=14).contains(&action) {
        return None;
    }
    let run_len = usize::from(frame[QA_RUN_LEN_OFFSET]);
    if run_len == 0 || run_len > QA_RUN_CAPACITY {
        return None;
    }
    let run_ascii = frame.get(QA_RUN_OFFSET..QA_RUN_OFFSET + run_len)?;
    if !run_ascii
        .iter()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return None;
    }
    if frame[QA_RUN_OFFSET + run_len..QA_STEP_OFFSET]
        .iter()
        .any(|byte| *byte != 0)
        || frame[QA_SEQUENCE_OFFSET + 4..]
            .iter()
            .any(|byte| *byte != 0)
    {
        return None;
    }
    let run_id = u64::from_str_radix(std::str::from_utf8(run_ascii).ok()?, 16).ok()?;
    let step = u32::from_be_bytes(
        frame
            .get(QA_STEP_OFFSET..QA_STEP_OFFSET + 4)?
            .try_into()
            .ok()?,
    );
    let sequence = u32::from_be_bytes(
        frame
            .get(QA_SEQUENCE_OFFSET..QA_SEQUENCE_OFFSET + 4)?
            .try_into()
            .ok()?,
    );
    Some(QaFrameTag {
        suite,
        action,
        run_id,
        step,
        sequence,
    })
}

fn panic_or_reset_marker(line: &[u8]) -> bool {
    let text = String::from_utf8_lossy(line);
    text.contains("Guru Meditation Error")
        || text.contains("panicked at")
        || text.starts_with("rst:")
}

/// Captures useful tool versions without aborting a run when one is absent.
#[must_use]
pub fn collect_tool_versions() -> Vec<ToolVersion> {
    [
        ("rustc", vec!["rustc", "--version"]),
        ("cargo", vec!["cargo", "--version"]),
        (
            "esp-rustc",
            vec!["rustup", "run", "esp", "rustc", "--version"],
        ),
        ("espflash", vec!["espflash", "--version"]),
    ]
    .into_iter()
    .map(|(name, command)| {
        let mut process = Command::new(command[0]);
        process.args(&command[1..]);
        ToolVersion {
            name: name.to_owned(),
            version: command_text(&mut process)
                .unwrap_or_else(|error| format!("unavailable:{error}")),
        }
    })
    .collect()
}

fn command_text(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        bail!("command exited with {}", output.status);
    }
    let text = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    Ok(String::from_utf8_lossy(text)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned())
}

fn generate_run_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mixed = (timestamp as u64) ^ ((timestamp >> 64) as u64) ^ u64::from(std::process::id());
    format!("{:x}", mixed.max(1))
}

/// Strict session-driving error.
#[derive(Debug, Error)]
pub enum DriveError {
    /// Firmware did not emit `RUN_START` before the boot deadline.
    #[error("firmware boot timed out before RUN_START")]
    BootTimeout,
    /// The suite deadline expired.
    #[error("suite timed out before RUN_END")]
    Timeout,
    /// Serial transport failed.
    #[error("serial transport failed: {0}")]
    Serial(io::Error),
    /// Protocol parsing failed.
    #[error("protocol failed: {0}")]
    Protocol(#[from] crate::protocol::ProtocolError),
    /// Session validation failed.
    #[error("session validation failed: {0}")]
    Session(#[from] crate::session::SessionError),
    /// A host-side action failed.
    #[error("host action failed: {0}")]
    Action(String),
    /// Raw output indicated a panic or reset.
    #[error("firmware panic or reset before RUN_END: {0}")]
    UnexpectedResetText(String),
    /// Evidence streaming failed.
    #[error("evidence streaming failed: {0}")]
    Evidence(String),
}

/// In-memory adapters used by deterministic host tests.
pub mod fakes {
    use super::{
        ActionRunner, Flasher, PacketClassStats, PacketIo, PacketStats, SerialIo, parse_qa_frame,
    };
    use crate::{
        config::{CommandArgv, LabConfig},
        evidence::ActionRecord,
    };
    use anyhow::Result;
    use std::{
        collections::{BTreeMap, BTreeSet, VecDeque},
        io,
        path::{Path, PathBuf},
        time::Duration,
    };

    /// Chunked serial source with captured writes.
    #[derive(Default)]
    pub struct FakeSerial {
        /// Serial chunks returned in order.
        pub reads: VecDeque<Vec<u8>>,
        /// Bytes written by the controller.
        pub writes: Vec<Vec<u8>>,
    }

    impl SerialIo for FakeSerial {
        fn read(&mut self, buffer: &mut [u8], _timeout: Duration) -> io::Result<usize> {
            let Some(mut chunk) = self.reads.pop_front() else {
                return Ok(0);
            };
            let length = chunk.len().min(buffer.len());
            buffer[..length].copy_from_slice(&chunk[..length]);
            if length < chunk.len() {
                let remainder = chunk.split_off(length);
                self.reads.push_front(remainder);
            }
            Ok(length)
        }

        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.writes.push(bytes.to_vec());
            Ok(())
        }

        fn discard_input(&mut self) -> io::Result<()> {
            self.reads.clear();
            Ok(())
        }
    }

    /// Packet adapter that records every injected frame.
    #[derive(Default)]
    pub struct FakePacket {
        /// Capture path supplied by the harness.
        pub capture_path: Option<PathBuf>,
        /// BPF supplied by the harness.
        pub filter: Option<String>,
        /// Injected Ethernet frames.
        pub injected: Vec<Vec<u8>>,
    }

    impl PacketIo for FakePacket {
        fn start_capture(&mut self, path: &Path, filter: &str) -> Result<()> {
            self.capture_path = Some(path.to_path_buf());
            self.filter = Some(filter.to_owned());
            Ok(())
        }

        fn inject(&mut self, frame: &[u8]) -> Result<()> {
            self.injected.push(frame.to_vec());
            Ok(())
        }

        fn finish_capture(&mut self) -> Result<PacketStats> {
            let mut classes = BTreeMap::<_, BTreeSet<u32>>::new();
            for frame in &self.injected {
                if let Some(tag) = parse_qa_frame(frame) {
                    classes
                        .entry((tag.suite, tag.action, tag.run_id, tag.step))
                        .or_default()
                        .insert(tag.sequence);
                }
            }
            Ok(PacketStats {
                total_frames: self.injected.len() as u64,
                qa_frames: self
                    .injected
                    .iter()
                    .filter(|frame| frame.get(12..14) == Some(&[0x88, 0xb5]))
                    .count() as u64,
                classes: classes
                    .into_iter()
                    .map(
                        |((suite, action, run_id, step), sequences)| PacketClassStats {
                            suite,
                            action,
                            run_id,
                            step,
                            unique_sequences: sequences.len() as u64,
                        },
                    )
                    .collect(),
                udp_echoes: Vec::new(),
                arp_replies: 0,
                dhcp_frames: 0,
            })
        }
    }

    /// Flasher that records the ELF path without touching hardware.
    #[derive(Default)]
    pub struct FakeFlasher {
        /// Last firmware passed to the adapter.
        pub firmware: Option<PathBuf>,
    }

    impl Flasher for FakeFlasher {
        fn flash(&mut self, firmware: &Path, _lab: &LabConfig) -> Result<()> {
            self.firmware = Some(firmware.to_path_buf());
            Ok(())
        }
    }

    /// Action runner that records literal command execution.
    #[derive(Default)]
    pub struct FakeActionRunner {
        /// Actions observed in order.
        pub actions: Vec<String>,
    }

    impl ActionRunner for FakeActionRunner {
        fn run(
            &mut self,
            action: &str,
            command: &CommandArgv,
            _base_dir: &Path,
            _timeout: Duration,
        ) -> Result<ActionRecord> {
            self.actions.push(action.to_owned());
            Ok(ActionRecord {
                action: action.to_owned(),
                executable: command.executable().to_owned(),
                exit_code: Some(0),
                duration_ms: 0,
                success: true,
                suite: None,
                run_id: None,
                step: None,
                count: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, fs};

    use tempfile::tempdir;

    use crate::{
        config::LabConfig,
        evidence::EvidenceBundle,
        orchestrator::fakes::{FakeActionRunner, FakePacket, FakeSerial},
        suite::Suite,
    };

    use super::{
        DriveError, QA_ACTION_OFFSET, QA_RUN_LEN_OFFSET, QA_RUN_OFFSET, QA_SEQUENCE_OFFSET,
        QA_STEP_OFFSET, arp_request, destination_for_action, drive_session, internet_checksum,
        is_arp_reply, is_dhcp_frame, parse_qa_frame, parse_udp_echo, qa_action_code, qa_frame,
        udp_echo_challenge,
    };

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn lab(temp: &std::path::Path) -> LabConfig {
        let path = temp.join("lab.toml");
        fs::write(
            &path,
            r#"
schema = 1
board_id = "test"
[serial]
port = "COM1"
baud = 115200
read_timeout_ms = 1
boot_timeout_ms = 10
[packet]
adapter = "fake"
host_mac = "02:00:00:00:00:01"
dut_mac = "02:00:00:00:00:02"
capture_timeout_ms = 1
[tools]
espflash = ["fake"]
[actions]
power_off = ["fake", "power_off"]
power_on = ["fake", "power_on"]
link_down = ["fake", "link_down"]
link_up = ["fake", "link_up"]
[timeouts]
action_ms = 10
flash_ms = 10
suite_ms = 10
[settle]
power_ms = 0
link_ms = 0
"#,
        )
        .unwrap();
        LabConfig::load(path).unwrap()
    }

    #[test]
    fn frame_has_deterministic_ethernet_header() {
        let frame = qa_frame(
            [2, 0, 0, 0, 0, 2],
            [2, 0, 0, 0, 0, 1],
            Suite::RxSync,
            11,
            0xdead_beef,
            7,
            9,
        )
        .unwrap();
        assert_eq!(&frame[..6], &[2, 0, 0, 0, 0, 2]);
        assert_eq!(&frame[6..12], &[2, 0, 0, 0, 0, 1]);
        assert_eq!(&frame[12..14], &0x88b5_u16.to_be_bytes());
        assert_eq!(&frame[14..18], b"PHQA");
        assert_eq!(frame[QA_ACTION_OFFSET], 11);
        assert_eq!(frame[QA_RUN_LEN_OFFSET], 8);
        assert_eq!(&frame[QA_RUN_OFFSET..QA_RUN_OFFSET + 8], b"deadbeef");
        assert_eq!(
            &frame[QA_STEP_OFFSET..QA_STEP_OFFSET + 4],
            &7_u32.to_be_bytes()
        );
        assert_eq!(
            &frame[QA_SEQUENCE_OFFSET..QA_SEQUENCE_OFFSET + 4],
            &9_u32.to_be_bytes()
        );
        assert_eq!(
            parse_qa_frame(&frame).unwrap(),
            super::QaFrameTag {
                suite: Suite::RxSync,
                action: 11,
                run_id: 0xdead_beef,
                step: 7,
                sequence: 9,
            }
        );
    }

    #[test]
    fn action_codes_match_the_firmware_contract() {
        assert_eq!(qa_action_code("link_down"), Some(1));
        assert_eq!(qa_action_code("inject_unicast"), Some(3));
        assert_eq!(qa_action_code("inject_new_mac"), Some(8));
        assert_eq!(qa_action_code("flood_rx"), Some(10));
        assert_eq!(qa_action_code("dhcp"), Some(14));
        assert_eq!(qa_action_code("unknown"), None);
    }

    #[test]
    fn udp_challenge_has_valid_ipv4_identity_and_echo_payload() {
        let dut = [2, 0, 0, 0, 0, 2];
        let host = [2, 0, 0, 0, 0, 1];
        let mut frame = udp_echo_challenge(dut, host, 0xdead_beef, 3).unwrap();
        assert_eq!(&frame[..6], &dut);
        assert_eq!(&frame[6..12], &host);
        assert_eq!(&frame[12..14], &[0x08, 0x00]);
        assert_eq!(internet_checksum(&frame[14..34]), 0);

        frame[..6].copy_from_slice(&host);
        frame[6..12].copy_from_slice(&dut);
        frame[26..30].copy_from_slice(&super::QA_DUT_IPV4);
        frame[30..34].copy_from_slice(&super::QA_HOST_IPV4);
        frame[34..36].copy_from_slice(&super::QA_DUT_UDP_PORT.to_be_bytes());
        frame[36..38].copy_from_slice(&super::QA_HOST_UDP_PORT.to_be_bytes());
        let tag = parse_udp_echo(&frame).unwrap();
        assert_eq!(tag.suite, Suite::RxEmbassy);
        assert_eq!(tag.action, 13);
        assert_eq!(tag.run_id, 0xdead_beef);
        assert_eq!(tag.step, 3);
        assert_eq!(tag.sequence, 0);
    }

    #[test]
    fn arp_request_primes_the_fixed_ipv4_neighbor() {
        let host = [2, 0, 0, 0, 0, 1];
        let mut frame = arp_request(host);
        assert_eq!(&frame[..6], &[0xff; 6]);
        assert_eq!(&frame[6..12], &host);
        assert_eq!(&frame[12..14], &[0x08, 0x06]);
        assert_eq!(&frame[20..22], &[0, 1]);
        assert_eq!(&frame[28..32], &super::QA_HOST_IPV4);
        assert_eq!(&frame[38..42], &super::QA_DUT_IPV4);

        frame[20..22].copy_from_slice(&2_u16.to_be_bytes());
        frame[28..32].copy_from_slice(&super::QA_DUT_IPV4);
        frame[38..42].copy_from_slice(&super::QA_HOST_IPV4);
        assert!(is_arp_reply(&frame));
    }

    #[test]
    fn recognizes_dhcp_udp_ports() {
        let mut frame = vec![0_u8; 42];
        frame[12..14].copy_from_slice(&0x0800_u16.to_be_bytes());
        frame[14] = 0x45;
        frame[23] = 17;
        frame[34..36].copy_from_slice(&68_u16.to_be_bytes());
        frame[36..38].copy_from_slice(&67_u16.to_be_bytes());
        assert!(is_dhcp_frame(&frame));
        frame[36..38].copy_from_slice(&53_u16.to_be_bytes());
        assert!(!is_dhcp_frame(&frame));
    }

    #[test]
    fn qa_frame_parser_rejects_nonzero_padding() {
        let mut frame = qa_frame(
            [2, 0, 0, 0, 0, 2],
            [2, 0, 0, 0, 0, 1],
            Suite::RxSync,
            11,
            1,
            1,
            0,
        )
        .unwrap();
        frame[37] = 1;
        assert!(parse_qa_frame(&frame).is_none());
    }

    #[test]
    fn mac_filter_destinations_derive_from_the_lab_dut() {
        let a = [0x02, 0x00, 0x00, 0x12, 0x01, 0x02];
        assert_eq!(destination_for_action("inject_old_mac", a).unwrap(), a);
        assert_eq!(
            destination_for_action("inject_new_mac", a).unwrap(),
            [0x02, 0x00, 0x00, 0x12, 0x01, 0x03]
        );
        assert_eq!(
            destination_for_action("inject_alien", a).unwrap(),
            [0x02, 0x00, 0x00, 0x12, 0x01, 0xfe]
        );
        let overflow = [0x02, 0, 0, 0, 0, 0xff];
        assert!(destination_for_action("inject_new_mac", overflow).is_err());
    }

    #[test]
    fn drive_session_times_out_on_absent_traffic() {
        let temp = tempdir().unwrap();
        let lab = lab(temp.path());
        let mut evidence = EvidenceBundle::create(
            temp.path().join("evidence"),
            Suite::Mdio,
            COMMIT,
            false,
            "test",
        )
        .unwrap();
        let mut serial = FakeSerial::default();
        let mut packet = FakePacket::default();
        let mut actions = FakeActionRunner::default();
        let mut records = Vec::new();
        assert!(matches!(
            drive_session(
                &mut serial,
                &mut packet,
                &mut actions,
                &lab,
                Suite::Mdio,
                COMMIT,
                0x1234,
                &mut evidence,
                &mut records,
            ),
            Err(DriveError::BootTimeout)
        ));
    }

    #[test]
    fn drive_session_detects_missing_required_ids() {
        let temp = tempdir().unwrap();
        let lab = lab(temp.path());
        let mut evidence = EvidenceBundle::create(
            temp.path().join("evidence"),
            Suite::Mdio,
            COMMIT,
            false,
            "test",
        )
        .unwrap();
        let input = format!(
            "PHQA|1|RUN_START|run=1234|suite=mdio|commit={COMMIT}|mode=release|reset_reason=power_on\n\
             PHQA|1|TEST|run=1234|id=mdio.read-repeat|status=BLOCKED|required=1|reason=not-implemented\n\
             PHQA|1|RUN_END|run=1234|result=FAIL\n"
        );
        let mut serial = FakeSerial {
            reads: VecDeque::from([input.into_bytes()]),
            writes: Vec::new(),
        };
        let mut packet = FakePacket::default();
        let mut actions = FakeActionRunner::default();
        let mut records = Vec::new();
        assert!(matches!(
            drive_session(
                &mut serial,
                &mut packet,
                &mut actions,
                &lab,
                Suite::Mdio,
                COMMIT,
                0x1234,
                &mut evidence,
                &mut records,
            ),
            Err(DriveError::Session(
                crate::session::SessionError::MissingTests(_)
            ))
        ));
    }
}
