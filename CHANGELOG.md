# Changelog

This document tracks notable changes to `ph-esp32-mac` using semantic versioning.

---

## Table of Contents

1. [Unreleased](#unreleased)
2. [0.1.1](#011)
3. [0.1.0](#010)

---

## Unreleased

- Flow control enablement now updates runtime config and re-applies correctly when peer PAUSE capability changes.
- Runtime config stays in sync when updating MAC address or promiscuous mode.
- MAC filter slot counting uses the shared `MAC_FILTER_SLOTS` constant.
- Added RMII clock builder helpers for external/internal clock selection.
- Added esp-hal builder convenience wrappers for RMII external/internal clock selection.
- Removed unused duplicate driver implementation (`src/driver/mac.rs`).
- Embassy-net example: optional promiscuous mode toggle for DHCP, auto-disabled after lease, documented in examples README.

---

## 0.1.1

- Fix docs.rs build configuration.

---

## 0.1.0

Baseline release. Changelog entries begin after 0.1.0.
