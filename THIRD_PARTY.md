# Third-party sources and components

## Original project work

The MIT license in LICENSE covers original feishin-rs contributions. It does not relicense third-party code, executables, fonts, icons, or other assets.

## References

- Feishin: https://github.com/jeffvli/feishin. The inspected upstream checkout declares GPL-3.0. Use its documented features and observable behavior as reference; do not copy or translate its implementation, or import its assets into this repository.
- PDFCraft: https://github.com/storytold/pdfcraft. The inspected checkout declares MIT OR Apache-2.0 for its code, with separate asset and brand terms. It is a reference for Cargo workspace organization and a Rust-native egui/eframe interface. No code or assets have been imported.

## MPV

MPV is the planned external playback dependency: https://github.com/mpv-player/mpv.

The application will control a separately installed MPV executable over JSON IPC. No MPV binary or source is included in this repository. MPV's own license and the licenses of its enabled dependencies remain separate from this project's MIT license. Packaging MPV requires a separate review of the exact build and its notices before distribution.

## Adding dependencies or assets

Before importing third-party material, record its original source, exact version or revision, license, modifications if any, and required notices here or in a dedicated inventory linked from this file. Include required license texts with redistributed material. Review transitive native dependencies against the project's Rust-only direction.

Do not assume that reference-project artwork, fonts, translations, or branding share that project's code license. Prefer original assets or assets with explicit redistribution terms compatible with the intended distribution.

No third-party code or assets are currently vendored. Dependency and asset inventories will be added when actual dependencies or assets are introduced.
