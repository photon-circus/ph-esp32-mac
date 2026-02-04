# Examples

This directory contains example applications demonstrating different ways to use the `ph-esp32-mac` driver.

## Example Overview

| Example | Description | Features Required |
|---------|-------------|-------------------|
| [bare_metal.rs](bare_metal.rs) | Lowest-level usage without any HAL | `esp32` |
| [esp_hal_integration.rs](esp_hal_integration.rs) | Integration with esp-hal ecosystem | `esp32`, `esp-hal`, `critical-section` |
| [smoltcp_echo.rs](smoltcp_echo.rs) | TCP/IP networking with smoltcp | `esp32`, `smoltcp`, `critical-section` |

## Building Examples

These examples are designed for ESP32 hardware and cannot be run on a host machine.
They require the xtensa toolchain.

### Prerequisites

1. Install Rust xtensa toolchain:
   ```bash
   rustup component add rust-src --toolchain nightly
   cargo install espup
   espup install
   ```

2. Install espflash for flashing:
   ```bash
   cargo install espflash
   ```

### Building

Examples should be built as standalone binaries. Due to target architecture requirements,
it's recommended to copy the example into a standalone project:

```bash
# Create a new ESP32 project
cargo generate esp-rs/esp-template

# Copy the example code and adapt the Cargo.toml
```

Or use the `integration_tests/` directory as a template.

## Example Details

### 1. Bare Metal (`bare_metal.rs`)

**Purpose**: Shows the lowest-level usage pattern without any HAL dependencies.

**Use Case**: 
- Custom `no_std` environments
- When you need maximum control
- Porting to non-standard ESP32 setups

**Key Points**:
- Provides a custom `DelayNs` implementation using busy-wait
- Direct GPIO register access for clock enable
- No dependencies on esp-hal or esp-rs ecosystem

**Features**:
```toml
ph-esp32-mac = { version = "0.1", features = ["esp32"] }
```

### 2. esp-hal Integration (`esp_hal_integration.rs`)

**Purpose**: Demonstrates the recommended way to use the driver with esp-hal.

**Use Case**:
- Standard ESP32 Rust development
- When using other esp-hal peripherals
- Production applications

**Key Points**:
- Uses esp-hal's `Delay` type
- Proper peripheral ownership pattern
- Critical section for interrupt-safe EMAC access
- Logging via esp-println

**Features**:
```toml
ph-esp32-mac = { version = "0.1", features = ["esp32", "critical-section"] }
esp-hal = { version = "1.0", features = ["esp32"] }
```

### 3. smoltcp TCP Echo (`smoltcp_echo.rs`)

**Purpose**: Full TCP/IP networking with the smoltcp stack.

**Use Case**:
- Network-connected applications
- TCP/UDP socket programming
- IoT devices

**Key Points**:
- Creates a TCP echo server on port 7
- Static IP configuration (easily changed to DHCP)
- Handles ARP, ICMP (ping), and TCP
- Shows socket creation and management

**Features**:
```toml
ph-esp32-mac = { version = "0.1", features = ["esp32", "smoltcp", "critical-section"] }
smoltcp = { version = "0.12", features = ["medium-ethernet", "proto-ipv4", "socket-tcp"] }
```

**Testing**:
```bash
# After flashing, test with netcat:
nc 192.168.1.100 7

# Or test ping:
ping 192.168.1.100
```

## Hardware Notes

All examples are configured for the **WT32-ETH01** board by default:

| Parameter | Value |
|-----------|-------|
| PHY | LAN8720A |
| PHY Address | 1 |
| Clock Mode | External 50 MHz oscillator |
| Clock Enable GPIO | 16 |

For other boards, modify the constants at the top of each example:
- `PHY_ADDR` - MDIO address of your PHY
- `CLK_EN_GPIO` - GPIO that enables the clock (if applicable)
- `RmiiClockMode` - Use `ExternalGpio0` or `InternalGpio0` depending on your hardware

## Memory Usage

With default configuration (10 RX/TX buffers, 1600 bytes each):

| Component | Size |
|-----------|------|
| RX Descriptors | 160 bytes |
| TX Descriptors | 160 bytes |
| RX Buffers | 16,000 bytes |
| TX Buffers | 16,000 bytes |
| **Total** | **~32 KB** |

Adjust buffer counts and sizes if memory is constrained:
```rust
// Smaller configuration
static mut EMAC: Emac<4, 4, 1600> = Emac::new();  // ~13 KB
```

## See Also

- [integration_tests/](../integration_tests/) - Full working example with build configuration
- [DESIGN.md](../DESIGN.md) - Architecture documentation
- [TESTING.md](../TESTING.md) - Testing guide
