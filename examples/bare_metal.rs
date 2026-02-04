//! Bare Metal Example - No esp-hal
//!
//! This example demonstrates using ph-esp32-mac without any HAL dependencies.
//! It shows the lowest-level usage pattern suitable for custom no_std environments.
//!
//! # Requirements
//!
//! - ESP32 with Ethernet PHY (e.g., LAN8720A)
//! - Custom startup/runtime (not esp-hal)
//! - Provide your own `DelayNs` implementation
//!
//! # Building
//!
//! This example cannot be built directly - it's a reference for bare-metal usage.
//! Copy the patterns into your custom ESP32 project.

#![no_std]
#![no_main]

// In a real bare-metal project, you'd have your own panic handler and runtime
// extern crate panic_halt;

use core::ptr::{read_volatile, write_volatile};

use embedded_hal::delay::DelayNs;

use ph_esp32_mac::{
    Duplex, Emac, EmacConfig, Lan8720a, MdioController, PhyDriver, PhyInterface, RmiiClockMode,
    Speed,
};

// =============================================================================
// Custom Delay Implementation
// =============================================================================

/// Simple busy-wait delay using CPU cycles
///
/// In a real project, calibrate CYCLES_PER_US for your CPU frequency.
pub struct BusyDelay;

impl BusyDelay {
    /// CPU cycles per microsecond (adjust for your clock speed)
    /// ESP32 at 240 MHz = 240 cycles/us
    const CYCLES_PER_US: u32 = 240;

    pub const fn new() -> Self {
        Self
    }
}

impl DelayNs for BusyDelay {
    fn delay_ns(&mut self, ns: u32) {
        let cycles = (ns as u64 * Self::CYCLES_PER_US as u64) / 1000;
        for _ in 0..cycles {
            // Prevent optimization
            core::hint::spin_loop();
        }
    }
}

// =============================================================================
// Static EMAC Instance
// =============================================================================

/// Static EMAC instance - must be in DMA-capable memory
///
/// On ESP32, place in `.dram1` section for DMA access.
/// Adjust buffer counts based on your memory constraints.
#[link_section = ".dram1"]
static mut EMAC: Emac<8, 8, 1600> = Emac::new();

// =============================================================================
// Hardware Constants
// =============================================================================

/// PHY address on the MDIO bus (board-specific)
const PHY_ADDR: u8 = 1;

/// GPIO for clock enable (if using external oscillator)
const CLK_EN_GPIO: u8 = 16;

// =============================================================================
// Bare-Metal GPIO Helper
// =============================================================================

/// Direct GPIO register access for bare-metal usage
mod gpio {
    use super::*;

    const GPIO_OUT_REG: u32 = 0x3FF4_4004;
    const GPIO_ENABLE_REG: u32 = 0x3FF4_4020;

    /// Configure a GPIO as output and set its level
    ///
    /// # Safety
    /// Direct hardware register access.
    pub unsafe fn set_output(gpio: u8, high: bool) {
        if gpio >= 32 {
            return; // Only handle GPIO 0-31 for simplicity
        }

        let mask = 1u32 << gpio;

        // Enable output
        let enable = read_volatile(GPIO_ENABLE_REG as *const u32);
        write_volatile(GPIO_ENABLE_REG as *mut u32, enable | mask);

        // Set level
        let out = read_volatile(GPIO_OUT_REG as *const u32);
        if high {
            write_volatile(GPIO_OUT_REG as *mut u32, out | mask);
        } else {
            write_volatile(GPIO_OUT_REG as *mut u32, out & !mask);
        }
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

/// Bare-metal entry point
///
/// Replace this with your actual entry point mechanism.
#[no_mangle]
pub extern "C" fn main() -> ! {
    // Create delay provider
    let mut delay = BusyDelay::new();

    // Enable external clock oscillator (board-specific)
    unsafe {
        gpio::set_output(CLK_EN_GPIO, true);
    }
    delay.delay_ms(10);

    // Get mutable reference to static EMAC
    let emac = unsafe { &mut EMAC };

    // Configure EMAC for external RMII clock
    let config = EmacConfig::new()
        .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(RmiiClockMode::ExternalGpio0);

    // Initialize EMAC
    if let Err(_e) = emac.init(config, &mut delay) {
        // Handle initialization error
        // In bare-metal, you might blink an LED or halt
        loop {
            core::hint::spin_loop();
        }
    }

    // Initialize PHY via MDIO
    let mut mdio = MdioController::new(&mut delay);
    let mut phy = Lan8720a::new(PHY_ADDR);

    if let Err(_e) = phy.init(&mut mdio) {
        // Handle PHY initialization error
        loop {
            core::hint::spin_loop();
        }
    }

    // Wait for link to come up
    let mut link_up = false;
    for _ in 0..100 {
        delay.delay_ms(100);

        if let Ok(Some(status)) = phy.poll_link(&mut mdio) {
            // Link is up - configure MAC to match PHY
            emac.set_speed(status.speed);
            emac.set_duplex(status.duplex);
            link_up = true;
            break;
        }
    }

    if !link_up {
        // Timeout waiting for link
        loop {
            core::hint::spin_loop();
        }
    }

    // Start the EMAC
    if let Err(_e) = emac.start() {
        loop {
            core::hint::spin_loop();
        }
    }

    // Main loop: simple echo server
    let mut rx_buffer = [0u8; 1600];

    loop {
        // Try to receive a frame
        match emac.receive(&mut rx_buffer) {
            Ok(len) if len > 0 => {
                // Got a frame - echo it back (swap MAC addresses)
                let frame = &mut rx_buffer[..len];

                // Swap destination and source MAC addresses
                for i in 0..6 {
                    frame.swap(i, i + 6);
                }

                // Transmit the response
                let _ = emac.transmit(&frame[..len]);
            }
            _ => {
                // No frame available - small delay to reduce polling
                delay.delay_us(100);
            }
        }

        // Periodically check link status
        if let Ok(Some(status)) = phy.poll_link(&mut mdio) {
            // Link changed - update MAC configuration
            emac.set_speed(status.speed);
            emac.set_duplex(status.duplex);
        }
    }
}

// =============================================================================
// Panic Handler (required for no_std)
// =============================================================================

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
