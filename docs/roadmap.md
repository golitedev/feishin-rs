# Roadmap

Status: first native implementation. Synthetic server-to-MPV listening and native rendering work; full UI and feature parity remain open. See [parity](parity.md) for evidence and gaps.

## 0. Repository foundation

- [x] Initialize Git on main.
- [x] Add the MIT license for original contributions.
- [x] Document Rust-only application code and MPV-only playback.
- [x] Add architecture, contributor, provenance, and agent instructions.

## 1. Runnable foundation

- [x] Create a Cargo workspace and select supported Rust/crate versions.
- [x] Open a native egui/eframe window with basic navigation.
- [x] Add typed, credential-free errors, settings paths, and atomic persistence.
- [ ] Add structured diagnostic logging with explicit redaction tests.
- [x] Establish formatting, linting, and focused test commands.

## 2. MPV playback

- [x] Configure MPV executable and manage an application-owned process.
- [x] Implement JSON IPC requests, replies, and property subscriptions.
- [x] Support load, play/pause, seek, volume, and stop.
- [x] Implement queue transitions and tagged end-of-track handling.
- [x] Handle unavailable MPV, failed loads, timeouts, disconnects, and shutdown.

## 3. Server-backed listening

- [x] Implement server adapters using public API documentation.
- [x] Add authentication, normalized library models, browsing, search, and artwork.
- [x] Play authenticated synthetic server streams through real MPV.
- [x] Implement Navidrome/OpenSubsonic and Jellyfin adapters with protocol tests.
- [ ] Validate against a real user-operated server and audio device.

## 4. Daily-use features

- [x] Basic playlist create/delete/add/remove, favorites, sorting/filtering, and queue editing.
- [x] Plaintext lyrics and listening/scrobble behavior with protocol tests.
- [x] Queue/preferences restoration, basic keyboard shortcuts, and Linux MPRIS controls.
- [x] Document feature differences and server capability limitations.
- [ ] Complete visual/interaction parity and advanced features listed in parity.md.

## 5. Distribution

- [ ] Package and validate the Linux desktop application first.
- [ ] Validate Windows and macOS support and their MPV IPC transports.
- [x] Document MPV requirements and font/dependency provenance.
- [ ] Generate and bundle complete third-party notices for release binaries.
- [ ] Add release automation and installation documentation.

Mark work complete only after it exists and has appropriate verification. Feature parity with Feishin is a long-term direction, not a current claim.
