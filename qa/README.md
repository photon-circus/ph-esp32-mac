# ESP32 Hardware QA

This directory documents the machine-local lab configuration used by the
release-validation firmware and Rust host controller. The controller is
designed to fail closed and retains raw evidence for both passing and failed
runs.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Lab Configuration](#lab-configuration)
- [Commands](#commands)
- [Synchronization and Trust Rules](#synchronization-and-trust-rules)
- [Evidence](#evidence)
- [Reset Matrix](#reset-matrix)

---

## Prerequisites

Hardware execution currently targets Windows and requires:

- the `esp` Rust toolchain with the `xtensa-esp32-none-elf` target;
- `espflash`;
- Npcap with a capture-capable Ethernet adapter and its SDK import library
  (`wpcap.lib`; set `LIBPCAP_LIBDIR` when it is not on the linker path);
- a serial connection to the ESP32;
- executable relay and link-control adapters.

Default host builds and parser tests do not require Npcap. The native packet
and serial adapters are compiled only for hardware commands.

---

## Lab Configuration

Copy [`lab.example.toml`](lab.example.toml) to the ignored `qa/lab.toml` and
replace each lab-specific value. The four required actions are `power_off`,
`power_on`, `link_down`, and `link_up`.

Commands are TOML arrays containing an executable followed by literal
arguments:

```toml
power_off = ["C:\\lab\\relayctl.exe", "--channel", "1", "off"]
```

The controller never passes these values through a command shell. Relative
executable paths are resolved from the directory containing `lab.toml`.

The embassy recovery fixture uses the documentation subnet: the host sends
from `192.0.2.1:42425` to the DUT at `192.0.2.2:42424`. The lab segment must
also provide an isolated DHCP server for the separate DHCP gate; the host
captures the exchange but does not synthesize DHCP replies.

---

## Commands

Build one locked firmware suite without accessing lab hardware:

```bash
cargo xtask qa build --suite mdio
```

Build, flash, stimulate, and validate one suite:

```bash
cargo xtask qa run --suite mac-filter --lab qa/lab.toml
```

Run the reset campaign:

```bash
cargo xtask qa matrix --lab qa/lab.toml --cold 20 --warm 20
```

The supported suite names are `mdio`, `reset`, `mac-filter`, `rx-sync`,
`rx-async`, and `rx-embassy`.

Release-grade execution requires a clean worktree. `--allow-dirty` permits
candidate-branch experimentation but permanently marks the resulting bundle
as sandbox evidence.

---

## Synchronization and Trust Rules

Firmware emits allocation-free `PHQA|1|...` records. The host performs an
external action only after receiving the corresponding `READY` record. It
rejects malformed records, unexpected actions, stale commit or run
identifiers, duplicate or missing test IDs, required `SKIP` or `BLOCKED`
outcomes, a panic or reset during a run, a timeout, and a missing or
contradictory `RUN_END`.

Raw Ethernet stimulus uses experimental EtherType `0x88b5` and carries the
suite, action, sequence, READY step, and compiled run identifier. Firmware
must reject traffic from another run. Packet capture independently records
the stimulus so a firmware result cannot pass when the host sent no matching
traffic.

---

## Evidence

Each invocation creates an ignored directory under:

```text
target/qa-evidence/<UTC>-<short-sha>/
```

The bundle includes the exact serial bytes, packet capture, normalized JSON,
JUnit XML, Markdown summary, source and lab metadata, build transcript,
flashed firmware and hashes, relevant `Cargo.lock` hashes, tool versions,
reset reasons, external-action records, and `SHA256SUMS`.

An `INCOMPLETE` marker remains if finalization fails. Raw evidence belongs in
CI artifacts or controlled lab storage; only approved summaries and
checksums should be referenced from a release checklist.

---

## Reset Matrix

Cold iterations use the configured relay and require a complete suite after
every power-on. Warm iterations use a separately built firmware image with an
iteration-tagged RTC marker. Its first boot performs
`esp_hal::system::software_reset()` before `RUN_START`; its second boot must
report the matching run ID and a core-software reset reason before the suite
can be accepted.

Lifecycle observations do not prove cross-core RCC atomicity. The reset suite
must report the required `reset.atomicity` test as `BLOCKED` until the
protected clock/reset implementation and its scripted-MMIO regression test
land together.
