# MPV playback adapter

`feishin-player-mpv` runs an external MPV executable and controls it through
[MPV's documented JSON IPC protocol](https://mpv.io/manual/stable/#json-ipc).
It does not embed libmpv or implement an audio output path.

## Supported environment

The current transport uses Unix-domain sockets on Linux and macOS. Linux has
been tested with MPV 0.41.0. Use MPV 0.41 or newer as the supported baseline;
older versions have not been validated. macOS has not been runtime-tested.
Windows returns an explicit unsupported-transport error until named-pipe
support is implemented.

An MPV build must return `data.playlist_entry_id` in successful `loadfile`
replies. Missing identity is an error rather than an invitation to guess which
track ended. The adapter uses named `loadfile` arguments, so the insertion-index
argument introduced in MPV 0.38 does not change its argument positions.

## Lifecycle and commands

`Player::spawn(Config)` starts a worker and returns immediately. Startup success
arrives as `Event::Ready`; failures arrive as `Event::Error` followed by
`Event::Disconnected`. The UI sends commands without waiting for socket I/O.
The command queue is bounded; a full queue returns `Error::Busy`.

The worker creates an owner-only temporary directory, launches MPV with a
private socket, and keeps a single IPC connection open for property
subscriptions. It disables user MPV configuration, scripts, terminal input,
video, looping, and youtube-dl integration. MPV's stderr is not forwarded,
because diagnostics can expose authenticated URLs. Stream headers are supplied
as file-local options and are cleared on the next load.

Each `Load` includes an application generation. Request IDs correlate MPV
replies with commands. Its `paused` flag is passed as a file-local option so a
pending user pause survives startup without first starting audio. Playlist
entry IDs correlate file events with the generation. Events arriving before
their acknowledgement are deferred until the entry can be identified.
`FileLoaded(generation)` and
`EndFile { generation, reason }` let callers ignore a previous selection's
late events. Only `EndReason::Eof` represents natural completion; stop, error,
quit, and redirect are distinct. Callers must compare generations before
advancing the queue or reporting listening completion.

Missing MPV, startup timeout, malformed or oversized IPC messages, unacknowledged
commands, disconnection, and process exit produce fixed, credential-free error
messages. Failed IPC is not retried automatically: retrying an uncertain load
could duplicate playback. Reconnection creates a new player instance. Dropping
the handle requests shutdown, joins the worker, and kills/reaps only its owned
MPV child. The temporary socket directory is removed afterwards.

## Verification

Run protocol, lifecycle, and generation tests with:

```sh
cargo test -p feishin-player-mpv
```

The explicit integration tests require a real MPV executable. They use MPV's
null audio output, synthetic WAV data, and a loopback HTTP fixture:

```sh
MPV_BIN=/path/to/mpv cargo test -p feishin-player-mpv --test real_mpv -- --ignored
```

They verify load, pause, seek, volume, explicit stop, natural EOF, bad-file
failure, shutdown, exact authentication headers, and removal of those headers
on a subsequent load. No real server or credentials are required.
