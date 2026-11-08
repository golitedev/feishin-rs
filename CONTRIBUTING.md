# Contributing

Read [AGENTS.md](AGENTS.md) and the [architecture](docs/architecture.md) before changing the project.

## Development status

The repository currently has no application code or Cargo workspace. The [roadmap](docs/roadmap.md) lists the planned implementation stages. Do not add placeholder commands that suggest a runnable application exists.

The intended stack is Rust, egui/eframe, and an external MPV executable. Exact crate versions and the Rust toolchain requirement will be selected with the implementation.

## Changes

Keep each change focused. Explain the resulting behavior, why it is needed, and how it was checked. Keep the UI separate from server access and playback. MPV must remain the only playback engine.

For documentation, verify relative links and run `git diff --check`. Once the Rust workspace exists, use `cargo fmt --all --check`, scoped Clippy with `-D warnings`, and relevant tests. Test MPV lifecycle and IPC failure cases when modifying playback.

Use descriptive commits, for example `docs: describe MPV playback architecture` or `feat(player): handle MPV disconnects`. Stage only the current task's files and commit completed work. Never include credentials or private server data.

## Licensing

Submit only work you have the right to contribute under MIT. Do not copy or translate GPL-licensed Feishin source into this project. Dependencies and assets retain their own licenses and must follow [THIRD_PARTY.md](THIRD_PARTY.md).
