# Pinned upstream license files

These are original license files for dependencies whose published Cargo source omits its code-license files. Each package directory records its package version, upstream repository, published package VCS revision, immutable license-file URLs, and SHA-256 hashes in `provenance.json`.

`revision` identifies the published Cargo source through `.cargo_vcs_info.json`. `license_revision` identifies the commit supplying the actual notice files. They normally match. For the egui 0.37.0 family, the published VCS revision does not supply the requested files; `license_reference` records the exact `0.37.0` release-tag API URL, resolved to a separate immutable commit. Both revisions are retained so this difference is visible. File hashes are checked on every inventory/package run.

Run `cargo xtask notices --fetch-missing` to fetch missing snapshots. Review new files and provenance before committing them. Ordinary `check`, `notices`, and `package` runs use existing snapshots and fail on missing or modified files. Never replace missing upstream notices with generic SPDX license templates.

Files are preserved byte for byte. EditorConfig and Git whitespace attributes exempt these files from trailing-space cleanup so their recorded hashes remain valid. Source versions and licenses remain those of the original projects; this directory does not relicense them.
