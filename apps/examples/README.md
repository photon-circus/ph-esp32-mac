# Examples

This directory contains example applications demonstrating different ways to use the `ph-esp32-mac` driver.

## Example Overview

| Example | Description | Features Required |
|---------|-------------|-------------------|
| [esp_hal_integration.rs](esp_hal_integration.rs) | Integration with esp-hal ecosystem | `esp32`, `esp-hal`, `critical-section` |
| [esp_hal_async.rs](esp_hal_async.rs) | Async RX with per-instance wakers | `esp32`, `esp-hal`, `async`, `critical-section` |
| [smoltcp_echo.rs](smoltcp_echo.rs) | TCP/IP networking with smoltcp | `esp32`, `smoltcp`, `critical-section` |
| [embassy_net.rs](embassy_net.rs) | Async TCP/IP networking with embassy-net | `esp32`, `embassy-net`, `esp-hal`, `critical-section` |

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

### Recommended: xtask Runner

Run examples from the repo root using the xtask helper:

```bash
cargo xtask run ex-esp-hal
cargo xtask run ex-esp-hal-async
cargo xtask run ex-smoltcp
cargo xtask run ex-embassy
```

Build only (no flash) if you want to inspect artifacts:

```bash
cargo xtask build ex-esp-hal
```

Add `--debug` for a debug build:

```bash
cargo xtask run ex-esp-hal --debug
```

The runner discovers the correct crate, bin name, and feature flags from the
`Cargo.toml`. It uses the ESP toolchain (`rustup run esp`) and sets the
ESP32 target/runner automatically. You can pass a short target name
(recommended) or a `.rs` entry path.

You can set `ESPFLASH_PORT` and `ESPFLASH_BAUD` as environment variables
if you prefer not to pass them on the command line.

### Manual Cargo Commands (Optional)

If you prefer to run cargo directly, follow the exact command that xtask prints
(it includes the required target, build-std, and feature flags).

### Example Feature Mapping

| Example | examples crate feature |
|---------|-------------------------|
| `esp_hal_integration` | `esp-hal-example` |
| `esp_hal_async` | `esp-hal-async-example` |
| `smoltcp_echo` | `smoltcp-example` |
| `embassy_net` | `embassy-net-example` |

## Example Details

### 1. esp-hal Integration (`esp_hal_integration.rs`)

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
ph-esp32-mac = { version = "0.1", features = ["esp32", "critical-section", "esp-hal"] }
esp-hal = { version = "1.0", features = ["esp32"] }
```

### 2. esp-hal Async RX (`esp_hal_async.rs`)

**Purpose**: Async receive example using per-instance wakers (`AsyncEmacState`) with esp-hal.

**Use Case**:
- Async tasks without embassy-net
- Low-boilerplate async receive
- Interrupt-driven RX wakeups

**Key Points**:
- Uses `AsyncEmacState` and `AsyncEmacExt`
- ISR wiring is one-line with `emac_async_isr!`
- Runs on the esp-rtos embassy executor

**Features**:
```toml
ph-esp32-mac = { version = "0.1", features = ["esp32", "async", "critical-section", "esp-hal"] }
```

### 3. smoltcp TCP Echo (`smoltcp_echo.rs`)

**Purpose**: Full TCP/IP networking with the smoltcp stack.

**Use Case**:
- Network-connected applications
- TCP/UDP socket programming
- IoT devices

**Key Points**:
- Creates a TCP echo server on port 7
- Uses DHCPv4 and logs the assigned address
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
- `RmiiClockMode` - Use `ExternalInput { gpio: 0 }` or `InternalOutput { gpio: 16 }` (or 17)

### 4. embassy-net Async Example (`embassy_net.rs`)

**Purpose**: Demonstrates async networking with embassy-net and the EMAC driver.

**Use Case**:
- Embassy-based async applications
- Non-blocking network stacks
- Integration with esp-hal 1.0.0 + esp-rtos

**Key Points**:
- Uses `embassy-net-driver` integration via `EmbassyEmac`
- Starts the Embassy time driver with `esp-rtos`
- Spawns an async network runner task
- Polls PHY link state periodically and updates the stack

**Features**:
```toml
ph-esp32-mac = { version = "0.1", features = ["esp32", "embassy-net", "esp-hal", "critical-section"] }
```

**Additional Dependencies** (example project):
```toml
embassy-net = { version = "0.7.0", default-features = false, features = ["medium-ethernet", "proto-ipv4", "dhcpv4"] }
embassy-net-driver = "0.2"
embassy-executor = "0.7"
embassy-time = "0.4"
esp-hal = { version = "1.0.0", features = ["esp32"] }
esp-rtos = { version = "0.2.0", features = ["embassy", "esp32"] }
static-cell = "2"
```

**Interrupt Wiring**:
```rust
use esp_hal::interrupt::InterruptHandler;
use ph_esp32_mac::esp_hal::Priority;

#[esp_hal::handler(priority = Priority::Priority1)]
fn emac_handler() {
    EMAC_STATE.handle_interrupt();
}

const EMAC_IRQ: InterruptHandler = emac_handler;
```

**Embassy Time Driver**:
```rust
let timg0 = TimerGroup::new(peripherals.TIMG0);
esp_rtos::start(timg0.timer0);
```

## Memory Usage

With default configuration (10 RX/TX buffers, 1600 bytes each):

| Component | Size |
|-----------|------|
| RX Descriptors | 320 bytes |
| TX Descriptors | 320 bytes |
| RX Buffers | 16,000 bytes |
| TX Buffers | 16,000 bytes |
| **Total** | **~32 KB** |

Adjust buffer counts and sizes if memory is constrained:
```rust
// Smaller configuration
static mut EMAC: Emac<4, 4, 1600> = Emac::new();  // ~13 KB
```

## See Also

- [qa-runner/](../qa-runner/) - Hardware QA runner for WT32-ETH01
- [DESIGN.md](../docs/DESIGN.md) - Architecture documentation
- [TESTING.md](../docs/TESTING.md) - Testing guide
