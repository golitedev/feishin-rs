# Roadmap

Status: repository bootstrap. No player or application implementation exists.

## 0. Repository foundation

- [x] Initialize Git on main.
- [x] Add the MIT license for original contributions.
- [x] Document Rust-only application code and MPV-only playback.
- [x] Add architecture, contributor, provenance, and agent instructions.

## 1. Runnable foundation

- [ ] Create a Cargo workspace and select supported Rust/crate versions.
- [ ] Open a native egui/eframe window with basic navigation.
- [ ] Add structured errors, redacted logging, settings paths, and persistence.
- [ ] Establish formatting, linting, and focused test commands.

## 2. MPV playback

- [ ] Discover/configure MPV and manage an application-owned process.
- [ ] Implement JSON IPC requests, replies, and property subscriptions.
- [ ] Support load, play/pause, seek, volume, and stop.
- [ ] Implement queue transitions and end-of-track handling.
- [ ] Handle unavailable MPV, failed loads, timeouts, disconnects, and shutdown.

## 3. Server-backed listening

- [ ] Choose and implement the first server adapter using public API documentation.
- [ ] Add authentication, normalized library models, browsing, search, and artwork.
- [ ] Play authenticated server streams through MPV.
- [ ] Add the remaining Navidrome, Jellyfin, and OpenSubsonic adapters.

## 4. Daily-use features

- [ ] Playlists, favorites, sorting/filtering, and queue editing.
- [ ] Lyrics and listening/scrobble behavior where supported.
- [ ] Session restoration, keyboard shortcuts, and desktop media controls.
- [ ] Document feature differences and server capability limitations.

## 5. Distribution

- [ ] Package and validate the Linux desktop application first.
- [ ] Validate Windows and macOS support and their MPV IPC transports.
- [ ] Document MPV installation requirements and third-party notices.
- [ ] Add release automation and installation documentation.

Mark work complete only after it exists and has appropriate verification. Feature parity with Feishin is a long-term direction, not a current claim.
