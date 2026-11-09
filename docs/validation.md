# Validation record

Date: 2026-11-09. Environment: remote Ubuntu x86_64, Rust/Cargo 1.98.1. The workspace declares Rust 1.95 as its minimum; that older toolchain has not been separately tested.

## Automated checks

- Locked debug and optimized release application builds: passed.
- Workspace formatting and strict Clippy with warnings denied: passed.
- Workspace default tests: 102 passing across domain/settings migration, credential cancellation, application state, HTTP protocols, MPV lifecycle, desktop metadata, UI behavior/layout, and packaging.
- Nine explicit real-MPV tests: passed with MPV 0.41.0, null audio, generated WAV data, and loopback HTTP. Cover playback/settings/devices/stats, pitch and gapless modes, actual stereo levels/silence, prepared transitions/cancellation, authentication-header isolation, and pause fades without changing volume. A new regression exposed successor prefetch forwarding current authorization; disabling URL prefetch fixes the test.
- One explicit server-to-MPV listening test: passed. Covers authenticated streams, two natural completions, scrobbles, and rejecting a stale Stop event.
- One isolated-session MPRIS integration test: passed.
- One explicit disposable Navidrome 0.64.2 integration test: passed with original synthetic FLACs, artwork and lyrics. Checks technical details, numeric ratings, synchronized lyrics, transcoding, downloads, playlist metadata/duplicate reordering, radio CRUD, and native smart-playlist rules. It restores/deletes its mutations.
- Windows GNU-target MPV transport compilation: passed. This is a compile check, not a Windows runtime test.
- Packaging inputs, 347 locked Linux dependency notice inventories, notice hashes, five packaging tests, font provenance, documentation links, and Git whitespace checks: passed.

The twelve environment-dependent checks are ignored by the default workspace run and were executed explicitly. No private account or music library was used. Commands appear in README.md and the adapter documentation.

## Native rendering and interaction

Captured actual eframe windows under Xvfb at 1440 × 900 using Mesa software rendering. Compared the live original Feishin 1.17.0 binary against the Rust app connected to the same disposable library. No upstream implementation/assets were read or copied.

Current Home geometry matches observed key positions: hero y=78–326, genre tiles starting y=418, Explore heading y=568, album artwork y=606–792, and the fixed 90 px player bar starting y=810. Added Inter typography, hero gradient/shadow, album hover playback/queue/favorite/rating controls, sidebar geometry, and an opaque 960 × 810 settings modal. Native input verified the Playback settings tab, extra arguments controls, album hover and context menus.

UI regressions cover settings modal click isolation, fixed player-bar sizing, album hover commands without accidental navigation, configured shortcuts/exact modifiers, Favorites categories, sidebar resize/collapse persistence, vertical fullscreen queue layout, route identity, bounded artwork requests, and seek-release behavior. App regressions cover stale sessions/search/sources, late playlist results, cancelled prepared audio, Auto DJ edits, entity-specific favorites, delayed pause intent, and forgetting a queued credential save.

Screenshots and logs are ignored local development artifacts. They do not prove every screen/state is pixel-identical.

## Distribution

Rust xtask creates and verifies debug and optimized Linux archives, with executable mode, safe/unique paths, source-notice hashes, desktop entry, original icon, fonts/licenses, documentation and build metadata. Extracted executables pass version/help checks and native Xvfb demo rendering. Required host libraries resolve on this Ubuntu box. Archives and checksums remain under ignored dist/; see [packaging](packaging.md).

## Limits

No private live server, live Jellyfin, real sound device, hardware GPU, actual unlocked desktop keyring, desktop shell overlay, macOS runtime, or Windows runtime has been validated. Cross-distribution compatibility and signed installers are not established. Full feature parity and pixel identity remain incomplete; [parity](parity.md) records the outstanding work.
