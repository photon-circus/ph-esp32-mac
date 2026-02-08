# Contributing

Contributions are welcome and encouraged. In particular, **hardware correctness**
is a priority for this project—PHY behavior, DMA edge cases, and register-level
details benefit greatly from real hardware validation.

## Ways to Contribute

- **Hardware validation**: test on ESP32 + LAN8720A boards and report behavior.
- **Bug fixes**: especially for timing, DMA, and PHY bring-up.
- **Docs**: improve clarity around bring-up, wiring, and troubleshooting.
- **Tests**: add or expand unit and integration tests.

## Development Notes

- This crate is **no_std** and **no_alloc** in production code.
- Follow the documentation standards in `docs/DOCUMENTATION_STANDARDS.md`.
- Keep unsafe code minimal and fully documented with `# Safety` sections.

## Recommended Checks

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo doc --no-deps
```

## Hardware Validation (Preferred)

If you can test on real hardware, please include:

- Board + PHY (e.g., WT32-ETH01 + LAN8720A)
- Link speed/duplex and clock source
- Any required GPIO wiring or oscillator notes
- Logs (boot + link + DHCP or smoltcp traffic)

## Reporting Issues

Include:

- Target toolchain (`rustc -V`, `esp-hal` version)
- Features enabled
- Exact steps to reproduce
- Logs and expected vs actual behavior

Thanks for helping improve the driver.
