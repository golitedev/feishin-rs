# Roadmap

Status: 2026-11-09. See [parity](parity.md) for the feature matrix and remaining acceptance work.

## Foundation and native application

- [x] Git on main, MIT original contributions, provenance and agent instructions.
- [x] Rust workspace with egui/eframe, typed domain models and worker boundaries.
- [x] Versioned atomic settings, schema migration, optional OS credentials.
- [x] Bounded structured diagnostics without free-form metadata or secrets.
- [x] Formatting, strict linting, focused unit/protocol/integration tests.

## Server-backed listening

- [x] Navidrome/OpenSubsonic and Jellyfin authentication and normalized libraries.
- [x] Album/artist/track/folder browsing, artwork, search, favorites and ratings.
- [x] Playlist occurrence editing/reordering and metadata; Navidrome smart rules.
- [x] Timed lyrics, downloads, internet radio, stream transcoding controls.
- [x] Disposable real-Navidrome advanced-workflow validation.
- [ ] Live Jellyfin validation and large-library performance measurements.

## MPV playback

- [x] Isolated external process, Unix socket and Windows named-pipe transport.
- [x] Playback controls, tagged queue transitions, shutdown and recoverable errors.
- [x] Prepared successor playback, cancellation recovery, Auto DJ.
- [x] Audio device/output controls, ReplayGain, EQ, normalization, speed and stats.
- [x] Real stereo RMS/peak analysis through MPV.
- [ ] Crossfade and a complete visualizer implementation.
- [ ] Real sound-device session and Windows/macOS runtime validation.

## Interface and desktop

- [x] Current-reference Home/sidebar/settings geometry and Inter typography.
- [x] Native library/detail/queue/fullscreen views and timed lyric controls.
- [x] Multiple profiles, saved filters, themes/custom colors, configurable shortcuts.
- [x] Linux MPRIS and optional desktop notifications.
- [ ] Complete visual/interaction parity, localization and accessibility audit.
- [ ] Remaining desktop integrations, external lyric providers and offline library.

## Distribution

- [x] Rust xtask for checks, pinned dependency notices and verified Linux archive.
- [x] Original desktop icon, desktop entry, font licenses and installation docs.
- [ ] Target-system validation, platform installers, signing and update delivery.

Check an item only when behavior exists and appropriate verification has run. No overall feature-parity or identical-UI claim is made by this checklist.
