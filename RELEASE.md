# Release Checklist

This checklist defines the steps required to publish `ph-esp32-mac` to crates.io.

For v0.1.2, the version-specific
[release checklist](docs/V0.1.2_RELEASE_CHECKLIST.md) adds the required
hardware, dependency, and remediation gates.

---

## Table of Contents

1. [Preflight](#preflight)
2. [Build and Test](#build-and-test)
3. [Packaging](#packaging)
4. [Publish](#publish)
5. [Post-release](#post-release)

---

## Preflight

- [ ] Confirm version bump and changelog updates.
- [ ] Verify MSRV in `Cargo.toml` and CI matches `1.92.0`.
- [ ] Confirm supported targets (ESP32 only) and feature docs are accurate.

---

## Build and Test

- [ ] Formatting passes for the root, examples, QA firmware, xtask, and QA
  host manifests.
- [ ] `cargo clippy --locked --lib --tests -- -D warnings`
- [ ] `cargo test --locked --lib`
- [ ] `cargo test --locked --manifest-path xtask/Cargo.toml`
- [ ] `cargo test --locked --manifest-path tools/qa-host/Cargo.toml`
- [ ] `cargo doc --locked --no-deps`
- [ ] The standalone esp-hal graph, supported feature aggregate, every example,
  and every QA binary build for `xtensa-esp32-none-elf`.
- [ ] Release-grade hardware evidence is retained for the exact release commit.

---

## Packaging

- [ ] `cargo package --locked --list` contains only intended files.
- [ ] `cargo package --locked` verifies the generated archive.
- [ ] `cargo publish --locked --dry-run` passes with no warnings.

---

## Publish

- [ ] `cargo publish --locked`
- [ ] Tag the release in git.

---

## Post-release

- [ ] Update README badges (docs.rs, crates.io).
- [ ] Announce release notes or link to changelog.
