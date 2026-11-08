# Architecture

Status: first implementation. See parity.md for unimplemented and unvalidated behavior.

## Product boundary

feishin-rs is a native Rust desktop music client. Rust owns the interface, server communication, library data, queue, settings, and control logic. A separately installed MPV process owns media decoding and audio output.

Use egui for the interface and eframe for the desktop event loop and rendering integration. PDFCraft demonstrates this arrangement; its PDF engine and large crate hierarchy are not needed here. Begin with focused components and split crates where their boundaries are useful.

## Workspace

```text
apps/feishin-rs/       Desktop entry point and component wiring
crates/core/          Domain models, library/queue state, application commands
crates/servers/       Navidrome, Jellyfin, and OpenSubsonic adapters
crates/player-mpv/    MPV process lifecycle, IPC commands, and events
crates/ui-egui/       Views, widgets, themes, and input handling
crates/platform/      Settings paths and credential-free atomic persistence
crates/desktop/       Linux MPRIS service and desktop transport commands
```

The core does not depend on egui, eframe, or MPV transport details. Server adapters normalize their responses into shared domain models. The desktop entry point wires components together; the interface issues application commands and presents resulting state. No xtask is needed yet.

## Playback through MPV

1. The core selects a queue item and asks the server adapter for a playable source and any required authentication information.
2. The MPV adapter sends the appropriate load command through JSON IPC.
3. MPV events update observed state: position, duration, pause state, buffering, errors, and end of track.
4. The core reconciles these events with the queue and sends updated state to the interface.

One application-owned MPV process has an isolated, owner-only IPC endpoint. Dropping the handle terminates and reaps only that process. Unix-domain sockets are implemented; Windows named pipes are not. MPV 0.41+ is the tested baseline.

Command replies are correlated by request ID and file events by MPV playlist entry ID and the application's load generation. Late EOF events from a previous selection cannot advance the new queue item. Natural EOF, failed loads, explicit stops, and replacement are distinct. Partial messages, timeouts, disconnects, and process exits are handled as errors; uncertain commands are not automatically retried. See [MPV details](mpv.md).

Allow configuration of the MPV executable path and report a useful error if it is missing or unusable. Secure the IPC endpoint for the current user. Avoid shell interpolation when starting MPV. Do not expose stream credentials in command logs or diagnostics.

All audio decoding and output stays inside MPV. There is no browser player, embedded libmpv, or fallback playback engine. MPV's internal codec and audio dependencies are part of the MPV exception.

## Responsiveness and persistence

Network access and artwork decoding run on background threads using blocking reqwest; the MPV adapter owns a dedicated worker. Typed results arrive through standard-library channels. Session, browse, and load generations discard stale results. Artwork requests are bounded to four in flight, with bounded response/decode sizes and a session cache. Track painting is clipped to visible rows.

Playback reports are serialized on a separate worker to preserve start/progress/stop order. MPRIS uses its own worker and coalesced snapshots, so D-Bus does not block the GUI. The UI polls progress at a modest interval in addition to background repaint requests.

On exit, the app queues the current stop report and waits at most two seconds for the reporting worker to flush. A slow or unavailable server can still miss its final report; shutdown does not wait indefinitely for network I/O.

Settings schema version 1 stores only server profile, MPV executable path, volume, queue, repeat, and shuffle. Passwords and tokens are memory-only; authentication is required on each launch. tempfile stages owner-only files before atomic replacement. Unknown/corrupt settings are preserved, and writes are disabled for that session. Queue restoration does not automatically resume audio or restore the exact position.

## Verification boundaries

Test queue transitions and server normalization independently of the GUI. Test IPC parsing and request correlation with synthetic messages. Use an installed MPV for explicit integration tests of startup, commands, events, and shutdown. Keep live-server credentials out of fixtures. UI tests should exercise user workflows rather than duplicate widget implementation.
