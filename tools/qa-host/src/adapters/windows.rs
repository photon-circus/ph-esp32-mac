//! Windows serial and Npcap adapters.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use pcap::{Active, Capture, Device, Error as PcapError, Linktype};
use serial2::SerialPort;

use crate::{
    config::{PacketConfig, SerialConfig},
    evidence::ToolVersion,
    orchestrator::{
        PacketClassStats, PacketIo, PacketStats, SerialIo, is_arp_reply, is_dhcp_frame,
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

    fn discard_input(&mut self) -> io::Result<()> {
        self.port.discard_input_buffer()
    }
}

/// Npcap capture/injection adapter.
pub struct NpcapPacket {
    device: String,
    timeout_ms: i32,
    injector: Capture<Active>,
    stop: Option<Sender<()>>,
    capture: Option<JoinHandle<Result<PacketStats, String>>>,
}

impl NpcapPacket {
    /// Opens an injection handle for the configured Npcap device.
    pub fn open(config: &PacketConfig) -> Result<Self> {
        let injector = open_capture(&config.adapter, config.capture_timeout_ms)?;
        Ok(Self {
            device: config.adapter.clone(),
            timeout_ms: config.capture_timeout_ms,
            injector,
            stop: None,
            capture: None,
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
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let result = capture_loop(&device, timeout_ms, &path, &filter, stop_rx, ready_tx);
            result.map_err(|error| format!("{error:#}"))
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
                stats.total_frames += 1;
                if packet.data.get(12..14) == Some(&[0x88, 0xb5]) {
                    stats.qa_frames += 1;
                }
                if let Some(tag) = parse_qa_frame(packet.data) {
                    sequences
                        .entry((tag.suite, tag.action, tag.run_id, tag.step))
                        .or_default()
                        .insert(tag.sequence);
                }
                if let Some(tag) = parse_udp_echo(packet.data) {
                    udp_echoes
                        .entry((tag.suite, tag.action, tag.run_id, tag.step))
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
        .map(
            |((suite, action, run_id, step), sequences)| PacketClassStats {
                suite,
                action,
                run_id,
                step,
                unique_sequences: sequences.len() as u64,
            },
        )
        .collect();
    stats.udp_echoes = udp_echoes
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
        .collect();
    Ok(stats)
}
