//! Windows serial and Npcap adapters.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use pcap::{Active, Capture, Device, Error as PcapError, Linktype};
use serial2::SerialPort;

use crate::{
    config::{PacketConfig, SerialConfig},
    evidence::ToolVersion,
    orchestrator::{
        DhcpTransaction, DhcpTransactionTracker, PacketClassStats, PacketIo, PacketStats, SerialIo,
        UdpEchoCapture, UdpEchoExpectation, is_arp_reply, is_dhcp_frame, matches_udp_echo,
        parse_qa_frame, parse_udp_echo,
    },
};

/// Blocking Windows serial adapter with bounded reads.
pub struct WindowsSerial {
    port: SerialPort,
}

impl WindowsSerial {
    /// Opens the configured COM port without discarding early boot records.
    pub fn open(config: &SerialConfig) -> Result<Self> {
        let mut port = SerialPort::open(&config.port, config.baud)
            .with_context(|| format!("open serial port {}", config.port))?;
        port.set_read_timeout(Duration::from_millis(config.read_timeout_ms))?;
        Ok(Self { port })
    }

    /// Reconnects a power-gated USB serial device until a bounded deadline.
    ///
    /// The successful port is not purged because firmware may already have
    /// emitted early boot bytes after `power_on`.
    pub fn open_bounded(config: &SerialConfig, timeout: Duration) -> Result<Self> {
        let deadline = Instant::now() + timeout;
        loop {
            match Self::open(config) {
                Ok(serial) => return Ok(serial),
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "serial port {} did not reconnect before deadline",
                            config.port
                        )
                    });
                }
            }
        }
    }
}

impl SerialIo for WindowsSerial {
    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize> {
        self.port.set_read_timeout(timeout)?;
        match self.port.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::TimedOut => Ok(0),
            result => result,
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.port.write_all(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.port.flush()
    }

    fn discard_input(&mut self) -> io::Result<()> {
        self.port.discard_input_buffer()
    }
}

/// Npcap capture/injection adapter.
pub struct NpcapPacket {
    device: String,
    timeout_ms: i32,
    dut_mac: [u8; 6],
    injector: Capture<Active>,
    stop: Option<Sender<()>>,
    capture: Option<JoinHandle<Result<PacketStats, String>>>,
    events: Arc<CaptureEvents>,
}

#[derive(Default)]
struct CaptureEvents {
    state: Mutex<CaptureEventState>,
    changed: Condvar,
}

#[derive(Default)]
struct CaptureEventState {
    failure: Option<String>,
    udp: Option<ArmedUdpEcho>,
    dhcp: Option<ArmedDhcp>,
}

struct ArmedUdpEcho {
    expected: UdpEchoExpectation,
    armed_at: Instant,
    armed_epoch_micros: u128,
    matched: Option<UdpEchoCapture>,
}

struct ArmedDhcp {
    tracker: DhcpTransactionTracker,
    armed_at: Instant,
    armed_epoch_micros: u128,
    matched: Option<DhcpTransaction>,
}

impl CaptureEvents {
    fn arm_udp(&self, expected: UdpEchoExpectation) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Npcap event state lock was poisoned"))?;
        if let Some(failure) = &state.failure {
            bail!("Npcap capture failed before UDP arm: {failure}");
        }
        state.udp = Some(ArmedUdpEcho {
            expected,
            armed_at: Instant::now(),
            armed_epoch_micros: unix_epoch_micros(SystemTime::now())?,
            matched: None,
        });
        Ok(())
    }

    fn arm_dhcp(&self, dut_mac: [u8; 6]) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Npcap event state lock was poisoned"))?;
        if let Some(failure) = &state.failure {
            bail!("Npcap capture failed before DHCP arm: {failure}");
        }
        state.dhcp = Some(ArmedDhcp {
            tracker: DhcpTransactionTracker::new(dut_mac),
            armed_at: Instant::now(),
            armed_epoch_micros: unix_epoch_micros(SystemTime::now())?,
            matched: None,
        });
        Ok(())
    }

    fn observe(&self, frame: &[u8], packet_epoch_micros: Option<u128>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if let Some(armed) = &mut state.udp
            && armed.matched.is_none()
            && packet_epoch_micros.is_some_and(|timestamp| timestamp >= armed.armed_epoch_micros)
            && matches_udp_echo(frame, armed.expected)
        {
            armed.matched = Some(UdpEchoCapture {
                latency: armed.armed_at.elapsed(),
            });
            self.changed.notify_all();
        }
        if let Some(armed) = &mut state.dhcp
            && armed.matched.is_none()
            && packet_epoch_micros.is_some_and(|timestamp| timestamp >= armed.armed_epoch_micros)
            && let Some(message) = armed.tracker.observe(frame)
        {
            armed.matched = Some(DhcpTransaction {
                xid: message.xid,
                yiaddr: message.yiaddr,
                latency: armed.armed_at.elapsed(),
            });
            self.changed.notify_all();
        }
    }

    fn wait_udp(&self, timeout: Duration) -> Result<UdpEchoCapture> {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Npcap event state lock was poisoned"))?;
        loop {
            if let Some(failure) = &state.failure {
                bail!("Npcap capture failed while waiting for UDP echo: {failure}");
            }
            let armed = state
                .udp
                .as_mut()
                .ok_or_else(|| anyhow!("UDP capture was not armed"))?;
            if let Some(capture) = armed.matched.take() {
                state.udp = None;
                return Ok(capture);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.udp = None;
                bail!("timed out waiting for exact DUT-to-host UDP echo");
            }
            let (next, result) = self
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| anyhow!("Npcap event state lock was poisoned"))?;
            state = next;
            if result.timed_out() {
                state.udp = None;
                bail!("timed out waiting for exact DUT-to-host UDP echo");
            }
        }
    }

    fn wait_dhcp(&self, timeout: Duration) -> Result<DhcpTransaction> {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Npcap event state lock was poisoned"))?;
        loop {
            if let Some(failure) = &state.failure {
                bail!("Npcap capture failed while waiting for DHCP: {failure}");
            }
            let armed = state
                .dhcp
                .as_mut()
                .ok_or_else(|| anyhow!("DHCP capture was not armed"))?;
            if let Some(transaction) = armed.matched.take() {
                state.dhcp = None;
                return Ok(transaction);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.dhcp = None;
                bail!("timed out waiting for post-arm DHCP Request/ACK transaction");
            }
            let (next, result) = self
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| anyhow!("Npcap event state lock was poisoned"))?;
            state = next;
            if result.timed_out() {
                state.dhcp = None;
                bail!("timed out waiting for post-arm DHCP Request/ACK transaction");
            }
        }
    }

    fn fail(&self, failure: String) {
        if let Ok(mut state) = self.state.lock() {
            state.failure = Some(failure);
            self.changed.notify_all();
        }
    }
}

impl NpcapPacket {
    /// Opens an injection handle for the configured Npcap device.
    pub fn open(config: &PacketConfig) -> Result<Self> {
        let injector = open_capture(&config.adapter, config.capture_timeout_ms)?;
        Ok(Self {
            device: config.adapter.clone(),
            timeout_ms: config.capture_timeout_ms,
            dut_mac: config.parsed_dut_mac()?,
            injector,
            stop: None,
            capture: None,
            events: Arc::new(CaptureEvents::default()),
        })
    }

    /// Returns the Rust packet-adapter dependency version.
    #[must_use]
    pub fn version() -> ToolVersion {
        ToolVersion {
            name: "pcap-rs".to_owned(),
            version: "2.4.0".to_owned(),
        }
    }
}

impl PacketIo for NpcapPacket {
    fn start_capture(&mut self, path: &Path, filter: &str) -> Result<()> {
        if self.capture.is_some() {
            bail!("packet capture is already active");
        }
        let device = self.device.clone();
        let timeout_ms = self.timeout_ms;
        let path = path.to_path_buf();
        let filter = filter.to_owned();
        let events = Arc::clone(&self.events);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let result = capture_loop(
                &device, timeout_ms, &path, &filter, stop_rx, ready_tx, &events,
            );
            result.map_err(|error| {
                let message = format!("{error:#}");
                events.fail(message.clone());
                message
            })
        });
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                self.stop = Some(stop_tx);
                self.capture = Some(handle);
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(anyhow!(error))
            }
            Err(error) => {
                let _ = stop_tx.send(());
                let _ = handle.join();
                Err(anyhow!("Npcap capture startup timed out: {error}"))
            }
        }
    }

    fn inject(&mut self, frame: &[u8]) -> Result<()> {
        self.injector
            .sendpacket(frame)
            .context("inject Ethernet frame through Npcap")
    }

    fn arm_udp_echo(&mut self, expected: UdpEchoExpectation) -> Result<()> {
        if self.capture.is_none() {
            bail!("packet capture is not active");
        }
        self.events.arm_udp(expected)
    }

    fn wait_udp_echo(&mut self, timeout: Duration) -> Result<UdpEchoCapture> {
        self.events.wait_udp(timeout)
    }

    fn arm_dhcp(&mut self, dut_mac: [u8; 6]) -> Result<()> {
        if self.capture.is_none() {
            bail!("packet capture is not active");
        }
        if dut_mac != self.dut_mac {
            bail!("DHCP arm address does not match configured DUT");
        }
        self.events.arm_dhcp(dut_mac)
    }

    fn wait_dhcp(&mut self, timeout: Duration) -> Result<DhcpTransaction> {
        self.events.wait_dhcp(timeout)
    }

    fn finish_capture(&mut self) -> Result<PacketStats> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let Some(handle) = self.capture.take() else {
            return Ok(PacketStats::default());
        };
        handle
            .join()
            .map_err(|_| anyhow!("Npcap capture thread panicked"))?
            .map_err(anyhow::Error::msg)
    }
}

impl Drop for NpcapPacket {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(handle) = self.capture.take() {
            let _ = handle.join();
        }
    }
}

fn open_capture(device: &str, timeout_ms: i32) -> Result<Capture<Active>> {
    let listed = Device::list()
        .context("list Npcap devices")?
        .into_iter()
        .find(|candidate| candidate.name == device)
        .ok_or_else(|| anyhow!("Npcap device `{device}` was not found"))?;
    let capture = Capture::from_device(listed)?
        .promisc(true)
        .immediate_mode(true)
        .timeout(timeout_ms)
        .open()
        .context("open Npcap device")?;
    let linktype = capture.get_datalink();
    if linktype != Linktype::ETHERNET {
        bail!("Npcap device `{device}` has non-Ethernet datalink {linktype:?}");
    }
    Ok(capture)
}

fn capture_loop(
    device: &str,
    timeout_ms: i32,
    path: &PathBuf,
    filter: &str,
    stop: mpsc::Receiver<()>,
    ready: Sender<Result<(), String>>,
    events: &CaptureEvents,
) -> Result<PacketStats> {
    let mut capture = match open_capture(device, timeout_ms) {
        Ok(capture) => capture,
        Err(error) => {
            let message = format!("{error:#}");
            let _ = ready.send(Err(message.clone()));
            bail!(message);
        }
    };
    capture
        .filter(filter, true)
        .with_context(|| format!("compile BPF `{filter}`"))?;
    let mut savefile = capture
        .savefile(path)
        .with_context(|| format!("open pcap artifact {}", path.display()))?;
    ready
        .send(Ok(()))
        .map_err(|_| anyhow!("capture caller disappeared"))?;

    let mut stats = PacketStats::default();
    let mut sequences = BTreeMap::<_, BTreeSet<u32>>::new();
    let mut udp_echoes = BTreeMap::<_, BTreeSet<u32>>::new();
    loop {
        if stop.try_recv().is_ok() {
            break;
        }
        match capture.next_packet() {
            Ok(packet) => {
                savefile.write(&packet);
                events.observe(packet.data, packet_epoch_micros(&packet));
                stats.total_frames += 1;
                if packet.data.get(12..14) == Some(&[0x88, 0xb5]) {
                    stats.qa_frames += 1;
                }
                if let Some(tag) = parse_qa_frame(packet.data) {
                    sequences
                        .entry((tag.suite, tag.action, tag.run_id, tag.step, tag.destination))
                        .or_default()
                        .insert(tag.sequence);
                }
                if let Some(tag) = parse_udp_echo(packet.data) {
                    udp_echoes
                        .entry((tag.suite, tag.action, tag.run_id, tag.step, tag.destination))
                        .or_default()
                        .insert(tag.sequence);
                }
                if is_arp_reply(packet.data) {
                    stats.arp_replies += 1;
                }
                if is_dhcp_frame(packet.data) {
                    stats.dhcp_frames += 1;
                }
            }
            Err(PcapError::TimeoutExpired) => {}
            Err(error) => return Err(error).context("capture Npcap packet"),
        }
    }
    savefile.flush()?;
    stats.classes = sequences
        .into_iter()
        .map(|((suite, action, run_id, step, destination), sequences)| {
            let sequences: Vec<_> = sequences.into_iter().collect();
            PacketClassStats {
                suite,
                action,
                run_id,
                step,
                destination,
                unique_sequences: sequences.len() as u64,
                sequences,
            }
        })
        .collect();
    stats.udp_echoes = udp_echoes
        .into_iter()
        .map(|((suite, action, run_id, step, destination), sequences)| {
            let sequences: Vec<_> = sequences.into_iter().collect();
            PacketClassStats {
                suite,
                action,
                run_id,
                step,
                destination,
                unique_sequences: sequences.len() as u64,
                sequences,
            }
        })
        .collect();
    Ok(stats)
}

fn unix_epoch_micros(time: SystemTime) -> Result<u128> {
    Ok(time
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_micros())
}

fn packet_epoch_micros(packet: &pcap::Packet<'_>) -> Option<u128> {
    let seconds = u128::try_from(packet.header.ts.tv_sec).ok()?;
    let micros = u128::try_from(packet.header.ts.tv_usec).ok()?;
    seconds.checked_mul(1_000_000)?.checked_add(micros)
}
