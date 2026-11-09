# feishin-rs

A native Rust port of [Feishin](https://github.com/jeffvli/feishin), with MPV as its only playback engine.

## Status

The Rust port supports server-backed listening, library management, advanced MPV settings, and persistent preferences. Full Feishin parity and an identical interface remain under development; the [parity record](docs/parity.md) lists implemented workflows and outstanding work.

Implemented workflows include album/artist/track/folder browsing, favorites, ratings, editable and smart playlists, internet radio, original-file downloads, synchronized lyrics, saved filters, multiple servers, themes, configurable shortcuts, notifications, and Linux MPRIS. Playback uses MPV for output devices, ReplayGain, equalization, speed/pitch, pause/resume fades, audio statistics, live level meters, and prepared queue transitions. Availability depends on the server protocol and account permissions. Tests include a real disposable Navidrome server and real MPV with synthetic media.

Library views provide a collapsible filter column for favorites, artist, genre, and minimum rating, plus search, sorting, and playback of the filtered collection. Album pages link to their artist and genre and show other albums by the artist. Artist pages switch between discography and tracks. Genre rows show track and album counts. Saved filters include artist and excluded-favorite selections.

Application code and interface are Rust. MPV is the explicit external playback exception. There is no Electron, webview, JavaScript/TypeScript frontend, browser audio, embedded libmpv, or second player. Low-level dependencies still use operating-system APIs and native cryptography; this is not a claim that every transitive dependency is implemented exclusively in Rust.

## Build and run

Requirements: Rust 1.95 or newer, a desktop graphical session, and MPV 0.41 or newer for playback. Cargo.lock pins dependencies. Linux is the tested platform. macOS uses Unix IPC; Windows named-pipe IPC passes a cross-compilation check. Neither platform has been runtime-tested.

```sh
cargo build --locked -p feishin-rs
cargo run --locked -p feishin-rs
```

Open the main menu, choose Manage servers, enter the server type, final URL (including any reverse-proxy base path), username, and password, then Connect. Settings provides MPV path and audio controls. Passwords remain in memory unless Remember password is selected; that option uses the operating system's credential store, never settings JSON. Linux requires an unlocked Secret Service, such as GNOME Keyring or KWallet with Secret Service support. A saved password allows reconnection at launch. Playback starts only on user action; the queue and exact position can be restored independently.

MPV extra parameters accept one literal audio/cache option per line, such as `--audio-buffer=0.2` or `--cache-secs=20`. Apply and restart MPV validates them and stops the existing player; the next playback starts with those options. Shell expansion, scripts, files, IPC/config overrides, and properties already managed by the audio controls are not accepted.

On Linux, settings live in `$XDG_CONFIG_HOME/feishin-rs/settings.json`, or `~/.config/feishin-rs/settings.json`. Set `FEISHIN_RS_CONFIG_DIR` to override the directory. Schema 2 migrates schema 1 automatically. Malformed or newer-version settings are preserved and are not overwritten during that session. Bounded local diagnostics contain event names and timestamps, without URLs, credentials, or library metadata.

This is a native desktop app. An SSH shell without a graphical session cannot display its window directly. For a headless rendering check, use the demo capture below; interactive use needs a desktop display or working X11 forwarding.

## Demo and native screenshots

```sh
cargo run --locked -p feishin-rs -- --demo
cargo run --locked -p feishin-rs -- --demo --demo-page album
mkdir -p local
xvfb-run -a -s '-screen 0 1440x900x24' env LIBGL_ALWAYS_SOFTWARE=1 \
  target/debug/feishin-rs --demo --demo-page album --screenshot local/album.png
```

Demo mode is labeled and contains original sample metadata and Rust-drawn placeholder artwork. It makes no server requests or playback attempts and does not save the sample library. Available demo pages: `home`, `albums`, `album`, `artists`, `artist`, `album-artists`, `tracks`, `favorites`, `genres`, `radio`, `folders`, `playlist`, `settings`, `queue`, `now-playing`, and `lyrics`.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
MPV_BIN=/path/to/mpv cargo test -p feishin-player-mpv --test real_mpv -- --ignored
MPV_BIN=/path/to/mpv cargo test -p feishin-rs --test listening -- --ignored
dbus-run-session -- cargo test -p feishin-desktop --test mpris -- --ignored
cargo xtask check
cargo xtask notices --fetch-missing
cargo xtask package --debug
```

Real MPV tests use null audio output, generated WAV data, and a local HTTP fixture. No account or private library is required. See [MPV adapter](docs/mpv.md), [server APIs](docs/server-api.md), [desktop controls](docs/desktop.md), and [architecture](docs/architecture.md).

The [validation record](docs/validation.md) lists executed checks and their practical limits. See [packaging](docs/packaging.md) for Linux archives, installation, and third-party license inventories.

## Documentation and license

- [Roadmap](docs/roadmap.md) and [parity](docs/parity.md).
- [Contributing](CONTRIBUTING.md) and [agent instructions](AGENTS.md).
- [Third-party provenance](THIRD_PARTY.md), including the separately licensed Inter and Poppins fonts.

Original contributions are [MIT licensed](LICENSE). Upstream Feishin is GPL-3.0 licensed; its implementation and assets have not been copied or translated into this repository. MPV and other third-party components retain their own licenses. This is not an official Feishin release.
