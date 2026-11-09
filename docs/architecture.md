# Architecture

Status: native implementation with advanced server and playback workflows. See parity.md for unimplemented and unvalidated behavior.

## Product boundary

feishin-rs is a native Rust port of Feishin. Rust owns the interface, server communication, library data, queue, settings, and control logic. A separately installed MPV process owns media decoding and audio output.

Use egui for the interface and eframe for the desktop event loop and rendering integration. PDFCraft demonstrates this arrangement; its PDF engine and large crate hierarchy are not needed here. Begin with focused components and split crates where their boundaries are useful.

## Workspace

```text
apps/feishin-rs/       Desktop entry point and component wiring
crates/core/          Domain models, library/queue state, application commands
crates/servers/       Navidrome, Jellyfin, and OpenSubsonic adapters
crates/player-mpv/    MPV process lifecycle, IPC commands, and events
crates/ui-egui/       Views, widgets, themes, and input handling
crates/platform/      Settings, OS credentials, diagnostics, notifications
crates/desktop/       Linux MPRIS service and desktop transport commands
xtask/                Rust validation, license inventory, Linux packaging
```

The core does not depend on egui, eframe, or MPV transport details. Server adapters normalize their responses into shared domain models. The desktop entry point wires components together; the interface issues application commands and presents resulting state. `xtask` owns release preparation without introducing another scripting language.

## Playback through MPV

1. The core selects a queue item and asks the server adapter for a playable source and any required authentication information.
2. The MPV adapter sends the appropriate load command through JSON IPC.
3. MPV events update observed state: position, duration, pause state, buffering, errors, and end of track.
4. The core reconciles these events with the queue and sends updated state to the interface.

One application-owned MPV process has an isolated IPC endpoint. Dropping the handle terminates and reaps only that process. Unix-domain sockets and Windows named pipes are implemented. MPV 0.41+ is the tested baseline; Windows has only been cross-checked, not runtime-tested.

Command replies are correlated by request ID and file events by MPV playlist entry ID and the application's load generation. Late EOF events from a previous selection cannot advance the new queue item. Natural EOF, failed loads, explicit stops, and replacement are distinct. Partial messages, timeouts, disconnects, and process exits are handled as errors; uncertain commands are not automatically retried. See [MPV details](mpv.md).

Allow configuration of the MPV executable path and report a useful error if it is missing or unusable. Secure the IPC endpoint for the current user. Avoid shell interpolation when starting MPV. Do not expose stream credentials in command logs or diagnostics.

All audio decoding and output stays inside MPV. There is no browser player, embedded libmpv, or fallback playback engine. MPV's internal codec and audio dependencies are part of the MPV exception.

The app prepares a cloned successor queue and obtains its source in the background. A ticket, session, and load generation reject obsolete preparation results. Queue edits invalidate preparation; the successor snapshot becomes authoritative only on a matching transition. Late file-loaded events for cancelled successors are repaired without replacing newer user selections. Auto DJ rechecks duplicates against the current queue when its result arrives. MPV URL prefetch is disabled to prevent successor requests inheriting current-source authorization. Remote buffering can therefore interrupt a transition; sample-accurate network playback is not guaranteed.

Playback settings are typed, normalized, and applied to the same MPV process. Audio analysis reads MPV's real RMS/peak metadata; it does not synthesize a spectrum. Source transcode identity travels with each load and its asynchronous Start report, so preparing a different source cannot change the active Jellyfin session's playback method.

## Responsiveness and persistence

Network access and artwork decoding run on background threads using blocking reqwest; the MPV adapter owns a dedicated worker. Typed results arrive through standard-library channels. Session, browse, and load generations discard stale results. Artwork requests are bounded to four in flight, with bounded response/decode sizes and a session cache. Track painting is clipped to visible rows.

Playback reports are serialized on a separate worker to preserve start/progress/stop order. MPRIS uses its own worker and coalesced snapshots, so D-Bus does not block the GUI. The UI polls progress at a modest interval in addition to background repaint requests.

On exit, the app queues the current stop report and waits at most two seconds for the reporting worker to flush. A slow or unavailable server can still miss its final report; shutdown does not wait indefinitely for network I/O.

Settings schema 2 stores server profiles, MPV path, queue and position, normalized preferences, saved filters, shortcuts, and hashed identities of explicitly remembered accounts. It migrates schema 1. Passwords and tokens never enter settings JSON. Optional OS credentials use a serial worker: a revision cancels obsolete saves/loads, and Forget is ordered after any in-flight save. Profile URLs cannot contain embedded credentials, query strings, or fragments. `tempfile` stages owner-only files before atomic replacement. Unknown/corrupt settings are preserved and writes are disabled for that session. Restoring a queue does not start audio; matching file-loaded events restore the saved position after user playback.

Diagnostics accept an enum of event names, without arbitrary error strings or metadata. A bounded worker rotates local JSONL files. Notifications run off the UI thread and honor private mode. OS integrations failing remain recoverable and do not introduce alternate playback paths.

## Verification boundaries

Test queue transitions and server normalization independently of the GUI. Test IPC parsing and request correlation with synthetic messages. Use an installed MPV for explicit integration tests of startup, commands, events, and shutdown. Keep live-server credentials out of fixtures. UI tests should exercise user workflows rather than duplicate widget implementation.
