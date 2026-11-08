# Validation record

Date: 2026-11-08. Environment: remote Ubuntu x86_64, Rust 1.98.1, Cargo 1.98.1. The workspace declares Rust 1.95 as its minimum; that older toolchain has not been separately tested.

## Automated checks

- `cargo build --locked -p feishin-rs`: passed.
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Workspace tests plus the final UI regression: 47 passing default tests across core, persistence, application state, HTTP protocols, MPV protocols/lifecycle, desktop metadata, and UI behavior/layout.
- Three explicit real-MPV integration tests: passed with MPV 0.41.0, null audio output, synthetic WAV media, and a loopback HTTP fixture. Includes paused startup, controls, EOF versus stop/error, authenticated headers, and clearing headers on the next file.
- One explicit server-to-MPV listening integration test: passed. Verifies token/salt authentication, album/queue/stream loading, two tracks completing, scrobbling, and a stale Stop event that must not skip the current item.
- One explicit MPRIS integration test inside `dbus-run-session`: passed. Exercises metadata, properties, signals, and desktop commands.
- Relative documentation links, pinned Poppins font hashes, and `git diff --check`: passed.

The five environment-dependent integration tests are ignored by the default workspace run and were executed explicitly. See README.md for commands.

## Native rendering

Captured and inspected the actual eframe window under Xvfb at 1440 × 900 using Mesa software rendering. Views inspected include Home, album detail, Settings, fullscreen now-playing, and Queue. Fixed playback-column alignment, missing font glyphs in hero buttons, oversized slider handles, and a fullscreen queue inheriting a horizontal layout.

The fullscreen queue regression asserts that successive track titles share a column and increase vertically. Reintroducing the horizontal layout makes this test fail; the corrected layout passes. Headless UI tests also cover empty/populated views at supported widths, route identity on Back, bounded artwork requests, and seek release behavior.

Screenshots are local development artifacts under ignored `local/`; they are not reference assets imported from upstream or release artifacts.

## Limits

No private live server, real sound device, hardware GPU, desktop shell media overlay, macOS runtime, or Windows runtime has been validated. The tests establish the implemented synthetic workflows, not full Feishin feature parity or pixel identity. Current omissions are listed in [parity.md](parity.md).
