# Linux packaging

The project-owned packager is the Rust `xtask` crate. MPV remains separately installed and is never bundled. Native Linux is the only packaging target currently implemented; the resulting binary uses the build host's system libraries and is not a universal Linux executable.

## Build an archive

Install the Rust version required by the workspace and the native libraries needed by eframe on the build host. Then run:

```sh
cargo xtask check
cargo xtask notices
cargo xtask package
```

The default package builds the application with `cargo build --locked --release`. For a faster development artifact, use `cargo xtask package --debug`. Outputs are ignored under `dist/`:

- `feishin-rs-<version>-linux-<arch>[-debug].tar.gz`
- A matching `.tar.gz.sha256` file.
- `dist/notices/` when the separate `notices` command is run.

The archive contains the executable, desktop entry, original SVG icon, application documentation, MIT license, Poppins and Inter font licenses/attribution inventories, and a locked dependency inventory with source notices and hashes. `BUILD-INFO.json` records version, profile, target, Git revision, and whether the working tree was dirty. The packager verifies archive paths, executable mode, and included dependency notices before publishing the final checksum.

Archive timestamps default to zero; set `SOURCE_DATE_EPOCH` to a nonnegative Unix timestamp to override them. This stabilizes archive metadata but does not guarantee bit-identical Rust binaries across compilers or hosts.

## Dependency notices

`check` validates packaging inputs and verifies that every package in the locked application's native-target dependency closure has source license texts. Normal and build dependencies are included; development-only and other-platform-only packages are excluded. The inventory preserves upstream SPDX declarations rather than choosing or changing dependency licenses.

Published Cargo sources provide most notices. Dependencies that omit their license files use reviewed [pinned upstream snapshots](../packaging/licenses/README.md). For a dependency update that introduces a missing license, run:

```sh
cargo xtask notices --fetch-missing
```

Review and commit changes under `packaging/licenses/`. Each downloaded file is pinned to an immutable original repository revision and hash. Packaging fails rather than silently generating a generic replacement license. The regular commands never fetch missing license snapshots, although Cargo may need its usual network access when dependencies have not been cached.

## Install for the current user

Verify the archive checksum, extract it into a directory you control, and run its `bin/feishin-rs` executable. Install MPV 0.41 or newer separately. Set its executable in the application settings when it is not available on `PATH`.

For desktop integration, copy the executable to `~/.local/bin/feishin-rs`, the supplied desktop entry to `~/.local/share/applications/feishin-rs.desktop`, and the SVG to `~/.local/share/icons/hicolor/scalable/apps/feishin-rs.svg`. Create the directories first and ensure `~/.local/bin` is on your desktop session's `PATH`. The archive does not perform these steps or modify the system automatically.

The packager does not create Debian/RPM installers, AppImages, Windows installers, or macOS bundles. Cross-distribution compatibility, signing, and update delivery require separate validation. This remote server can validate native rendering through Xvfb and null-audio MPV behavior; it does not establish playback on a user's physical device.
