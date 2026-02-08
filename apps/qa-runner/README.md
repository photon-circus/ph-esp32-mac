# QA Runner

This directory contains the QA runner for the `ph-esp32-mac` driver
running on real ESP32 hardware. It is a **standalone crate** and is not
packaged with the published library crate.

## Project Structure

```
apps/qa-runner/
├── Cargo.toml           # Standalone crate configuration
├── rust-toolchain.toml  # ESP32 toolchain selector
├── qa_runner.rs         # WT32-ETH01 QA runner binary
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

## WT32-ETH01 QA Target

The primary QA target is the WT32-ETH01 board, a compact and
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

### Recommended: xtask Runner

From the repo root:

```bash
# Build, flash, and monitor
cargo xtask run qa-runner

# Build only (no flash)
cargo xtask build qa-runner
```

Add `--debug` if you want a debug build:

```bash
cargo xtask run qa-runner --debug
```

The runner uses the ESP toolchain (`rustup run esp`) and configures the
target/runner/rustflags needed for the WT32-ETH01 QA binary. You can pass
the short target name (recommended) or a `.rs` entry path.

You can set `ESPFLASH_PORT` and `ESPFLASH_BAUD` as environment variables
if you prefer not to pass them on the command line.

### Manual Cargo Commands (Optional)

If you prefer direct cargo commands, follow the exact command printed by xtask
(it includes the required target, build-std, runner, and QA-specific rustflags).

### Expected Output

Example output (counts may change as tests evolve):

```
╔══════════════════════════════════════════════════════════════╗
║       WT32-ETH01 Integration Test Suite                      ║
║       ph-esp32-mac Driver Verification                       ║
╚══════════════════════════════════════════════════════════════╝

Enabling external 50MHz oscillator...
Oscillator enabled

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  GROUP 1: Register Access
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

▶ EMAC clock enable
  DPORT WIFI_CLK_EN=0x..., EMAC_EN=1
  ✓ PASS
...

══════════════════════════════════════════════════════════════════
  TEST SUMMARY
══════════════════════════════════════════════════════════════════

  Total:   47
  Passed:  47 ✓
  Failed:  0 ✗
  Skipped: 0 ○

╔══════════════════════════════════════════════════════════════╗
║                    ALL TESTS PASSED! ✓                       ║
╚══════════════════════════════════════════════════════════════╝

Entering continuous RX monitoring mode...
```

### Test Groups

The test suite is organized into 9 groups:

| Group | Description |
|-------|-----------|
| 1. Register Access | EMAC clock, DMA/MAC/extension registers |
| 2. EMAC Initialization | Init, RMII pins, DMA descriptors |
| 3. PHY Communication | MDIO read, PHY init, link detection |
| 4. EMAC Operations | Start, TX, RX (3s), stop/start |
| 5. Link Status | Link status query |
| 6. smoltcp Integration | Interface, capabilities, poll |
| 7. State/Interrupts/Utils | State, interrupts, TX/RX utilities |
| 8. Advanced Features | Promiscuous, force link, PHY caps, int enable |
| 9. Edge Cases | MAC/hash/VLAN filtering, flow control, EDPD, cleanup |

Tests in later groups may be skipped if dependencies in earlier groups fail
(e.g., EMAC operations require successful initialization and link).

### What the Tests Verify

1. **Register Access** - Verifies EMAC peripheral clock and register accessibility
2. **EMAC Initialization** - Tests driver initialization and hardware configuration
3. **PHY Communication** - Validates MDIO bus and LAN8720A PHY functionality
4. **EMAC Operations** - End-to-end packet TX/RX verification
5. **Link Status** - PHY link monitoring capability
6. **smoltcp Integration** - Network stack Device trait implementation
7. **State/Interrupts/Utils** - State machine, interrupt handling, TX ready, RX peek, frame sizes
8. **Advanced Features** - Promiscuous mode, forced link, PHY capabilities, interrupt enable/disable
9. **Edge Cases** - MAC/hash/VLAN filtering, flow control, PHY energy detect, async API

After tests complete, the binary enters a continuous RX monitoring mode that
logs all received packets for debugging.

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
use smoltcp::iface::{Config, Interface};
use smoltcp::time::Instant as SmolInstant;
use smoltcp::wire::EthernetAddress;

let hw_addr = EthernetAddress(*emac.mac_address());
let config = Config::new(hw_addr.into());

// Emac implements smoltcp::phy::Device directly
let mut iface = Interface::new(config, &mut emac, SmolInstant::from_millis(0));
```

## Board Support (Driver Crate)

Board scaffolding is provided by the main driver crate:

```rust
use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01;

// PHY instance for WT32-ETH01
let phy = Wt32Eth01::lan8720a();

// Board-specific EMAC configuration
let config = Wt32Eth01::emac_config_with_mac(my_mac);
```

## Adding Support for Other Boards

To add support for another ESP32+Ethernet board:

1. Add a board helper under `src/boards/` in the main crate
2. Update documentation and examples to use the new helper
3. Update this QA runner to use the new board helper

### Known Compatible Boards

| Board | PHY | Addr | Clock | Notes |
|-------|-----|------|-------|-------|
| WT32-ETH01 | LAN8720A | 1 | Ext 50MHz GPIO0 | GPIO16 enables osc |
| Olimex ESP32-POE | LAN8720A | 0 | Ext 50MHz GPIO0 | Has PoE option |
| wESP32 | LAN8720A | 0 | Ext 50MHz GPIO0 | Industrial grade |

## License

Licensed under MIT OR Apache-2.0, same as the main crate.
