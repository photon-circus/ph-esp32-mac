# Release Checklist

This checklist defines the steps required to publish `ph-esp32-mac` to crates.io.

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

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --lib`
- [ ] `cargo doc --no-deps`
- [ ] Target toolchain build for ESP32 examples (documented in `apps/examples/README.md`).

---

## Packaging

- [ ] `cargo package --list` contains only intended files.
- [ ] `cargo publish --dry-run` passes with no warnings.

---

## Publish

- [ ] `cargo publish`
- [ ] Tag the release in git.

---

## Post-release

- [ ] Update README badges (docs.rs, crates.io).
- [ ] Announce release notes or link to changelog.
