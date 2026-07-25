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
- [Manual Lab Workflow](#manual-lab-workflow)
- [Reset Matrix](#reset-matrix)

---

## Prerequisites

Hardware execution currently targets Windows and requires:

- the `esp` Rust toolchain with the `xtensa-esp32-none-elf` target;
- `espflash`;
- Npcap with a capture-capable Ethernet adapter and its SDK import library
  (`wpcap.lib`; set `LIBPCAP_LIBDIR` when it is not on the linker path);
- a bidirectional serial connection to the ESP32 (GPIO1 TX and GPIO3 RX);
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
the stimulus and requires the exact destination and sequence range, so a
firmware result cannot pass when the host sent missing, duplicated, mislabeled,
or misaddressed traffic.

For RX flood tests the host sends exactly 4096 frames over two seconds, waits
100 ms for the NIC transmit path to drain, and writes an exact run/step-bound
acknowledgement over serial. Firmware keeps the ring exhausted and counts
interrupts until that acknowledgement arrives. The host also rejects captures
whose measured injection pacing is outside 1900–2250 ms.

The embassy stack and EMAC driver are constructed before starvation, while the
network runner is deliberately not spawned. The same runner is then spawned to
recycle the exhausted ring and service the fixed-IP UDP challenge—there is no
synchronous drain or EMAC reinitialization between exhaustion and recovery.
Both firmware recovery and host-observed challenge-to-echo timing are bounded
to two seconds. Before DHCP starts, the host arms a fresh capture window and
acknowledges the matching READY record; acceptance requires a BOOTP-valid
client request and server ACK with the same transaction ID, DUT MAC, and
nonzero leased address.

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

## Manual Lab Workflow

`.github/workflows/hardware-qa.yml` is prepared with only a
`workflow_dispatch` trigger. It requires a full candidate commit SHA, a
protected `esp32-lab` environment approval, an approved board label, and a lab
configuration path supplied by the environment-level `QA_LAB_CONFIG`
configuration variable. That variable is loaded by the first execution step,
normalized, and required to resolve outside the checkout. The workflow verifies
that the selected SHA is the exact checkout and belongs to
`origin/v0.1.2-candidate` before executing candidate code on the persistent
runner.

GitHub exposes `workflow_dispatch` only when the workflow file exists on the
default branch. Candidate development therefore continues with the local
commands above. Activating the workflow requires a later, explicitly approved
tooling-only merge to `main`; the workflow then checks out the selected
candidate SHA rather than running arbitrary pull-request refs.

The repository environment must require reviewers, and the runner must carry
the labels `self-hosted`, `esp32`, `wt32-eth01`, and its board-specific label.
Use a dedicated Actions Runner 2.329.0 or later with no reusable repository or
cloud credentials and no route to production networks. The workflow pins its
checkout and artifact actions to reviewed immutable commits.
After candidate identity succeeds, evidence upload runs even when a suite
fails so its raw artifacts are retained. Missing evidence fails the workflow.

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
