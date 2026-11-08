# Agent instructions

## Scope and current state

Read README.md, docs/architecture.md, and docs/roadmap.md before working here. This repository currently contains documentation only. Do not claim that planned features are implemented. Follow the user's requested scope; do not start application implementation during a documentation task.

## Language and playback

- All application logic, UI, and project-owned build tooling must be Rust. Declarative configuration, documentation, shaders, and appropriately licensed assets are allowed.
- Use egui/eframe for the native desktop interface. Keep UI dependencies out of the core, server, and playback components.
- MPV is the only allowed playback engine and the explicit exception to Rust-only dependencies. Control an external MPV process using JSON IPC.
- Do not introduce Electron, Tauri, a webview, handwritten JavaScript/TypeScript, browser playback, libmpv embedding, or a second audio engine.
- Ordinary operating-system and graphics integration is expected. Review new native dependencies; MPV is not blanket permission to add C/C++ application libraries or tools.

## Architecture and behavior

- Keep queue state and user playback intent in the core. Keep IPC, MPV process management, and observed playback state in the MPV adapter.
- Use typed commands, events, server models, and errors at component boundaries.
- Keep network requests, image decoding, and blocking IPC off the UI thread.
- Handle missing MPV, disconnects, invalid server responses, and process exits as recoverable errors. Never silently fall back to another player.
- Avoid panicking shortcuts in production code. Return useful errors and preserve user state.
- Do not add unsafe code to project-owned crates. Prefer maintained safe wrappers for OS integration.
- Never commit passwords, access tokens, authenticated stream URLs, user configuration, or listening history. Redact them from logs and examples.

## Licensing and provenance

- Original project contributions use MIT. Follow THIRD_PARTY.md for dependencies and assets.
- Upstream Feishin is GPL-3.0 licensed. Do not copy or translate its implementation into this repository, or copy its artwork, translations, or other assets.
- PDFCraft is an architecture reference, not a template to import wholesale. Its branding and project-specific rules do not become this project's assets or requirements.
- Document third-party additions with their source, version/revision, license, and required notices. Do not bundle MPV as part of an unrelated change.

## Verification and commits

- For documentation changes, check local links, consistency, and `git diff --check`. Do not invent build or test results.
- Once Rust code exists, run formatting, scoped Clippy with warnings denied, and relevant tests. Add tests for meaningful behavior and regressions.
- Keep Cargo.lock tracked when the application workspace is created.
- Update documentation when behavior, setup, or implementation status changes.
- After every requested change, stage only files changed for that request and create a commit before the final response. State the commit at the top of the final response when code changes; include it for documentation changes as well.
- Preserve unrelated user changes. Do not push or publish without authorization.
