//! Real ETH_MAC interrupt observations for QA assertions.

use core::sync::atomic::{AtomicU32, Ordering};

use ph_esp32_mac::InterruptStatus;
use ph_esp32_mac::unsafe_registers::DmaRegs;

/// Monotonic interrupt counter snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InterruptSnapshot {
    /// Number of ETH_MAC ISR entries.
    pub total: u32,
    /// Number of receive-complete events.
    pub ri: u32,
    /// Number of receive-buffer-unavailable events.
    pub ru: u32,
    /// Number of abnormal-summary events.
    pub ais: u32,
    /// Number of fatal-bus-error events.
    pub fbi: u32,
    /// Raw DMA status observed by the latest ISR entry.
    pub last_raw: u32,
}

impl InterruptSnapshot {
    /// Calculate wrapping counter deltas while preserving the latest raw status.
    pub const fn since(self, earlier: Self) -> Self {
        Self {
            total: self.total.wrapping_sub(earlier.total),
            ri: self.ri.wrapping_sub(earlier.ri),
            ru: self.ru.wrapping_sub(earlier.ru),
            ais: self.ais.wrapping_sub(earlier.ais),
            fbi: self.fbi.wrapping_sub(earlier.fbi),
            last_raw: self.last_raw,
        }
    }
}

/// Atomic counters updated by the real ETH_MAC ISR.
pub struct InterruptCounters {
    total: AtomicU32,
    ri: AtomicU32,
    ru: AtomicU32,
    ais: AtomicU32,
    fbi: AtomicU32,
    last_raw: AtomicU32,
}

impl InterruptCounters {
    /// Create zeroed interrupt counters.
    pub const fn new() -> Self {
        Self {
            total: AtomicU32::new(0),
            ri: AtomicU32::new(0),
            ru: AtomicU32::new(0),
            ais: AtomicU32::new(0),
            fbi: AtomicU32::new(0),
            last_raw: AtomicU32::new(0),
        }
    }

    /// Record one raw DMA status without clearing hardware.
    ///
    /// Keeping observation separate from acknowledgement lets future async and
    /// embassy handlers record the status before their state object clears it.
    pub fn observe(&self, raw: u32) {
        let status = InterruptStatus::from_raw(raw);
        self.total.fetch_add(1, Ordering::Relaxed);
        self.last_raw.store(raw, Ordering::Relaxed);

        if status.rx_complete {
            self.ri.fetch_add(1, Ordering::Relaxed);
        }
        if status.rx_buf_unavailable {
            self.ru.fetch_add(1, Ordering::Relaxed);
        }
        if status.abnormal_summary {
            self.ais.fetch_add(1, Ordering::Relaxed);
        }
        if status.fatal_bus_error {
            self.fbi.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Load a consistent-enough monotonic snapshot for before/after assertions.
    pub fn snapshot(&self) -> InterruptSnapshot {
        InterruptSnapshot {
            total: self.total.load(Ordering::Relaxed),
            ri: self.ri.load(Ordering::Relaxed),
            ru: self.ru.load(Ordering::Relaxed),
            ais: self.ais.load(Ordering::Relaxed),
            fbi: self.fbi.load(Ordering::Relaxed),
            last_raw: self.last_raw.load(Ordering::Relaxed),
        }
    }
}

impl Default for InterruptCounters {
    fn default() -> Self {
        Self::new()
    }
}

/// Global counters for the QA firmware's single ETH_MAC interrupt source.
pub static INTERRUPTS: InterruptCounters = InterruptCounters::new();

/// Observe and acknowledge one synchronous ETH_MAC interrupt.
pub fn service_sync_interrupt() {
    let raw = DmaRegs::status();
    INTERRUPTS.observe(raw);
    DmaRegs::set_status(InterruptStatus::from_raw(raw).to_raw());
}
