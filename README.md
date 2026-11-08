# feishin-rs

A native Rust music client inspired by [Feishin](https://github.com/jeffvli/feishin), with MPV as its only playback engine.

## Status

Repository bootstrap only. Documentation, licensing, and contributor rules are in place. No application code, Cargo workspace, or runnable player exists yet.

## Project direction

- Write the application, desktop interface, and project tooling in Rust.
- Build the native interface with egui and eframe, following the separation of UI and engine demonstrated by PDFCraft.
- Use an external MPV process controlled through its JSON IPC protocol for all playback.
- Target Navidrome, Jellyfin, and OpenSubsonic-compatible music servers.
- Keep server access, library state, queue management, playback control, and presentation in separate components.
- Start development on Linux; keep platform-specific code isolated for future Windows and macOS support.

MPV is the explicit exception to the Rust-only rule. There will be no JavaScript or TypeScript frontend, Electron, webview, browser audio, or alternative playback engine.

## Documentation

- [Architecture](docs/architecture.md): component boundaries and playback design.
- [Roadmap](docs/roadmap.md): implementation order and current status.
- [Contributing](CONTRIBUTING.md): development and review conventions.
- [Agent instructions](AGENTS.md): rules for automated contributors.
- [Third-party policy](THIRD_PARTY.md): source, dependency, and asset provenance.

## Building

Build and run commands will be documented when the first Cargo workspace lands. Rust dependencies and the minimum supported Rust version have not been selected or pinned. An installed MPV executable will be required for playback.

## License

Original contributions to feishin-rs are licensed under the [MIT License](LICENSE).

Upstream Feishin is GPL-3.0 licensed. This project is an independent implementation: upstream source code, translations of that code, and assets must not be copied into this MIT repository. MPV and other third-party components retain their own licenses. See [THIRD_PARTY.md](THIRD_PARTY.md).

This project is not presented as an official Feishin release.
