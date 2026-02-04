# Integration Tests

This directory contains integration tests for the `ph-esp32-mac` driver
running on real ESP32 hardware.

## Project Structure

```
integration_tests/
├── Cargo.toml           # Standalone crate configuration
├── rust-toolchain.toml  # ESP32 toolchain selector
├── .cargo/
│   └── config.toml      # Build target configuration
├── wt32_eth01.rs        # WT32-ETH01 test binary
├── boards/
│   ├── mod.rs           # Board configuration module
│   └── wt32_eth01.rs    # WT32-ETH01 pin mappings
└── README.md            # This file
```

## Prerequisites

### Install ESP32 Rust Toolchain

```bash
# Install espup (ESP32 Rust toolchain manager)
cargo install espup
espup install

# Install flashing tool
cargo install espflash

# Source the environment (or restart terminal)
# On Windows: the installer should configure this automatically
# On Linux/macOS: source $HOME/export-esp.sh
```

### Verify Installation

```bash
# Check that esp toolchain is available
rustup show

# Should list "esp" channel
```

## WT32-ETH01 Example

The primary integration test target is the WT32-ETH01 board, a compact and
affordable ESP32 development board with built-in Ethernet.

### Hardware Requirements

- **WT32-ETH01 board** (~$7-15 USD)
- **USB-TTL adapter** (3.3V TTL, not 5V or RS-232!)
- **Ethernet cable** (connected to a switch/router)
- **Jumper wires** for programming connections

### WT32-ETH01 Specifications

| Component | Details |
|-----------|---------|
| MCU | ESP32-D0WD-V3 (via WT32-S1 module) |
| Flash | 4MB |
| PHY | LAN8720A (RMII interface) |
| PHY Address | **1** (PHYAD0 pulled high) |
| Clock | External 50MHz oscillator (enable via GPIO16) |
| Power | 5V (onboard 3.3V regulator) or 3.3V direct |

### Wiring for Programming

Connect your USB-TTL adapter:

| USB-TTL | WT32-ETH01 | Notes |
|---------|------------|-------|
| 3.3V    | 3V3        | Or use 5V to 5V if regulator needed |
| GND     | GND        | |
| TX      | IO3 (RXD)  | USB TX → ESP RX |
| RX      | IO1 (TXD)  | USB RX ← ESP TX |

**To enter bootloader mode:**
1. Connect IO0 to GND
2. Press reset (or power cycle)
3. Release IO0 after flashing starts

Or use a programmer with auto-boot circuit (like M5Stack ESP32 Downloader).

### Building

Build from the integration_tests directory:

```bash
cd integration_tests

# Build release (recommended for flash size)
cargo build --release

# Or build debug
cargo build
```

Or build from the project root:

```bash
cargo build --manifest-path integration_tests/Cargo.toml --release
```

### Flashing

```bash
cd integration_tests

# Flash and open serial monitor (configured in .cargo/config.toml)
cargo run --release

# Or flash manually
espflash flash target/xtensa-esp32-none-elf/release/wt32_eth01 --monitor

# Or just monitor an already-flashed device
espflash monitor
```

### Expected Output

```
WT32-ETH01 Ethernet Integration Test
=====================================
Enabling external oscillator (GPIO16)...
Oscillator enabled
Initializing EMAC...
EMAC initialized successfully
Initializing LAN8720A PHY at address 1...
PHY initialized successfully
PHY ID: 0x0007C0F1
  -> Confirmed: LAN8720A/LAN8720AI
Waiting for Ethernet link...
Link UP: 100 Mbps Full Duplex

=== INTEGRATION TEST ACTIVE ===
EMAC is running. Testing packet reception...

RX #1: 60 bytes, EtherType=0x0806
  Dst: FF:FF:FF:FF:FF:FF
  Src: AA:BB:CC:DD:EE:FF
  Type: ARP
```

### What the Test Does

1. **Enables the external oscillator** (GPIO16 HIGH)
2. **Initializes EMAC** with proper RMII configuration
3. **Initializes LAN8720A PHY** at address 1
4. **Waits for link** (auto-negotiation)
5. **Configures MAC speed/duplex** based on PHY status
6. **Starts EMAC** for packet reception
7. **Logs all received packets** with header parsing

The test receives and logs all Ethernet frames, showing:
- Destination and source MAC addresses
- EtherType (IPv4, IPv6, ARP, etc.)
- Protocol for IP packets (ICMP, TCP, UDP)

### Troubleshooting

#### "Timeout waiting for link"
- Check Ethernet cable connection
- Verify the other end is connected to an active port
- Try a different cable

#### "PHY init failed"
- Check oscillator is working (GPIO16 should be HIGH)
- Verify MDIO wiring (GPIO18=MDIO, GPIO23=MDC)
- Try power cycling the board

#### "EMAC init failed"
- Usually a clock issue - check GPIO0 is receiving 50MHz
- Verify GPIO16 is correctly enabling the oscillator

#### "Unexpected PHY ID"
- You may have a different board/PHY
- Check if this is really a WT32-ETH01

### Adding Network Stack Integration

To add smoltcp for full IP/TCP/UDP support, see the main library documentation.
The basic pattern is:

```rust
use ph_esp32_mac::smoltcp::EmacDevice;

// Wrap EMAC in smoltcp Device
let device = EmacDevice::new(&mut emac);

// Use with smoltcp Interface
let mut iface = Interface::new(config, &mut device, Instant::now());
```

## Board Configuration Module

The `boards/` directory contains reusable configuration for supported boards:

- `boards/wt32_eth01.rs` - WT32-ETH01 pin mappings and constants

Use the configuration like:

```rust
use boards::wt32_eth01::{Wt32Eth01Config, Wt32Eth01Ext};

// Get PHY address
let phy = Lan8720a::new(Wt32Eth01Config::PHY_ADDR);

// Easy EMAC config
let config = EmacConfig::new()
    .with_mac_address(my_mac)
    .for_wt32_eth01();  // Extension method
```

## Adding Support for Other Boards

To add support for another ESP32+Ethernet board:

1. Create `boards/your_board.rs` with pin mappings
2. Add to `boards/mod.rs`
3. Create an example `your_board.rs` based on the WT32-ETH01 example
4. Update this README

### Known Compatible Boards

| Board | PHY | Addr | Clock | Notes |
|-------|-----|------|-------|-------|
| WT32-ETH01 | LAN8720A | 1 | Ext 50MHz GPIO0 | GPIO16 enables osc |
| Olimex ESP32-POE | LAN8720A | 0 | Ext 50MHz GPIO0 | Has PoE option |
| wESP32 | LAN8720A | 0 | Ext 50MHz GPIO0 | Industrial grade |

## License

Licensed under MIT OR Apache-2.0, same as the main crate.
