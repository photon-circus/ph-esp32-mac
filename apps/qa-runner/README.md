# QA Runner

Hardware QA runner for the `ph-esp32-mac` driver on real ESP32 devices. This is
a standalone crate used for verification and is not published to crates.io.

---

## Table of Contents

- [Overview](#overview)
- [Prerequisites](#prerequisites)
- [Running With `cargo xtask`](#running-with-cargo-xtask)
- [Hardware: WT32-ETH01](#hardware-wt32-eth01)
- [Test Suite](#test-suite)
- [Troubleshooting](#troubleshooting)
- [Board Support](#board-support)
- [License](#license)

---

## Overview

The QA runner validates the EMAC driver against real hardware. Tests are grouped
into a fixed sequence; later groups are skipped if earlier prerequisites fail.

---

## Prerequisites

Install the ESP toolchain and flash tool:

```bash
cargo install espup
espup install

cargo install espflash
```

---

## Running With `cargo xtask`

Run from the repo root. `xtask` selects the right crate, target, and features,
injects the required linker flags, and invokes the ESP toolchain.

```bash
cargo xtask run qa-runner
```

Build only:

```bash
cargo xtask build qa-runner
```

Debug build:

```bash
cargo xtask run qa-runner --debug
```

Environment overrides:

```bash
$env:ESPFLASH_PORT = "COM7"
$env:ESPFLASH_BAUD = "921600"
```

---

## Hardware: WT32-ETH01

The default QA target is the WT32-ETH01 board.

| Component | Details |
|-----------|---------|
| MCU | ESP32-D0WD-V3 (WT32-S1 module) |
| Flash | 4MB |
| PHY | LAN8720A (RMII) |
| PHY Address | 1 |
| Clock | External 50 MHz oscillator (GPIO16 enable) |
| Power | 5V (onboard regulator) or 3.3V direct |

### Wiring for Programming

| USB-TTL | WT32-ETH01 | Notes |
|---------|------------|-------|
| 3.3V    | 3V3        | Use 5V only if regulator needed |
| GND     | GND        | |
| TX      | IO3 (RXD)  | USB TX → ESP RX |
| RX      | IO1 (TXD)  | USB RX ← ESP TX |

Bootloader mode:
1. Connect IO0 to GND
2. Reset or power-cycle
3. Release IO0 after flashing starts

---

## Test Suite

### Groups

| Group | ID Range | Category |
|-------|----------|----------|
| 1 | IT-1-xxx | Register Access |
| 2 | IT-2-xxx | EMAC Initialization |
| 3 | IT-3-xxx | PHY Communication |
| 4 | IT-4-xxx | EMAC Operations |
| 5 | IT-5-xxx | Link Status |
| 6 | IT-6-xxx | smoltcp Integration |
| 7 | IT-7-xxx | State & Interrupts |
| 8 | IT-8-xxx | Advanced Features |
| 9 | IT-9-xxx | Edge Cases |

### Expected Output

```text
╔══════════════════════════════════════════════════════════════╗
║       WT32-ETH01 Integration Test Suite                      ║
║       ph-esp32-mac Driver Verification                       ║
╚══════════════════════════════════════════════════════════════╝

...
══════════════════════════════════════════════════════════════════
  TEST SUMMARY
══════════════════════════════════════════════════════════════════

  Total:   47
  Passed:  47 ✓
  Failed:  0 ✗
  Skipped: 0 ○
```

After tests complete, the runner enters continuous RX monitoring mode and logs
received frames.

---

## Troubleshooting

### Timeout waiting for link

- Check the Ethernet cable and link partner
- Confirm the oscillator enable (GPIO16 HIGH)
- Power-cycle the board

### PHY init failed

- Verify MDIO wiring (GPIO18=MDIO, GPIO23=MDC)
- Confirm the LAN8720A address (default: 1)
- Re-check the external 50 MHz clock

---

## Board Support

The QA runner uses board helpers from the driver crate:

```rust
use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01;

let phy = Wt32Eth01::lan8720a();
let config = Wt32Eth01::emac_config_with_mac(my_mac);
```

To add another board:
1. Add a board helper under `src/boards/` in the driver crate
2. Update the QA runner to use it
3. Document the wiring and PHY details here

---

## License

Licensed under Apache-2.0. See [LICENSE](../../LICENSE).
