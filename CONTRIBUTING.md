# Contributing

Read [AGENTS.md](AGENTS.md) and the [architecture](docs/architecture.md) before changing the project.

## Development status

The first native application is implemented in a Cargo workspace. The [roadmap](docs/roadmap.md) and [parity checklist](docs/parity.md) distinguish working features from missing or unvalidated behavior.

The stack is Rust 2024 (minimum Rust 1.95), egui/eframe 0.37, and an external MPV 0.41+ executable. Cargo.lock pins dependencies. Run `cargo run --locked -p feishin-rs -- --demo` to inspect the native interface without an account.

## Changes

Keep each change focused. Explain the resulting behavior, why it is needed, and how it was checked. Keep the UI separate from server access and playback. MPV must remain the only playback engine.

For documentation, verify relative links and run `git diff --check`. Use `cargo fmt --all --check`, scoped Clippy with `-D warnings`, and relevant tests. Test MPV lifecycle and IPC failure cases when modifying playback. README.md lists explicit real-MPV, MPRIS, and native screenshot checks. Keep builds at two jobs on this remote server.

Use descriptive commits, for example `docs: describe MPV playback architecture` or `feat(player): handle MPV disconnects`. Stage only the current task's files and commit completed work. Never include credentials or private server data.

## Licensing

Submit only work you have the right to contribute under MIT. Do not copy or translate GPL-licensed Feishin source into this project. Dependencies and assets retain their own licenses and must follow [THIRD_PARTY.md](THIRD_PARTY.md).
