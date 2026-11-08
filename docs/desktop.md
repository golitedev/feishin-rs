# Desktop media controls

`feishin-desktop` implements the Linux
[MPRIS interface](https://specifications.freedesktop.org/mpris/latest/)
with the Rust `zbus` crate. It does not link libdbus or introduce another
playback engine. Commands still reach MPV through the application.

The service owns `org.mpris.MediaPlayer2.feishin_rs` on the user's session bus,
at `/org/mpris/MediaPlayer2`. It exposes the application and Player interfaces,
metadata, playback status, normalized volume, queue capabilities, loop mode,
shuffle, position, and desktop transport controls. It also routes Raise and
Quit requests back to the application. Opening arbitrary URIs, track-list
editing, and playback-rate changes are not supported.

`Desktop::start()` returns immediately. Bus setup, method handling, and signals
run outside the UI thread. `update(DesktopState)` coalesces application snapshots
instead of queuing every frame. Poll `try_recv()` for typed desktop commands.
A missing session bus, conflicting service name, or lost connection produces
an asynchronous `DesktopCommand::Error` diagnostic; the application can keep
running without desktop integration. Dropping the handle requests worker
shutdown without blocking on a bus handshake.

State with no current track advertises Stopped. A track with `paused = true`
advertises Paused, and an unpaused track advertises Playing. The caller should
clear the desktop track on actual stop or end of queue even if it retains the
last track in the application UI. MPRIS volume uses 0.0 through 1.0; the Rust
boundary uses percentages. Seek commands are absolute seconds at the boundary,
and D-Bus positions/offsets are microseconds. SetPosition ignores stale track
identities. Metadata never contains authenticated stream URLs.

Changed properties emit PropertiesChanged. Ordinary position progress does not
emit a property signal, as required by MPRIS. Call `Desktop::seeked(position)`
after a confirmed application/MPV seek to emit Seeked with the actual position.

The current implementation is Linux-only. Other platforms report unsupported
desktop integration. Linux D-Bus behavior is tested with an isolated session
bus; interaction with a particular desktop shell has not been visually tested.

```sh
cargo test -p feishin-desktop
dbus-run-session -- cargo test -p feishin-desktop --test mpris -- --ignored
```
