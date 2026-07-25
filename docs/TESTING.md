# Testing

This document defines the reproducible host, Xtensa, and hardware-validation
layers used for the v0.1.2 candidate. Hardware correctness remains the
highest-risk area and requires evidence from the exact candidate commit.

---

## Table of Contents

- [Host Checks](#host-checks)
- [Dependency Checks](#dependency-checks)
- [ESP32 Build Checks](#esp32-build-checks)
- [Hardware Validation](#hardware-validation)
- [CI and docs.rs](#ci-and-docsrs)
- [Coverage and Remaining Gaps](#coverage-and-remaining-gaps)

---

## Host Checks

All dependency-resolving commands use committed lockfiles:

```bash
cargo test --locked --lib
cargo test --locked --lib \
  --features "smoltcp,async,critical-section,embassy-net"
cargo clippy --locked --lib --tests -- -D warnings
cargo clippy --locked --lib --tests \
  --features "smoltcp,async,critical-section,embassy-net" -- -D warnings
cargo test --locked --manifest-path xtask/Cargo.toml
cargo test --locked --manifest-path tools/qa-host/Cargo.toml
```

Formatting is checked separately for the root, examples, QA firmware, xtask,
and QA host manifests. Optional coverage uses the same feature graph:

```bash
cargo llvm-cov --locked \
  --features "smoltcp,async,critical-section,embassy-net" \
  --lcov --output-path lcov.info
```

---

## Dependency Checks

Run the shared `deny.toml` policy against every independently locked graph:

```bash
cargo deny --manifest-path Cargo.toml --all-features --locked check
cargo deny --manifest-path apps/examples/Cargo.toml --all-features --locked check
cargo deny --manifest-path apps/qa-runner/Cargo.toml --all-features --locked check
cargo deny --manifest-path xtask/Cargo.toml --all-features --locked check
cargo deny --manifest-path tools/qa-host/Cargo.toml --all-features --locked check
```

Warnings about duplicate transitive versions require review, but advisories,
bans, licenses, and sources must pass for every graph.

---

## ESP32 Build Checks

Use the ESP Rust toolchain and the real Xtensa target:

```bash
cargo +esp check --locked --lib \
  --target xtensa-esp32-none-elf -Zbuild-std=core

cargo +esp check --locked --lib \
  --target xtensa-esp32-none-elf -Zbuild-std=core \
  --no-default-features --features "esp32,esp-hal"

cargo +esp check --locked --lib \
  --target xtensa-esp32-none-elf -Zbuild-std=core \
  --no-default-features \
  --features "esp32,esp-hal,smoltcp,critical-section,async,embassy-net,log,defmt"
```

CI also builds every example and every QA firmware binary in release mode with
the Xtensa linker configuration. ESP32-P4 is experimental and is never
accepted as substitute evidence.

---

## Hardware Validation

The exploratory `qa-smoke` firmware remains available, but release evidence
comes from the six finite suites driven by the Rust host controller:

```bash
cargo xtask qa build --suite mdio
cargo xtask qa run --suite mdio --lab qa/lab.toml
cargo xtask qa run --suite mac-filter --lab qa/lab.toml
cargo xtask qa run --suite rx-sync --lab qa/lab.toml
cargo xtask qa run --suite rx-async --lab qa/lab.toml
cargo xtask qa run --suite rx-embassy --lab qa/lab.toml
cargo xtask qa matrix --lab qa/lab.toml --cold 20 --warm 20
```

Release-grade runs require a clean tree. Evidence is stored below
`target/qa-evidence/<UTC>-<short-sha>/` and includes raw serial bytes, pcap,
normalized results, JUnit, exact firmware and lockfile hashes, tool versions,
external actions, reset reasons, and `SHA256SUMS`.

See [qa/README.md](../qa/README.md) for lab configuration, synchronization,
Npcap, relay controls, and fail-closed rules.

---

## CI and docs.rs

Normal CI runs only on GitHub-hosted runners. The lab workflow is
`workflow_dispatch`-only, uses the protected `esp32-lab` environment, and
checks out an exact commit that must belong to `v0.1.2-candidate`. After that
identity check succeeds, it uploads evidence even when a suite fails. It must
never be enabled for automatic pull-request execution.

GitHub exposes a manual workflow only after the workflow file exists on the
default branch. Until an explicitly approved tooling-only merge activates it,
candidate testing uses the identical local `cargo xtask qa ...` commands.

docs.rs accepts targets supported through rustup; the ESP Xtensa compiler is a
separate toolchain. Package metadata therefore renders target-independent API
documentation on `x86_64-unknown-linux-gnu`, while CI independently runs the
metadata feature set plus the ESP32 and esp-hal facade features through
rustdoc on `xtensa-esp32-none-elf`. Both gates must pass.

---

## Coverage and Remaining Gaps

Covered by host tests:

- configuration, error handling, protocol parsing, and evidence generation;
- PHY behavior through MDIO mocks;
- DMA ring behavior and private ordering seams;
- strict READY/action identity, packet sequence sets, UDP echo timing, and
  DHCP transaction correlation.

Covered only by hardware evidence:

- actual register access, DMA ownership, ISR behavior, PHY/link behavior, and
  relay-controlled reset lifecycle;
- non-promiscuous MAC filtering and network-stack recovery.

Hardware lifecycle results do not prove cross-core RCC atomicity. The
`reset.atomicity` release test remains required and `BLOCKED` until the
protected reset implementation and scripted-MMIO concurrent-writer regression
land together.
