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
into two modes:

- `qa-smoke` preserves the exploratory, continuously monitoring firmware;
- `mdio`, `reset`, `mac-filter`, `rx-sync`, `rx-async`, and `rx-embassy` are
  finite release suites controlled by the Rust host harness.

Release suites emit the strict allocation-free `PHQA|1|...` protocol and stop
after `RUN_END`. The RX suites also consume exact run/step-bound host
acknowledgements on the serial RX line after flood completion and before DHCP.
A required skip, block, timeout, missing frame, missing interrupt, stale run
identity, or incomplete terminal record fails the run.

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
cargo xtask qa build --suite rx-sync
cargo xtask qa run --suite rx-async --lab qa/lab.toml
cargo xtask qa matrix --lab qa/lab.toml --cold 20 --warm 20
```

The exploratory runner remains available separately:

```bash
cargo xtask build qa-runner
cargo xtask run qa-runner --debug
```

See [qa/README.md](../../qa/README.md) for the Npcap, serial, relay, and
evidence-bundle configuration.

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

### Release Suites

| Suite | Primary evidence |
|-------|------------------|
| `mdio` | 100 valid register samples, LAN8720A identity, ANAR restore, link transition |
| `reset` | lifecycle/retry/readback, warm resets, cold boots, unicast challenge |
| `mac-filter` | exact MAC/filter registers and tagged unicast/broadcast/multicast rejection |
| `rx-sync` | four-descriptor exhaustion, bounded ISR growth, synchronous recovery |
| `rx-async` | the same exhaustion matrix with causally verified future wakes |
| `rx-embassy` | withheld runner polling, same-driver resume, fixed-IP UDP echo, DHCP |

### Expected Output

```text
PHQA|1|RUN_START|run=...|suite=rx-sync|commit=...|mode=release|reset_reason=power_on
PHQA|1|READY|run=...|step=1|action=fill_rx
PHQA|1|OBS|run=...|name=rx.dma_owned|value=0
PHQA|1|TEST|run=...|id=rx-sync.exhausted|status=PASS|required=1
PHQA|1|RUN_END|run=...|result=PASS|passed=5|failed=0|skipped=0|blocked=0
```

Only `qa-smoke` enters continuous RX monitoring after its tests.

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
