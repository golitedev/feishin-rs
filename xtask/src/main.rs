//! Project-owned Linux packaging and notice inventory, implemented in Rust.
mod notices;

use flate2::{Compression, GzBuilder, read::GzDecoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
};
use tar::{Archive, Builder, Header};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
    target_directory: PathBuf,
    resolve: Resolve,
}
#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    id: String,
    license: Option<String>,
    license_file: Option<PathBuf>,
    manifest_path: PathBuf,
    repository: Option<String>,
    source: Option<String>,
}
#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<Dependency>,
}
#[derive(Deserialize)]
struct Dependency {
    pkg: String,
    dep_kinds: Vec<DependencyKind>,
}
#[derive(Deserialize)]
struct DependencyKind {
    kind: Option<String>,
}

#[derive(Clone, Serialize)]
struct Entry {
    path: String,
    #[serde(skip)]
    bytes: Vec<u8>,
    sha256: String,
    origin: String,
}
impl Entry {
    fn new(path: String, bytes: Vec<u8>, origin: String) -> Self {
        let sha256 = hex_hash(&bytes);
        Self {
            path,
            bytes,
            sha256,
            origin,
        }
    }
}
#[derive(Serialize)]
struct InventoryPackage {
    name: String,
    version: String,
    license: String,
    repository: Option<String>,
    source: String,
    license_files: Vec<Entry>,
}
#[derive(Serialize)]
struct Inventory {
    schema: u32,
    target: String,
    packages: Vec<InventoryPackage>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    let flags = args.collect::<Vec<_>>();
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!(
            "cargo xtask check\ncargo xtask notices [--fetch-missing]\ncargo xtask package [--debug]\ncargo xtask icon\n\nNotice generation fails if a dependency has no source license text.\n--fetch-missing downloads pinned upstream license snapshots for review.\nMPV is required separately and is never bundled."
        );
        return Ok(());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no workspace parent")?
        .to_path_buf();
    if command == "icon" {
        if !flags.is_empty() {
            return Err("icon takes no flags".into());
        }
        fs::write(root.join("packaging/linux/feishin-rs.svg"), icon_svg())?;
        println!("Generated packaging/linux/feishin-rs.svg");
        return Ok(());
    }
    match command.as_str() {
        "check" if flags.is_empty() => {
            let metadata = metadata(&root)?;
            check_assets(&root)?;
            let inventory = notices::collect(&metadata, false)?;
            println!(
                "Packaging inputs and source-license texts checked: {} dependency packages ({})",
                inventory.packages.len(),
                inventory.target
            );
        }
        "notices" if flags.iter().all(|flag| flag == "--fetch-missing") => {
            let metadata = metadata(&root)?;
            check_assets(&root)?;
            let inventory = notices::collect(
                &metadata,
                flags.iter().any(|flag| flag == "--fetch-missing"),
            )?;
            let output = root.join("dist/notices");
            write_notices(&output, &inventory)?;
            println!(
                "Wrote {} dependency inventories to {}",
                inventory.packages.len(),
                output.display()
            );
        }
        "package" if flags.iter().all(|flag| flag == "--debug") => {
            package(&root, flags.iter().any(|flag| flag == "--debug"))?
        }
        _ => return Err("Unknown command or flag. Run cargo xtask --help".into()),
    }
    Ok(())
}

fn host_target() -> Result<String> {
    let output = Command::new("rustc").arg("-vV").output()?;
    if !output.status.success() {
        return Err("rustc -vV failed".into());
    }
    String::from_utf8(output.stdout)?
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or_else(|| "rustc did not report its host target".into())
}
fn metadata(root: &Path) -> Result<Metadata> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            &host_target()?,
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        io::stderr().write_all(&output.stderr)?;
        return Err(
            "cargo metadata --locked failed; update Cargo.lock deliberately before packaging"
                .into(),
        );
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn application(metadata: &Metadata) -> Result<&Package> {
    metadata
        .packages
        .iter()
        .find(|package| {
            package.name == "feishin-rs" && metadata.workspace_members.contains(&package.id)
        })
        .ok_or_else(|| "Workspace application package feishin-rs was not found".into())
}

fn check_assets(root: &Path) -> Result<()> {
    for path in [
        "LICENSE",
        "packaging/linux/feishin-rs.desktop",
        "packaging/linux/feishin-rs.svg",
        "crates/ui-egui/assets/fonts/OFL.txt",
        "crates/ui-egui/assets/fonts/inter/OFL.txt",
    ] {
        if fs::read(root.join(path))?.is_empty() {
            return Err(format!("Required packaging input is empty: {path}").into());
        }
    }
    if fs::read_to_string(root.join("packaging/linux/feishin-rs.svg"))? != icon_svg() {
        return Err(
            "Original SVG icon differs from its Rust generator; run cargo xtask icon".into(),
        );
    }
    Ok(())
}

fn write_notices(output: &Path, inventory: &Inventory) -> Result<()> {
    fs::create_dir_all(output)?;
    fs::write(
        output.join("inventory.json"),
        serde_json::to_vec_pretty(inventory)?,
    )?;
    let mut summary = String::from(
        "# Third-party dependency source notices\n\nGenerated from cargo metadata --locked for the Linux application dependency closure, including build dependencies. Development-only and other-platform-only packages are excluded. This inventory preserves declared SPDX expressions; it does not relicense dependencies.\n\n",
    );
    for package in &inventory.packages {
        summary.push_str(&format!(
            "## {} {}\n\nLicense: `{}`\n\n",
            package.name, package.version, package.license
        ));
        for file in &package.license_files {
            let destination = output
                .join("crates")
                .join(format!("{}-{}", package.name, package.version))
                .join(&file.path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(destination, &file.bytes)?;
            summary.push_str(&format!("- {} (SHA-256 `{}`)\n", file.path, file.sha256));
        }
        summary.push('\n');
    }
    fs::write(output.join("THIRD-PARTY-NOTICES.md"), summary)?;
    Ok(())
}

fn package(root: &Path, debug: bool) -> Result<()> {
    if env::consts::OS != "linux" {
        return Err("This packager currently supports native Linux builds only".into());
    }
    let metadata = metadata(root)?;
    check_assets(root)?;
    let inventory = notices::collect(&metadata, false)?;
    let mut command = Command::new("cargo");
    command.args(["build", "--locked", "-p", "feishin-rs"]);
    if !debug {
        command.arg("--release");
    }
    if !command.current_dir(root).status()?.success() {
        return Err("Application build failed; no package was produced".into());
    }
    let profile = if debug { "debug" } else { "release" };
    let binary = metadata.target_directory.join(profile).join("feishin-rs");
    let version = &application(&metadata)?.version;
    let suffix = if debug { "-debug" } else { "" };
    let label = format!("feishin-rs-{version}-linux-{}{suffix}", env::consts::ARCH);
    let dist = root.join("dist");
    fs::create_dir_all(&dist)?;
    let temporary = dist.join(format!("{label}.tar.gz.tmp"));
    let output = dist.join(format!("{label}.tar.gz"));
    let timestamp = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(0);
    let encoder = GzBuilder::new()
        .mtime(
            u32::try_from(timestamp)
                .map_err(|_| "SOURCE_DATE_EPOCH exceeds gzip's timestamp range")?,
        )
        .write(fs::File::create(&temporary)?, Compression::default());
    let mut archive = Builder::new(encoder);
    append_file(
        &mut archive,
        &format!("{label}/bin/feishin-rs"),
        &binary,
        0o755,
        timestamp,
    )?;
    for (source, destination) in [
        ("LICENSE", "LICENSE"),
        ("README.md", "README.md"),
        ("THIRD_PARTY.md", "THIRD_PARTY.md"),
        ("AGENTS.md", "AGENTS.md"),
        ("CONTRIBUTING.md", "CONTRIBUTING.md"),
        ("Cargo.lock", "Cargo.lock"),
        (
            "packaging/licenses/README.md",
            "packaging/licenses/README.md",
        ),
        (
            "crates/ui-egui/assets/fonts/OFL.txt",
            "crates/ui-egui/assets/fonts/OFL.txt",
        ),
        (
            "crates/ui-egui/assets/fonts/README.md",
            "crates/ui-egui/assets/fonts/README.md",
        ),
        (
            "crates/ui-egui/assets/fonts/inter/OFL.txt",
            "crates/ui-egui/assets/fonts/inter/OFL.txt",
        ),
        (
            "crates/ui-egui/assets/fonts/inter/README.md",
            "crates/ui-egui/assets/fonts/inter/README.md",
        ),
        (
            "packaging/linux/feishin-rs.desktop",
            "share/applications/feishin-rs.desktop",
        ),
        (
            "packaging/linux/feishin-rs.svg",
            "share/icons/hicolor/scalable/apps/feishin-rs.svg",
        ),
        (
            "crates/ui-egui/assets/fonts/OFL.txt",
            "share/licenses/fonts/Poppins-OFL.txt",
        ),
        (
            "crates/ui-egui/assets/fonts/inter/OFL.txt",
            "share/licenses/fonts/Inter-OFL.txt",
        ),
        (
            "crates/ui-egui/assets/fonts/README.md",
            "share/licenses/fonts/Poppins-inventory.md",
        ),
        (
            "crates/ui-egui/assets/fonts/inter/README.md",
            "share/licenses/fonts/Inter-inventory.md",
        ),
    ] {
        append_file(
            &mut archive,
            &format!("{label}/{destination}"),
            &root.join(source),
            0o644,
            timestamp,
        )?;
    }
    let mut documents = fs::read_dir(root.join("docs"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    documents.sort();
    for document in documents {
        if document.is_file()
            && document
                .extension()
                .is_some_and(|extension| extension == "md")
        {
            let name = document
                .file_name()
                .ok_or("Documentation file has no name")?
                .to_string_lossy();
            append_file(
                &mut archive,
                &format!("{label}/docs/{name}"),
                &document,
                0o644,
                timestamp,
            )?;
        }
    }
    let generated = serde_json::to_vec_pretty(&inventory)?;
    append_bytes(
        &mut archive,
        &format!("{label}/share/licenses/dependencies.json"),
        &generated,
        0o644,
        timestamp,
    )?;
    for package in &inventory.packages {
        for file in &package.license_files {
            append_bytes(
                &mut archive,
                &format!(
                    "{label}/share/licenses/crates/{}-{}/{}",
                    package.name, package.version, file.path
                ),
                &file.bytes,
                0o644,
                timestamp,
            )?;
        }
    }
    let readme = "feishin-rs native Linux archive\n\nRun bin/feishin-rs. Install MPV 0.41 or newer separately and configure its executable path in Settings. MPV is not included. The archive does not install desktop files or modify your system. To integrate it, place the binary on your PATH, and copy the desktop file and SVG icon from share/ to the corresponding user data directories.\n\nThis build is dynamically linked to the development host's system libraries. Compatibility with other distributions has not been validated. Debug archives are development artifacts, not release builds.\n\nOriginal application code is MIT. Dependencies and embedded fonts retain their own licenses in share/licenses. The dependency inventory includes build-time packages and is generated from the locked Linux dependency closure.\n";
    append_bytes(
        &mut archive,
        &format!("{label}/INSTALL.txt"),
        readme.as_bytes(),
        0o644,
        timestamp,
    )?;
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|result| result.status.success())
        .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_owned());
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .map(|result| !result.stdout.is_empty())
        .unwrap_or(true);
    let info = serde_json::json!({"version":version,"profile":profile,"target":inventory.target,"git_commit":commit,"working_tree_dirty":dirty,"mpv_bundled":false,"dependency_packages":inventory.packages.len()});
    append_bytes(
        &mut archive,
        &format!("{label}/BUILD-INFO.json"),
        &serde_json::to_vec_pretty(&info)?,
        0o644,
        timestamp,
    )?;
    archive.into_inner()?.finish()?.sync_all()?;
    verify_archive(&temporary, &label, &inventory)?;
    fs::rename(&temporary, &output)?;
    let digest = hash_file(&output)?;
    fs::write(
        dist.join(format!("{label}.tar.gz.sha256")),
        format!("{digest}  {label}.tar.gz\n"),
    )?;
    println!(
        "Packaged {} dependencies into {}\nSHA-256 {digest}",
        inventory.packages.len(),
        output.display()
    );
    Ok(())
}
fn append_file<W: Write>(
    archive: &mut Builder<W>,
    path: &str,
    source: &Path,
    mode: u32,
    timestamp: u64,
) -> Result<()> {
    let mut file = fs::File::open(source)?;
    let mut header = Header::new_gnu();
    header.set_size(file.metadata()?.len());
    header.set_mode(mode);
    header.set_mtime(timestamp);
    header.set_uid(0);
    header.set_gid(0);
    header.set_cksum();
    archive.append_data(&mut header, path, &mut file)?;
    Ok(())
}
fn append_bytes<W: Write>(
    archive: &mut Builder<W>,
    path: &str,
    bytes: &[u8],
    mode: u32,
    timestamp: u64,
) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(mode);
    header.set_mtime(timestamp);
    header.set_uid(0);
    header.set_gid(0);
    header.set_cksum();
    archive.append_data(&mut header, path, bytes)?;
    Ok(())
}
fn verify_archive(path: &Path, label: &str, inventory: &Inventory) -> Result<()> {
    let mut archive = Archive::new(GzDecoder::new(fs::File::open(path)?));
    let mut entries = BTreeSet::new();
    let mut has_executable = false;
    let mut expected_licenses = BTreeMap::new();
    for package in &inventory.packages {
        for notice in &package.license_files {
            expected_licenses.insert(
                format!(
                    "{label}/share/licenses/crates/{}-{}/{}",
                    package.name, package.version, notice.path
                ),
                &notice.sha256,
            );
        }
    }
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        validate_relative(Path::new(&path))?;
        if !entries.insert(path.clone()) {
            return Err(format!("Archive contains duplicate path: {path}").into());
        }
        if path == format!("{label}/bin/feishin-rs") {
            has_executable = entry.header().mode()? & 0o111 != 0;
        }
        if let Some(expected) = expected_licenses.remove(&path) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            if hex_hash(&bytes) != *expected {
                return Err(format!("Archive notice hash mismatch: {path}").into());
            }
        }
    }
    let required = [
        "LICENSE",
        "BUILD-INFO.json",
        "share/applications/feishin-rs.desktop",
        "share/icons/hicolor/scalable/apps/feishin-rs.svg",
        "share/licenses/fonts/Poppins-OFL.txt",
        "share/licenses/fonts/Inter-OFL.txt",
        "share/licenses/dependencies.json",
    ];
    if !has_executable
        || !expected_licenses.is_empty()
        || required
            .iter()
            .any(|path| !entries.contains(&format!("{label}/{path}")))
    {
        return Err("Archive verification failed: missing executable or dependency notices".into());
    }
    Ok(())
}
fn validate_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Unsafe package path".into());
    }
    Ok(())
}
fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path) -> Result<String> {
    let mut source = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let size = source.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn icon_svg() -> String {
    let mut svg = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 128 128\" role=\"img\" aria-label=\"feishin-rs music player\">\n<rect width=\"128\" height=\"128\" rx=\"28\" fill=\"#101114\"/>\n",
    );
    for (radius, opacity) in [(44, "0.25"), (34, "0.45"), (24, "0.8")] {
        svg.push_str(&format!("<circle cx=\"64\" cy=\"64\" r=\"{radius}\" fill=\"none\" stroke=\"#3279ff\" stroke-width=\"4\" opacity=\"{opacity}\"/>\n"));
    }
    svg.push_str("<path d=\"M57 47v34l28-17z\" fill=\"#ffffff\"/>\n</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_roundtrip_preserves_bytes_modes_and_paths() {
        let mut bytes = Vec::new();
        {
            let mut builder = Builder::new(&mut bytes);
            append_bytes(
                &mut builder,
                "app/bin/player",
                b"original fixture",
                0o755,
                0,
            )
            .unwrap();
            builder.finish().unwrap();
        }
        let mut archive = Archive::new(bytes.as_slice());
        let mut entry = archive.entries().unwrap().next().unwrap().unwrap();
        assert_eq!(entry.path().unwrap(), Path::new("app/bin/player"));
        assert_eq!(entry.header().mode().unwrap(), 0o755);
        let mut actual = String::new();
        entry.read_to_string(&mut actual).unwrap();
        assert_eq!(actual, "original fixture");
    }
    #[test]
    fn package_paths_reject_absolute_and_parent_components() {
        for path in ["/tmp/file", "../file", "app/../../file", ""] {
            assert!(validate_relative(Path::new(path)).is_err());
        }
        assert!(validate_relative(Path::new("app/share/LICENSE")).is_ok());
    }
    #[test]
    fn archive_verification_rejects_modified_dependency_notice() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("fixture.tar.gz");
        let encoder =
            GzBuilder::new().write(fs::File::create(&output).unwrap(), Compression::default());
        let mut archive = Builder::new(encoder);
        append_bytes(
            &mut archive,
            "app/share/licenses/crates/fixture-1.0.0/LICENSE",
            b"modified notice",
            0o644,
            0,
        )
        .unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        let inventory = Inventory {
            schema: 1,
            target: "fixture".into(),
            packages: vec![InventoryPackage {
                name: "fixture".into(),
                version: "1.0.0".into(),
                license: "MIT".into(),
                repository: None,
                source: "fixture".into(),
                license_files: vec![Entry::new(
                    "LICENSE".into(),
                    b"original notice".to_vec(),
                    "fixture".into(),
                )],
            }],
        };
        let error = verify_archive(&output, "app", &inventory).unwrap_err();
        assert!(error.to_string().contains("notice hash mismatch"));
    }
}
