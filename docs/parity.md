# Feishin parity and validation

Status: 2026-11-09. The native Rust implementation has substantial server, library, and MPV functionality. Full feature parity and pixel identity are not established. Controls, protocol support, live-server evidence, and visual similarity are separate acceptance criteria.

## Reference and visual evidence

The current observed reference is the official Feishin 1.17.0 Linux desktop binary connected to a disposable Navidrome library (downloaded Debian archive SHA-256 `8f9833e4c16b40c4451e81c1c9deb35e0ce1658b7a8525d3f1487b08d85bc903`). Earlier work used public README screenshots at upstream commit `155e562138f39adaa985a01bc974936925518b46`. No upstream implementation, CSS, artwork, translations, or bundled fonts were imported or translated.

Current native work follows the observed title bar, 400 px sidebar, Inter typography, single featured-album hero, four-column genre tiles, album cards, playback strip, settings tabs, menus, and fullscreen player. Inter and Poppins are separately sourced OFL fonts. Native eframe screenshots are captured under Xvfb at 1440 × 900, using the same original fixture library as the observed application. Matching key geometry is evidence of progress, not a pixel-identical audit of every state.

## Working workflows

| Area | Implemented behavior | Limits |
| --- | --- | --- |
| Library | Albums, tracks, artists, album artists, genres, folders, server search, local sorting/filtering, saved filters, quick search | Full library is initially fetched; large-library performance remains unmeasured |
| Favorites and ratings | Track, album, and artist favorites; 0–5 numeric ratings; minimum-rating filters | Numeric ratings require Subsonic support; server permissions still apply |
| Playlists | Create/delete, add/remove distinct occurrences, metadata/public visibility, move tracks, preserve duplicate songs | Native Navidrome smart rules use an editable JSON criteria object; no graphical rule builder |
| Smart playlists | Navidrome native authentication and list/create/update/delete rules | Native API is version-sensitive; verified with Navidrome 0.64.2 |
| Radio and downloads | Subsonic internet-radio CRUD/playback; original-file downloads to a chosen destination | Downloads do not constitute an indexed offline library; radio is server-dependent |
| Lyrics | OpenSubsonic and Jellyfin timed lyrics, plaintext fallback, follow/seek, offset, alignment, font size | No independent external provider search, translation, or word-by-word karaoke renderer |
| Playback | MPV load/pause/seek/volume/stop, repeat/shuffle/play-next, queue move/remove/select, prepared successor playback, Auto DJ | Remote buffering can interrupt transitions; no sample-accurate network-gapless guarantee or crossfade |
| Audio | Device selection, exclusive-output request, channels/sample rate, speed/pitch, pause/resume fades, Weak/Yes/No gapless, audio/cache parameters, ReplayGain, preamp, ten-band EQ, normalization, technical stats | Output-driver support varies; no real sound device validated |
| Analysis | Real MPV RMS/peak stereo meters | No FFT spectrum or full upstream visualizer catalogue |
| Accounts and preferences | Multiple profiles, optional OS keyring, schema migration, queue/position restoration, themes/custom color JSON, fonts/layout, configurable shortcuts | English interface only; no arbitrary custom CSS |
| Desktop | Linux MPRIS, native title bar, optional Linux notifications, bounded metadata-free diagnostics | No tray, Discord presence, casting/remote controller, or macOS/Windows media integration |
| Distribution | Rust xtask checks, Linux desktop archive, fonts/licenses/dependency inventory and archive verification | No signed installer, AppImage, auto-updater, or Windows/macOS release package |

## Protocol and runtime verification

Synthetic loopback HTTP tests cover Subsonic/Navidrome and Jellyfin authentication, pagination, normalized data, duplicate playlist occurrences, metadata/rating/radio/lyrics/transcoding/download/folder requests, smart rules, and playback-report identity. A real disposable Navidrome 0.64.2 instance verifies advanced mutations and round trips against generated FLACs, cover images, and timed lyrics. No user account or private library is required.

Real MPV tests use synthetic media, null audio, and authenticated HTTP. They cover startup, property changes, settings, EQ, audio levels, prepared transitions, cancellation, EOF, and per-source header isolation. The application rejects obsolete session/browse/load/preparation results, preserves queue edits when Auto DJ returns, repairs cancelled successors, and carries exact source method into delayed Jellyfin reports. Linux MPRIS has an explicit isolated-D-Bus integration test. Windows MPV named-pipe transport has passed a GNU-target compilation check; no Windows runtime has been exercised.

See [validation](validation.md) for executed commands and counts. Screenshots and disposable server data stay under ignored `local/`.

## Outstanding acceptance work

- Complete the remaining workflows in the limits column and compare their behavior against the running reference. Do not substitute a visually present control for working behavior.
- Audit every screen, context menu, modal, keyboard/focus state, narrow layout, loading/error state, and fullscreen view against upstream. Existing native screenshots do not prove identity.
- Add live Jellyfin compatibility evidence, large-library measurements, a real sound-device session, hardware rendering checks, and desktop accessibility assessment.
- Exercise OS credential persistence with a real unlocked desktop keyring, actual desktop notifications/media overlays, Windows named pipes at runtime, and macOS integration.
- Validate release builds and platform installers on their target systems before advertising distribution support.

The MIT implementation remains independent of GPL Feishin source. Feature completion must preserve the Rust-only application and MPV-only playback requirements.
