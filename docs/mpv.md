# MPV playback adapter

`feishin-player-mpv` runs an external MPV executable and controls it through
[MPV's documented JSON IPC protocol](https://mpv.io/manual/stable/#json-ipc).
It does not embed libmpv or implement an audio output path.

## Supported environment

The transport uses Unix-domain sockets on Linux/macOS and named pipes on
Windows. Linux has been tested with MPV 0.41.0. Use MPV 0.41 or newer as the
supported baseline; older versions have not been validated. macOS has not been
runtime-tested. Windows named-pipe code uses safe Tokio wrappers, a randomly
generated pipe name, and bounded read/write waits. It passes a Windows GNU
cross-target compile check but has not been runtime-tested on Windows.

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

## Audio preferences and observed parameters

`Config::settings` supplies initial `PlaybackSettings` from `feishin-core`.
`Command::ApplySettings` applies normalized changes to the running MPV process.
Unchanged settings are not resent, so changing playback speed does not reload
the audio device or rebuild the filter chain.

The supported settings are device selection, exclusive output, sample rate and
channel layout, ReplayGain mode/preamp/fallback/clipping policy, independent
preamp, speed, volume ceiling, ten-band equalizer, dynamic normalization, and
gapless playback. Pitch preservation maps to MPV's `audio-pitch-correction`
and defaults to enabled. Gapless defaults to MPV's recommended `weak` mode,
which reopens output when the audio format changes; explicit `yes` and `no`
modes are also available. Numeric values are bounded before sending them to MPV. Device
and exclusive-mode availability depend on the audio output supported by the
installed MPV build.

`ListAudioDevices` produces `AudioDevices` using MPV's `audio-device-list`.
The adapter also observes `AudioStats`: source codec, sample format/rate/channel
layout/count and bitrate, plus actual output sample rate/channel layout.
Equalization and normalization run as MPV audio filters; no second decoder or
audio process is introduced.

Pause/resume fades are enabled by default and can be disabled. They use a
150 ms gain ramp through MPV's `volume-gain`
property. The configured preamp remains the baseline, and the user's volume
does not change. Pause takes effect at the end of fade-out; resume starts at a
quiet gain and fades up. Rapid toggles reverse the current ramp, and explicit
loads/stops restore the baseline so a new source cannot inherit attenuation.
`AudioGain` reports the actual observed MPV gain separately from `Volume`.

## Preparing the next track

With gapless enabled, the adapter sets MPV's `gapless-audio=weak` or `yes`.
The application resolves the next source in advance
and sends `PrepareNext { current_generation, generation, url, headers }` after
the current track has loaded. A stale current generation is ignored. Preparing
replaces the previous successor while preserving current playback, then emits
`PreparedNext(generation)` when MPV acknowledges it.

The application must keep the prepared target mapped to its queue item. On the
current track's natural EOF, it must avoid issuing a duplicate Load for an
acknowledged successor. MPV starts that entry automatically in the same
process, and `FileLoaded(target_generation)` activates the corresponding queue
item. Queue edits and repeat/shuffle changes must send ClearNext and prepare
the newly selected successor. An explicit Load replaces MPV's playlist.
ClearNext cancels generation promotion immediately; a successor which already
started can still produce a tagged FileLoaded event, so the application must
restore its desired selection when a cancelled successor wins this race.

MPV playlist URL prefetch is disabled because MPV 0.41 can open a successor
using the current item's HTTP headers before applying the successor's per-file
options. An authenticated current source must never supply its headers to an
unrelated queued source. A real HTTP fixture tests this isolation on both
explicit replacement and automatic prepared-track playback.

This removes the application-level stop/load gap. A remote successor still
opens at the transition and may need time to buffer; sample-accurate seamless
output across every server/device/format has not been measured. Crossfade is
not implemented or advertised by this adapter.

## Measured audio levels

`EnableAnalysis(true)` adds a labeled `astats` filter inside MPV. The worker
queries `af-metadata/feishin_levels` at about 20 Hz and emits `AudioLevels` with
left/right RMS and peak dBFS. It measures the filter chain before MPV's final
volume stage. Mono reports the same measured source channel on both meters;
digital silence is represented by a -120 dB display floor. Missing/invalid
metadata produces no invented replacement levels. Analysis is disabled by
default and is removed with `EnableAnalysis(false)`.

These are real level measurements suitable for stereo meters and level
history. They are not FFT bins or waveform samples. No arbitrary animation,
Lua script, raw-audio export, or additional decoder is used.

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

They verify load, paused startup, seek, volume, explicit stop, natural EOF,
bad-file failure, shutdown, exact authentication headers, removal of those
headers on a subsequent load, live audio settings, observed source/output
parameters, device discovery, automatic prepared-track playback, successor
cancellation, and measured stereo-tone/silence levels. No real server or
credentials are required.
They also verify pause-fade gain ramps, delayed pause, restored preamp after
resume/replacement, and preservation of the user's volume.
