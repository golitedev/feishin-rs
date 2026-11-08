# feishin-rs

A native Rust music client inspired by [Feishin](https://github.com/jeffvli/feishin), with MPV as its only playback engine.

## Status

The first implementation runs: a Rust egui/eframe desktop interface, Navidrome/OpenSubsonic and Jellyfin adapters, an MPV JSON IPC player, queue management, and credential-free settings persistence. This is an early port, not a finished replacement or a claim of identical UI. See [feature parity and gaps](docs/parity.md).

Implemented workflows include album/artist/track browsing, server search, playlist playback and basic editing, favorites, plaintext lyrics, shuffle/repeat, seeking, volume, queue editing, and Linux MPRIS controls. Server behavior is tested with synthetic HTTP fixtures and MPV playback with real MPV and synthetic media. A private live music server has not been tested.

Application code and interface are Rust. MPV is the explicit external playback exception. There is no Electron, webview, JavaScript/TypeScript frontend, browser audio, embedded libmpv, or second player. Low-level dependencies still use operating-system APIs and native cryptography; this is not a claim that every transitive dependency is implemented exclusively in Rust.

## Build and run

Requirements: Rust 1.95 or newer, a desktop graphical session, and MPV 0.41 or newer for playback. Cargo.lock pins dependencies. Linux is the tested platform. macOS has a Unix IPC implementation but is untested; Windows MPV transport is not implemented.

```sh
cargo build --locked -p feishin-rs
cargo run --locked -p feishin-rs
```

Open Settings, choose the server type, enter the final server URL (including any reverse-proxy base path), username, and password, then Connect. MPV can be selected by executable path in Settings. Passwords and server tokens stay in memory: reconnect after launching again. The server profile, MPV path, volume, queue, shuffle, and repeat preferences are saved, but playback does not start automatically.

On Linux, settings live in `$XDG_CONFIG_HOME/feishin-rs/settings.json`, or `~/.config/feishin-rs/settings.json`. Set `FEISHIN_RS_CONFIG_DIR` to override the directory. Malformed or newer-version settings are preserved and are not overwritten during that session.

This is a native desktop app. An SSH shell without a graphical session cannot display its window directly. For a headless rendering check, use the demo capture below; interactive use needs a desktop display or working X11 forwarding.

## Demo and native screenshots

```sh
cargo run --locked -p feishin-rs -- --demo
cargo run --locked -p feishin-rs -- --demo --demo-page album
mkdir -p local
xvfb-run -a -s '-screen 0 1440x900x24' env LIBGL_ALWAYS_SOFTWARE=1 \
  target/debug/feishin-rs --demo --demo-page album --screenshot local/album.png
```

Demo mode is labeled and contains original sample metadata and Rust-drawn placeholder artwork. It makes no server requests or playback attempts and does not save the sample library. Available demo pages: `home`, `albums`, `album`, `artists`, `artist`, `tracks`, `playlist`, `settings`, `queue`, `now-playing`, and `lyrics`.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
MPV_BIN=/path/to/mpv cargo test -p feishin-player-mpv --test real_mpv -- --ignored
MPV_BIN=/path/to/mpv cargo test -p feishin-rs --test listening -- --ignored
dbus-run-session -- cargo test -p feishin-desktop --test mpris -- --ignored
```

Real MPV tests use null audio output, generated WAV data, and a local HTTP fixture. No account or private library is required. See [MPV adapter](docs/mpv.md), [server APIs](docs/server-api.md), [desktop controls](docs/desktop.md), and [architecture](docs/architecture.md).

The [validation record](docs/validation.md) lists executed checks and their practical limits.

## Documentation and license

- [Roadmap](docs/roadmap.md) and [parity](docs/parity.md).
- [Contributing](CONTRIBUTING.md) and [agent instructions](AGENTS.md).
- [Third-party provenance](THIRD_PARTY.md), including the separately licensed Poppins fonts.

Original contributions are [MIT licensed](LICENSE). Upstream Feishin is GPL-3.0 licensed; its implementation and assets have not been copied or translated into this repository. MPV and other third-party components retain their own licenses. This is not an official Feishin release.
