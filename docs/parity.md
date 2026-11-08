# Feishin parity and validation

Status: first working Rust implementation, 2026-11-08. Full parity and pixel-identical UI have not been achieved. Do not interpret an implemented screen as complete parity with every upstream control or server capability.

## Reference

The observed reference is the local Feishin development checkout at commit `155e562138f39adaa985a01bc974936925518b46`. Its README screenshots of Home, album detail, artist detail, and fullscreen playback were viewed as visual references. No TypeScript, CSS, implementation, artwork, or translations were copied or translated into the Rust project. The interface is independently drawn in Rust, with separately sourced Poppins fonts.

## Implemented

- Native dark layout: sidebar search/navigation/playlists/server card, featured albums, genre tiles, album grids, detail heroes, track tables, and playback strip.
- Album, track, artist, playlist, search, settings, queue, and fullscreen now-playing/lyrics views.
- Navidrome/OpenSubsonic token authentication and Jellyfin authentication, paginated normalized libraries, artwork, search, and detail requests.
- External MPV load/pause/seek/volume/stop, property subscriptions, recoverable errors, secure process/socket lifecycle, and tagged file events.
- Queue selection/removal/clear, play next/last, previous/next, shuffle without immediate repeated items in a cycle, and repeat off/all/one.
- Track favorites, plaintext lyrics, playlist create/delete/add/remove. Playlist order and duplicate song occurrences are preserved.
- Subsonic now-playing/scrobble and Jellyfin playback start/progress/stop reporting.
- Credential-free versioned settings and queue restoration; no automatic playback or stored passwords.
- Linux MPRIS metadata, playback commands, seeking, volume, shuffle/repeat, and Raise/Quit.

## Verification

Core tests exercise repeat/shuffle/removal. Server tests use synthetic loopback HTTP to exercise authentication, URL paths, pagination, protocol normalization, duplicate playlist entries, mutations, and stale playback reports. Real MPV tests use generated WAV files, null audio, and authenticated HTTP streams, including EOF queue advancement. Application regression tests cover stale sessions/search/stream events and delayed playlist results. UI tests render empty/sample views headlessly and exercise input. Native screenshots are captured through eframe under Xvfb for visual review. MPRIS is tested through an isolated D-Bus session.

These checks do not establish live Navidrome/Jellyfin compatibility, sound-device quality, every desktop shell's media controls, macOS/Windows operation, or pixel identity with upstream.

## Remaining work

- Complete a screen-by-screen visual and interaction audit against a running current Feishin instance, including spacing, responsive layouts, menus, keyboard focus, and all settings. Current screenshots are a reference, not exhaustive behavior evidence.
- Live-server verification and larger-library performance measurements. Initial connection currently fetches the full library, and filters/sorts are computed locally.
- Smart playlists, ratings, advanced playlist editing/reordering, saved filters, server-specific navigation, and multiple saved server profiles.
- Synchronized lyrics, lyric providers/translation, visualization, audio analysis, equalizer/output/device controls, ReplayGain and crossfade/gapless behavior.
- Transcoding selection, downloads/offline mode, richer metadata, internet radio, casting/remote control, Discord integration, and any other upstream features not explicitly listed as implemented.
- Light/custom themes, full localization, accessibility audit, configurable shortcuts, notifications/tray, and complete window/session preferences.
- Secure persistent credential-store integration if desired; exact playback position restoration.
- Windows named-pipe IPC, macOS integration/runtime validation, desktop installers, release builds, full third-party notices, and release automation.

The next validation target is a real server-backed listening session with a native visual audit. Add evidence and narrow limitations as each gap is closed.
