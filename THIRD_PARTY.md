# Third-party sources and components

## Original project work

The MIT license in LICENSE covers original feishin-rs contributions. It does not relicense third-party code, executables, fonts, icons, or other assets.

## References

- Feishin: https://github.com/jeffvli/feishin. The inspected upstream checkout declares GPL-3.0. Use its documented features and observable behavior as reference; do not copy or translate its implementation, or import its assets into this repository.
- PDFCraft: https://github.com/storytold/pdfcraft. The inspected checkout declares MIT OR Apache-2.0 for its code, with separate asset and brand terms. It is a reference for Cargo workspace organization and a Rust-native egui/eframe interface. No code or assets have been imported.

## MPV

MPV is the external playback dependency: https://github.com/mpv-player/mpv. The tested baseline is MPV 0.41.0.

The application controls a separately installed MPV executable over JSON IPC. No MPV binary or source is tracked or distributed in this repository. An unprivileged test copy and its runtime libraries may exist in ignored local/ on the development server. MPV's own license and the licenses of its enabled dependencies remain separate from this project's MIT license. Packaging MPV requires a separate review of the exact build and its notices before distribution.

## Embedded assets

Poppins Regular and Bold are unmodified font files from the official google/fonts repository at revision `8b0a1d0f5983c89bc2b93f1b5fb55f9e252744b5`, under SIL OFL 1.1. Copyright 2020 The Poppins Project Authors. The [font inventory](crates/ui-egui/assets/fonts/README.md) records exact source and SHA-256 values, and [OFL.txt](crates/ui-egui/assets/fonts/OFL.txt) contains the required license. Icons and placeholder artwork are original Rust drawings; demo metadata is original synthetic material under MIT. No Feishin or PDFCraft images, logos, or other assets are embedded.

## Rust dependencies

Cargo.lock is the exact version/checksum inventory for registry dependencies; none are patched or vendored. Direct third-party components and their upstream license declarations are:

- egui/eframe 0.37: MIT OR Apache-2.0, https://github.com/emilk/egui.
- reqwest 0.12: MIT OR Apache-2.0, https://github.com/seanmonstar/reqwest.
- serde/serde_json 1: MIT OR Apache-2.0, https://github.com/serde-rs/serde and https://github.com/serde-rs/json.
- thiserror 2: MIT OR Apache-2.0, https://github.com/dtolnay/thiserror.
- image 0.25: MIT OR Apache-2.0, https://github.com/image-rs/image.
- rand 0.9: MIT OR Apache-2.0, https://github.com/rust-random/rand.
- tempfile 3: MIT OR Apache-2.0, https://github.com/Stebalien/tempfile.
- uuid 1: MIT OR Apache-2.0, https://github.com/uuid-rs/uuid.
- md-5 0.10: MIT OR Apache-2.0, https://github.com/RustCrypto/hashes; used only for Subsonic's required authentication protocol.
- zbus 5.19: MIT, https://github.com/dbus2/zbus; Linux desktop integration, without libdbus linkage.

Transitive dependencies include window-system bindings and native cryptographic implementation code through rustls/ring. The project-owned application/UI/tooling are Rust; this inventory does not assert a wholly Rust transitive dependency graph. Third-party crate notices must be collected and shipped before binary releases. Release packaging and notice generation are not implemented yet.

## Adding dependencies or assets

Before importing third-party material, record its original source, exact version or revision, license, modifications if any, and required notices here or in a dedicated inventory linked from this file. Include required license texts with redistributed material. Review transitive native dependencies against the project's Rust-only direction.

Do not assume that reference-project artwork, fonts, translations, or branding share that project's code license. Prefer original assets or assets with explicit redistribution terms compatible with the intended distribution.

No third-party source code is vendored. The embedded fonts above are the only bundled third-party assets beyond assets provided by Cargo dependencies (such as egui's fallback fonts).
