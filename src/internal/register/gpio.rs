//! GPIO Matrix Register Definitions
//!
//! ESP32 GPIO Matrix configuration for routing EMAC SMI signals (MDC/MDIO)
//! to the appropriate GPIO pins.
//!
//! Unlike the RMII data pins which have fixed internal routing, the SMI
//! (Station Management Interface) signals must be explicitly routed via
//! the GPIO Matrix.
//!
//! # Signal Routing
//!
//! | Signal | Index | Direction | Default GPIO |
//! |--------|-------|-----------|--------------|
//! | EMAC_MDC_O | 200 | Output | GPIO23 |
//! | EMAC_MDI_I | 201 | Input | GPIO18 |
//! | EMAC_MDO_O | 201 | Output | GPIO18 |

use super::{read_reg, write_reg};

// =============================================================================
// GPIO Base Addresses
// =============================================================================

/// GPIO peripheral base address
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const GPIO_BASE: usize = 0x3FF4_4000;

/// GPIO output enable set register (W1TS)
pub const GPIO_ENABLE_W1TS_OFFSET: usize = 0x24;

/// GPIO output enable clear register (W1TC)
pub const GPIO_ENABLE_W1TC_OFFSET: usize = 0x28;

/// GPIO output set register (W1TS)
pub const GPIO_OUT_W1TS_OFFSET: usize = 0x08;

/// GPIO output clear register (W1TC)
pub const GPIO_OUT_W1TC_OFFSET: usize = 0x0C;

/// GPIO input function configuration register base offset
/// For signal S: GPIO_FUNC_IN_SEL_CFG_REG = GPIO_BASE + 0x130 + (S * 4)
pub const GPIO_FUNC_IN_SEL_CFG_BASE: usize = 0x130;

/// GPIO output function configuration register base offset
/// For GPIO N: GPIO_FUNC_OUT_SEL_CFG_REG = GPIO_BASE + 0x530 + (N * 4)
pub const GPIO_FUNC_OUT_SEL_CFG_BASE: usize = 0x530;

// =============================================================================
// GPIO Matrix Signal Numbers for EMAC
// =============================================================================

/// EMAC MDC output signal index
pub const EMAC_MDC_O_IDX: u32 = 200;

/// EMAC MDIO input signal index
pub const EMAC_MDI_I_IDX: u32 = 201;

/// EMAC MDIO output signal index
pub const EMAC_MDO_O_IDX: u32 = 201;

// =============================================================================
// GPIO_FUNC_OUT_SEL_CFG bit fields
// =============================================================================

/// Function output select field (bits 8:0) - which peripheral signal to output
pub const GPIO_FUNC_OUT_SEL_MASK: u32 = 0x1FF;

/// Output enable select (bit 10) - 0=GPIO, 1=peripheral controls OE
pub const GPIO_OEN_SEL: u32 = 1 << 10;

/// Output invert (bit 9)
pub const GPIO_OUT_INV_SEL: u32 = 1 << 9;

/// Output enable invert (bit 11)
pub const GPIO_OEN_INV_SEL: u32 = 1 << 11;

// =============================================================================
// GPIO_FUNC_IN_SEL_CFG bit fields
// =============================================================================

/// Function input select field (bits 5:0) - which GPIO to use as input
pub const GPIO_FUNC_IN_SEL_MASK: u32 = 0x3F;

/// Input invert (bit 6)
pub const GPIO_IN_INV_SEL: u32 = 1 << 6;

/// Signal input select (bit 7) - 1=route through GPIO Matrix
pub const GPIO_SIG_IN_SEL: u32 = 1 << 7;

// =============================================================================
// IO_MUX Configuration
// =============================================================================

/// IO_MUX base address
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const IO_MUX_BASE: usize = 0x3FF4_9000;

/// IO_MUX register offsets for relevant GPIOs
/// Note: IO_MUX addresses are NOT simply (base + gpio * 4)
/// Each GPIO has a specific offset in the IO_MUX register block
pub const IO_MUX_GPIO18_OFFSET: usize = 0x70; // VSPICLK
pub const IO_MUX_GPIO23_OFFSET: usize = 0x8C; // VSPID

/// IO_MUX function select field (bits 14:12)
pub const IO_MUX_MCU_SEL_SHIFT: u32 = 12;
pub const IO_MUX_MCU_SEL_MASK: u32 = 0x7 << 12;

/// IO_MUX function value for GPIO Matrix routing
pub const IO_MUX_FUNC_GPIO: u32 = 2;

/// IO_MUX input enable (bit 9)
pub const IO_MUX_FUN_IE: u32 = 1 << 9;

/// IO_MUX output enable (bit 8) - for some GPIOs, need to check
pub const IO_MUX_FUN_DRV_SHIFT: u32 = 10;
pub const IO_MUX_FUN_DRV_MASK: u32 = 0x3 << 10;

// =============================================================================
// GPIO Matrix Configuration Functions
// =============================================================================

/// GPIO Matrix configuration for EMAC SMI pins
pub struct GpioMatrix;

impl GpioMatrix {
    /// Configure MDC pin (output only)
    ///
    /// Routes the EMAC_MDC_O signal (index 200) to the specified GPIO pin.
    /// Default: GPIO23
    ///
    /// # Arguments
    /// * `gpio_num` - GPIO number to use for MDC (typically 23)
    ///
    /// # Safety
    /// This function directly manipulates hardware registers.
    pub fn configure_mdc(gpio_num: u8) {
        unsafe {
            // 1. Configure IO_MUX to use GPIO Matrix (function 2)
            let iomux_addr = Self::iomux_addr_for_gpio(gpio_num);
            if iomux_addr != 0 {
                let iomux_val = read_reg(iomux_addr);
                let new_iomux =
                    (iomux_val & !IO_MUX_MCU_SEL_MASK) | (IO_MUX_FUNC_GPIO << IO_MUX_MCU_SEL_SHIFT);
                write_reg(iomux_addr, new_iomux);
            }

            // 2. Enable GPIO output
            write_reg(GPIO_BASE + GPIO_ENABLE_W1TS_OFFSET, 1 << gpio_num);

            // 3. Connect GPIO output to EMAC_MDC_O signal via GPIO Matrix
            let out_sel_addr = GPIO_BASE + GPIO_FUNC_OUT_SEL_CFG_BASE + (gpio_num as usize * 4);
            // OEN_SEL = 1 (peripheral controls output enable)
            let out_sel_val = (EMAC_MDC_O_IDX & GPIO_FUNC_OUT_SEL_MASK) | GPIO_OEN_SEL;
            write_reg(out_sel_addr, out_sel_val);

            #[cfg(feature = "defmt")]
            defmt::debug!(
                "GPIO{} configured as MDC: IOMUX={:#010x} OUT_SEL={:#010x}",
                gpio_num,
                if iomux_addr != 0 {
                    read_reg(iomux_addr)
                } else {
                    0
                },
                out_sel_val
            );
        }
    }

    /// Configure MDIO pin (bidirectional)
    ///
    /// Routes the EMAC_MDI_I input and EMAC_MDO_O output signals (index 201)
    /// to/from the specified GPIO pin. Default: GPIO18
    ///
    /// # Arguments
    /// * `gpio_num` - GPIO number to use for MDIO (typically 18)
    ///
    /// # Safety
    /// This function directly manipulates hardware registers.
    pub fn configure_mdio(gpio_num: u8) {
        unsafe {
            // 1. Configure IO_MUX to use GPIO Matrix (function 2) with input enabled
            let iomux_addr = Self::iomux_addr_for_gpio(gpio_num);
            if iomux_addr != 0 {
                let iomux_val = read_reg(iomux_addr);
                let new_iomux = (iomux_val & !IO_MUX_MCU_SEL_MASK)
                    | (IO_MUX_FUNC_GPIO << IO_MUX_MCU_SEL_SHIFT)
                    | IO_MUX_FUN_IE; // Enable input
                write_reg(iomux_addr, new_iomux);
            }

            // 2. Enable GPIO for both input and output
            // Output enable is controlled by the peripheral via OEN_SEL
            write_reg(GPIO_BASE + GPIO_ENABLE_W1TS_OFFSET, 1 << gpio_num);

            // 3. Connect GPIO output to EMAC_MDO_O signal via GPIO Matrix
            let out_sel_addr = GPIO_BASE + GPIO_FUNC_OUT_SEL_CFG_BASE + (gpio_num as usize * 4);
            // OEN_SEL = 1 (peripheral controls output enable)
            let out_sel_val = (EMAC_MDO_O_IDX & GPIO_FUNC_OUT_SEL_MASK) | GPIO_OEN_SEL;
            write_reg(out_sel_addr, out_sel_val);

            // 4. Connect EMAC_MDI_I signal input to this GPIO
            let in_sel_addr = GPIO_BASE + GPIO_FUNC_IN_SEL_CFG_BASE + (EMAC_MDI_I_IDX as usize * 4);
            // SIG_IN_SEL = 1 (route through GPIO Matrix)
            let in_sel_val = (gpio_num as u32 & GPIO_FUNC_IN_SEL_MASK) | GPIO_SIG_IN_SEL;
            write_reg(in_sel_addr, in_sel_val);

            #[cfg(feature = "defmt")]
            defmt::debug!(
                "GPIO{} configured as MDIO: IOMUX={:#010x} OUT_SEL={:#010x} IN_SEL={:#010x}",
                gpio_num,
                if iomux_addr != 0 {
                    read_reg(iomux_addr)
                } else {
                    0
                },
                out_sel_val,
                in_sel_val
            );
        }
    }

    /// Configure both MDC and MDIO pins with default assignments
    ///
    /// This configures:
    /// - GPIO23 as MDC (SMI clock output)
    /// - GPIO18 as MDIO (SMI data bidirectional)
    ///
    /// This function MUST be called before using the MDIO interface
    /// to communicate with the PHY.
    pub fn configure_smi_pins() {
        Self::configure_mdc(23);
        Self::configure_mdio(18);
    }

    /// Configure SMI pins with custom GPIO assignments
    ///
    /// # Arguments
    /// * `mdc_gpio` - GPIO number for MDC (clock output)
    /// * `mdio_gpio` - GPIO number for MDIO (data bidirectional)
    pub fn configure_smi_pins_custom(mdc_gpio: u8, mdio_gpio: u8) {
        Self::configure_mdc(mdc_gpio);
        Self::configure_mdio(mdio_gpio);
    }

    /// Configure RMII data pins via IO_MUX
    ///
    /// The RMII data pins are FIXED on ESP32 and cannot be remapped via GPIO Matrix.
    /// They must be configured via IO_MUX to function 5 (EMAC function).
    ///
    /// | Signal   | GPIO | Direction |
    /// |----------|------|-----------|
    /// | TXD0     | 19   | Output    |
    /// | TXD1     | 22   | Output    |
    /// | TX_EN    | 21   | Output    |
    /// | RXD0     | 25   | Input     |
    /// | RXD1     | 26   | Input     |
    /// | CRS_DV   | 27   | Input     |
    ///
    /// This function MUST be called during EMAC initialization for packet TX/RX to work.
    pub fn configure_rmii_pins() {
        // EMAC function is function 5 for all RMII pins on ESP32
        const EMAC_FUNC: u32 = 5;

        // TX pins (output)
        Self::configure_iomux_output(19, EMAC_FUNC); // TXD0
        Self::configure_iomux_output(22, EMAC_FUNC); // TXD1
        Self::configure_iomux_output(21, EMAC_FUNC); // TX_EN

        // RX pins (input)
        Self::configure_iomux_input(25, EMAC_FUNC); // RXD0
        Self::configure_iomux_input(26, EMAC_FUNC); // RXD1
        Self::configure_iomux_input(27, EMAC_FUNC); // CRS_DV

        #[cfg(feature = "defmt")]
        defmt::info!("RMII data pins configured via IO_MUX (function 5)");
    }

    /// Configure a GPIO as IO_MUX output for EMAC
    ///
    /// For IO_MUX peripheral functions, we ONLY set the MCU_SEL field.
    /// The peripheral itself controls the output enable - we should NOT
    /// manipulate GPIO_ENABLE registers as that's for GPIO Matrix mode.
    fn configure_iomux_output(gpio_num: u8, func: u32) {
        let iomux_addr = Self::iomux_addr_for_gpio(gpio_num);
        if iomux_addr == 0 {
            return;
        }

        unsafe {
            let current = read_reg(iomux_addr);
            // Set MCU_SEL field to specified function
            // Clear pull-up/pull-down (bits 7, 8)
            // For outputs, we still set FUN_IE=0 (bit 9) since it's output only
            // Also set FUN_DRV (bits 10-11) to maximum drive strength (3)
            let new_val = (current
                & !IO_MUX_MCU_SEL_MASK
                & !(1 << 7)
                & !(1 << 8)
                & !IO_MUX_FUN_IE
                & !(3 << 10))
                | (func << IO_MUX_MCU_SEL_SHIFT)
                | (3 << 10); // Maximum drive strength
            write_reg(iomux_addr, new_val);

            // Disconnect GPIO Matrix output by setting output signal to SIG_GPIO_OUT_IDX (256)
            // This ensures the IO_MUX peripheral function drives the pin, not GPIO Matrix
            let out_sel_addr = GPIO_BASE + GPIO_FUNC_OUT_SEL_CFG_BASE + (gpio_num as usize * 4);
            write_reg(out_sel_addr, 256); // SIG_GPIO_OUT_IDX = 256 means disconnect/bypass
        }
    }

    /// Configure a GPIO as IO_MUX input for EMAC
    ///
    /// For IO_MUX peripheral functions, we set MCU_SEL and enable input.
    /// The GPIO Matrix is bypassed by using the IO_MUX function directly.
    fn configure_iomux_input(gpio_num: u8, func: u32) {
        let iomux_addr = Self::iomux_addr_for_gpio(gpio_num);
        if iomux_addr == 0 {
            return;
        }

        unsafe {
            let current = read_reg(iomux_addr);
            // Set MCU_SEL field to specified function
            // Enable input (bit 9) - critical for receiving data
            // Clear pull-up/pull-down (bits 7, 8) for floating input
            let new_val = (current & !IO_MUX_MCU_SEL_MASK & !(1 << 7) & !(1 << 8))
                | (func << IO_MUX_MCU_SEL_SHIFT)
                | IO_MUX_FUN_IE; // Enable input
            write_reg(iomux_addr, new_val);

            // Disconnect GPIO Matrix output (this pin is input only)
            let out_sel_addr = GPIO_BASE + GPIO_FUNC_OUT_SEL_CFG_BASE + (gpio_num as usize * 4);
            write_reg(out_sel_addr, 256); // SIG_GPIO_OUT_IDX = 256 means disconnect/bypass

            // For IO_MUX inputs, the peripheral reads directly from the pad
            // No GPIO_IN_SEL configuration needed for IO_MUX mode
        }
    }

    /// Get IO_MUX register address for a GPIO
    ///
    /// Returns 0 if GPIO is not supported
    fn iomux_addr_for_gpio(gpio_num: u8) -> usize {
        // IO_MUX offsets are not sequential - each GPIO has a specific offset
        // Based on ESP32 Technical Reference Manual Table 4-3
        #[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
        let offset = match gpio_num {
            0 => 0x44,
            1 => 0x88,
            2 => 0x40,
            3 => 0x84,
            4 => 0x48,
            5 => 0x6C,
            6 => 0x60,
            7 => 0x64,
            8 => 0x68,
            9 => 0x54,
            10 => 0x58,
            11 => 0x5C,
            12 => 0x34,
            13 => 0x38,
            14 => 0x30,
            15 => 0x3C,
            16 => 0x4C,
            17 => 0x50,
            18 => 0x70,
            19 => 0x74,
            20 => 0x78,
            21 => 0x7C,
            22 => 0x80,
            23 => 0x8C,
            25 => 0x24,
            26 => 0x28,
            27 => 0x2C,
            32 => 0x1C,
            33 => 0x20,
            34 => 0x14,
            35 => 0x18,
            36 => 0x04,
            37 => 0x08,
            38 => 0x0C,
            39 => 0x10,
            _ => return 0,
        };

        #[cfg(all(feature = "esp32p4", not(feature = "esp32")))]
        let offset = 0; // ESP32-P4 has different layout, not implemented yet

        if offset == 0 {
            return 0;
        }

        IO_MUX_BASE + offset
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_indices() {
        assert_eq!(EMAC_MDC_O_IDX, 200);
        assert_eq!(EMAC_MDI_I_IDX, 201);
        assert_eq!(EMAC_MDO_O_IDX, 201);
    }

    #[test]
    fn test_gpio_out_sel_address() {
        // GPIO23 output select address should be GPIO_BASE + 0x530 + (23 * 4)
        let addr = GPIO_BASE + GPIO_FUNC_OUT_SEL_CFG_BASE + (23 * 4);
        assert_eq!(addr, 0x3FF4_458C);
    }

    #[test]
    fn test_gpio_in_sel_address() {
        // Signal 201 input select address should be GPIO_BASE + 0x130 + (201 * 4)
        let addr = GPIO_BASE + GPIO_FUNC_IN_SEL_CFG_BASE + (EMAC_MDI_I_IDX as usize * 4);
        // 0x3FF44000 + 0x130 + 0x324 = 0x3FF44454
        assert_eq!(addr, 0x3FF4_4454);
    }

    #[test]
    fn test_iomux_addresses() {
        assert_eq!(GpioMatrix::iomux_addr_for_gpio(18), 0x3FF4_9070);
        assert_eq!(GpioMatrix::iomux_addr_for_gpio(23), 0x3FF4_908C);
    }
}
