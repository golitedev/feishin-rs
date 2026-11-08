# Architecture

Status: design only. No components described below have been implemented.

## Product boundary

feishin-rs is a native Rust desktop music client. Rust owns the interface, server communication, library data, queue, settings, and control logic. A separately installed MPV process owns media decoding and audio output.

Use egui for the interface and eframe for the desktop event loop and rendering integration. PDFCraft demonstrates this arrangement; its PDF engine and large crate hierarchy are not needed here. Begin with focused components and split crates where their boundaries are useful.

## Planned workspace

```text
apps/feishin-rs/       Desktop entry point and component wiring
crates/core/          Domain models, library/queue state, application commands
crates/servers/       Navidrome, Jellyfin, and OpenSubsonic adapters
crates/player-mpv/    MPV process lifecycle, IPC commands, and events
crates/ui-egui/       Views, widgets, themes, and input handling
crates/platform/      Settings paths, credentials, and desktop integration
xtask/                Rust project tooling when needed
```

These paths are planned, not existing directories. The core must not depend on egui, eframe, or MPV transport details. Server adapters normalize their responses into shared domain models. The desktop entry point wires components together; the interface issues application commands and presents resulting state.

## Playback through MPV

1. The core selects a queue item and asks the server adapter for a playable source and any required authentication information.
2. The MPV adapter sends the appropriate load command through JSON IPC.
3. MPV events update observed state: position, duration, pause state, buffering, errors, and end of track.
4. The core reconciles these events with the queue and sends updated state to the interface.

Start one application-owned MPV process with an isolated IPC endpoint. Track its lifecycle and shut down only the process the application started. Use a Unix-domain socket on Linux/macOS and a named pipe on Windows when those platforms are implemented.

Correlate command replies by request ID, subscribe to relevant property changes, and handle partial messages, timeouts, disconnects, and process exits. Connection recovery must not duplicate playback commands or advance the queue twice. Distinguish normal end of track from load failures, explicit stops, and replacement of the current track.

Allow configuration of the MPV executable path and report a useful error if it is missing or unusable. Secure the IPC endpoint for the current user. Avoid shell interpolation when starting MPV. Do not expose stream credentials in command logs or diagnostics.

All audio decoding and output stays inside MPV. There is no browser player, embedded libmpv, or fallback playback engine. MPV's internal codec and audio dependencies are part of the MPV exception.

## Responsiveness and persistence

Run network access, artwork decoding, and blocking IPC outside the UI thread. Send typed results and events back to the application and request a repaint when visible state changes. Choose the async runtime and channel crates during implementation.

Separate user preferences and resumable queue/session data from credentials. Use an appropriate credential store or explicitly documented secure alternative. Write persistent state atomically and handle malformed settings without crashing. Define schema versions before depending on saved state across releases.

## Verification boundaries

Test queue transitions and server normalization independently of the GUI. Test IPC parsing and request correlation with synthetic messages. Use an installed MPV for explicit integration tests of startup, commands, events, and shutdown. Keep live-server credentials out of fixtures. UI tests should exercise user workflows rather than duplicate widget implementation.
