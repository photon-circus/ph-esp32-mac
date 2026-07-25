//! WT32-ETH01 board context shared by QA firmware.

use core::cell::RefCell;

use critical_section::Mutex;
use esp_hal::gpio::{Level, Output, OutputConfig, OutputPin};
use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01;
use ph_esp32_mac::hal::MdioController;
use ph_esp32_mac::{Duplex, Emac, Lan8720a, Speed};

/// IO_MUX peripheral base address.
pub const IO_MUX_BASE: u32 = 0x3FF4_9000;
/// EMAC DMA peripheral base address.
pub const DMA_BASE: u32 = 0x3FF6_9000;
/// EMAC MAC peripheral base address.
pub const MAC_BASE: u32 = 0x3FF6_A000;
/// DPORT peripheral clock-enable register address.
pub const DPORT_WIFI_CLK_EN: u32 = 0x3FF0_00CC;

/// Shared state used by the exploratory test groups.
pub struct TestContext<'a> {
    /// Canonical LAN8720A PHY driver.
    pub phy: Lan8720a,
    /// ESP32 MDIO controller.
    pub mdio: MdioController<esp_hal::delay::Delay>,
    /// GPIO16 oscillator-enable pin kept alive for the run.
    pub clk_pin: Option<Output<'a>>,
    /// Most recently negotiated link speed.
    pub link_speed: Speed,
    /// Most recently negotiated duplex mode.
    pub link_duplex: Duplex,
    /// Whether EMAC initialization completed.
    pub emac_initialized: bool,
    /// Whether the link-up prerequisite completed.
    pub link_up: bool,
}

impl<'a> TestContext<'a> {
    /// Create board test state around an enabled oscillator pin.
    pub fn new(clk_pin: Output<'a>) -> Self {
        Self {
            phy: Wt32Eth01::lan8720a(),
            mdio: MdioController::new(esp_hal::delay::Delay::new()),
            clk_pin: Some(clk_pin),
            link_speed: Speed::Mbps100,
            link_duplex: Duplex::Full,
            emac_initialized: false,
            link_up: false,
        }
    }
}

/// Static EMAC instance with four RX/TX descriptors and 1600-byte buffers.
pub static EMAC: Mutex<RefCell<Option<Emac<4, 4, 1600>>>> = Mutex::new(RefCell::new(None));

/// Return a stable PHQA token for the reset that started this firmware.
pub fn reset_reason_token() -> &'static str {
    use esp_hal::rtc_cntl::SocResetReason;

    match esp_hal::system::reset_reason() {
        None => "unknown",
        Some(SocResetReason::ChipPowerOn) => "power_on",
        Some(SocResetReason::CoreSw) => "core_software",
        Some(SocResetReason::CoreDeepSleep) => "deep_sleep",
        Some(SocResetReason::CoreSdio) => "sdio",
        Some(SocResetReason::CoreMwdt0) => "mwdt0_core",
        Some(SocResetReason::CoreMwdt1) => "mwdt1_core",
        Some(SocResetReason::CoreRtcWdt) => "rtc_wdt_core",
        Some(SocResetReason::CpuMwdt0) => "mwdt0_cpu",
        Some(SocResetReason::Cpu0Sw) => "cpu_software",
        Some(SocResetReason::Cpu0RtcWdt) => "rtc_wdt_cpu",
        Some(SocResetReason::Cpu1Cpu0) => "cpu1_by_cpu0",
        Some(SocResetReason::SysBrownOut) => "brownout",
        Some(SocResetReason::SysRtcWdt) => "rtc_wdt_system",
    }
}

/// Enable the WT32-ETH01 external oscillator and wait for stabilization.
pub fn enable_wt32_oscillator<'d>(pin: impl OutputPin + 'd) -> Output<'d> {
    let output = Output::new(pin, Level::High, OutputConfig::default());
    esp_hal::delay::Delay::new().delay_millis(Wt32Eth01::OSC_STARTUP_MS);
    output
}
