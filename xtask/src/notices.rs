use super::*;
use serde_json::Value;
use std::{collections::HashMap, time::Duration};

#[derive(Serialize, Deserialize)]
struct Snapshot {
    package: String,
    version: String,
    repository: String,
    revision: String,
    #[serde(default)]
    license_revision: String,
    #[serde(default)]
    license_reference: String,
    files: Vec<SnapshotFile>,
}
#[derive(Serialize, Deserialize)]
struct SnapshotFile {
    path: String,
    url: String,
    sha256: String,
}

pub(super) fn collect(metadata: &Metadata, fetch: bool) -> Result<Inventory> {
    let packages = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<HashMap<_, _>>();
    let nodes = metadata
        .resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut pending = vec![application(metadata)?.id.as_str()];
    let mut selected = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !selected.insert(id) {
            continue;
        }
        let node = nodes
            .get(id)
            .ok_or("Cargo dependency resolution node is missing")?;
        for dependency in &node.deps {
            if dependency
                .dep_kinds
                .iter()
                .any(|kind| kind.kind.as_deref() != Some("dev"))
            {
                pending.push(dependency.pkg.as_str());
            }
        }
    }
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("feishin-rs-license-inventory/0.1")
        .build()?;
    let mut downloads = HashMap::<String, Option<Vec<u8>>>::new();
    let mut inventory = Vec::new();
    let mut missing = Vec::new();
    for id in selected {
        if metadata.workspace_members.iter().any(|member| member == id) {
            continue;
        }
        let package = packages
            .get(id)
            .ok_or("Cargo package metadata is missing")?;
        let root = package
            .manifest_path
            .parent()
            .ok_or("Package manifest has no parent")?;
        let license = package
            .license
            .as_ref()
            .filter(|license| !license.is_empty())
            .ok_or_else(|| {
                format!(
                    "{} {} declares no SPDX license; review it before packaging",
                    package.name, package.version
                )
            })?;
        let source = package.source.as_ref().ok_or_else(|| {
            format!(
                "{} {} has an untracked path source; explicit provenance is required",
                package.name, package.version
            )
        })?;
        let mut files = Vec::new();
        scan(root, root, &mut files)?;
        let license_file = package.license_file.as_ref().map(|file| {
            if file.is_absolute() {
                file.clone()
            } else {
                root.join(file)
            }
        });
        if let Some(file) = license_file.as_ref() {
            if !file.starts_with(root) {
                return Err(format!(
                    "{} license-file points outside its source directory",
                    package.name
                )
                .into());
            }
            if !files.contains(file) {
                if !file.is_file() {
                    return Err(format!(
                        "{} license-file is missing: {}",
                        package.name,
                        file.display()
                    )
                    .into());
                }
                files.push(file.clone());
            }
        }
        let code_license = files.iter().any(|file| {
            Some(file) == license_file.as_ref()
                || file.parent() == Some(root)
                || file
                    .strip_prefix(root)
                    .ok()
                    .and_then(|path| path.components().next())
                    .is_some_and(|component| {
                        matches!(component, Component::Normal(name)
                        if name.to_string_lossy().to_ascii_lowercase().starts_with("license"))
                    })
        });
        let mut entries = files
            .into_iter()
            .map(|file| {
                let relative = file
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                validate_relative(Path::new(&relative))?;
                let bytes = read_text(&file)?;
                Ok(Entry::new(
                    relative,
                    bytes,
                    format!("Cargo package source ({source})"),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        if !code_license {
            let directory = metadata
                .workspace_root
                .join("packaging/licenses")
                .join(format!("{}-{}", package.name, package.version));
            if !directory.join("provenance.json").is_file() && fetch {
                fetch_snapshot(package, root, &directory, &http, &mut downloads)?;
            }
            if directory.join("provenance.json").is_file() {
                entries.extend(read_snapshot(package, root, &directory)?);
            } else {
                missing.push(format!("{} {} ({license})", package.name, package.version));
                continue;
            }
        }
        if entries.is_empty() {
            return Err(format!(
                "{} {} has no license/notice texts",
                package.name, package.version
            )
            .into());
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        inventory.push(InventoryPackage {
            name: package.name.clone(),
            version: package.version.clone(),
            license: license.clone(),
            repository: package.repository.clone(),
            source: source.clone(),
            license_files: entries,
        });
    }
    if !missing.is_empty() {
        return Err(format!("Missing source license texts for {} packages:\n  {}\nRun cargo xtask notices --fetch-missing to fetch pinned upstream license snapshots, then review and commit packaging/licenses. No package was produced.",missing.len(),missing.join("\n  ")).into());
    }
    inventory.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.version.cmp(&right.version))
    });
    Ok(Inventory {
        schema: 1,
        target: host_target()?,
        packages: inventory,
    })
}

fn scan(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if !matches!(entry.file_name().to_str(), Some(".git" | "target")) {
                scan(root, &path, files)?;
            }
        } else if kind.is_file() && is_notice(&path, root) {
            files.push(path);
        }
    }
    Ok(())
}
fn is_notice(path: &Path, root: &Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let font = path.strip_prefix(root).ok().is_some_and(|path| {
        path.components()
            .any(|component| matches!(component,Component::Normal(part) if part=="fonts"))
    });
    name.contains("license")
        || name.contains("licence")
        || ["copying", "copyright", "notice", "unlicense"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        || matches!(name.as_str(), "ofl.txt" | "ufl.txt")
        || (font && name.ends_with(".txt"))
}
fn read_text(path: &Path) -> Result<Vec<u8>> {
    if fs::metadata(path)?.len() > 2 * 1024 * 1024 {
        return Err(format!("License/notice file exceeds 2 MiB: {}", path.display()).into());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Err(format!("License/notice file is empty: {}", path.display()).into());
    }
    if bytes.contains(&0) {
        return Err(format!("License/notice file is binary: {}", path.display()).into());
    }
    Ok(bytes)
}

fn source_revision(root: &Path) -> Result<String> {
    let value: Value = serde_json::from_slice(&fs::read(root.join(".cargo_vcs_info.json"))?)?;
    let revision = value
        .pointer("/git/sha1")
        .and_then(Value::as_str)
        .filter(|revision| {
            revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .ok_or("Package source has no pinned 40-character VCS revision")?;
    Ok(revision.to_owned())
}
fn github_repository(package: &Package) -> Result<String> {
    let repository = package.repository.as_ref().ok_or_else(|| {
        format!(
            "{} has no repository for missing-license provenance",
            package.name
        )
    })?;
    let url = reqwest::Url::parse(repository)?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(format!(
            "{} requires a manually reviewed pinned license snapshot from its original repository",
            package.name
        )
        .into());
    }
    let original = url.path().trim_matches('/').trim_end_matches(".git");
    let parts: Vec<_> = original.split('/').collect();
    if parts.len() < 2
        || parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
        || parts.len() > 2 && !matches!(parts.get(2), Some(&"tree" | &"blob"))
    {
        return Err("GitHub repository must be an owner/project URL".into());
    }
    let path = format!("{}/{}", parts[0], parts[1]);
    Ok(format!("https://github.com/{path}"))
}
fn read_snapshot(package: &Package, root: &Path, directory: &Path) -> Result<Vec<Entry>> {
    let provenance: Snapshot =
        serde_json::from_slice(&fs::read(directory.join("provenance.json"))?)?;
    if provenance.package != package.name
        || provenance.version != package.version
        || provenance.revision != source_revision(root)?
        || provenance.repository != github_repository(package)?
    {
        return Err(format!(
            "Pinned license snapshot provenance mismatch for {} {}",
            package.name, package.version
        )
        .into());
    }
    if provenance.files.is_empty() {
        return Err("Pinned license snapshot has no files".into());
    }
    if !provenance.license_revision.is_empty()
        && (provenance.license_revision.len() != 40
            || !provenance
                .license_revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("Pinned license revision must be a full commit SHA".into());
    }
    let expected_reference = format!(
        "https://api.github.com/repos/{}/git/ref/tags/{}",
        provenance
            .repository
            .trim_start_matches("https://github.com/"),
        package.version
    );
    if (!provenance.license_reference.is_empty()
        && provenance.license_reference != expected_reference)
        || (provenance.license_revision != provenance.revision
            && !provenance.license_revision.is_empty()
            && provenance.license_reference.is_empty())
    {
        return Err("Alternate license revision must record the exact package release tag".into());
    }
    let mut files = Vec::new();
    let mut paths = BTreeSet::new();
    let prefix = format!(
        "https://raw.githubusercontent.com/{}/{}/",
        provenance
            .repository
            .trim_start_matches("https://github.com/"),
        if provenance.license_revision.is_empty() {
            &provenance.revision
        } else {
            &provenance.license_revision
        }
    );
    for source in provenance.files {
        validate_relative(Path::new(&source.path))?;
        if !paths.insert(source.path.clone()) {
            return Err("Pinned license snapshot has duplicate paths".into());
        }
        if source.url != format!("{prefix}{}", source.path) {
            return Err("Pinned license URL does not match the package repository/revision".into());
        }
        let bytes = read_text(&directory.join(&source.path))?;
        if hex_hash(&bytes) != source.sha256 {
            return Err(format!(
                "Pinned license hash mismatch: {} {}",
                package.name, source.path
            )
            .into());
        }
        files.push(Entry::new(
            format!("upstream/{}", source.path),
            bytes,
            source.url,
        ));
    }
    Ok(files)
}
fn fetch_snapshot(
    package: &Package,
    root: &Path,
    directory: &Path,
    http: &reqwest::blocking::Client,
    cache: &mut HashMap<String, Option<Vec<u8>>>,
) -> Result<()> {
    let repository = github_repository(package)?;
    let revision = source_revision(root)?;
    let mut license_revision = revision.clone();
    let mut license_reference = String::new();
    // Some monorepo packages were published from a revision which is no longer
    // reachable upstream. Use that exact version's immutable release tag for
    // license texts, preserving both the package and license source revisions.
    let probe = format!(
        "https://raw.githubusercontent.com/{}/{revision}/LICENSE-MIT",
        repository.trim_start_matches("https://github.com/")
    );
    if package
        .license
        .as_deref()
        .is_some_and(|license| license.contains("MIT"))
        && fetch_url(http, &probe, cache)?.is_none()
    {
        let api = format!(
            "https://api.github.com/repos/{}/git/ref/tags/{}",
            repository.trim_start_matches("https://github.com/"),
            package.version
        );
        let response = http.get(&api).send()?;
        if response.status().is_success() {
            let mut value: Value = response.json()?;
            for _ in 0..3 {
                let object = value.get("object").ok_or("Release tag has no object")?;
                let sha = object
                    .get("sha")
                    .and_then(Value::as_str)
                    .ok_or("Release tag has no SHA")?
                    .to_owned();
                if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("Release tag has an invalid SHA".into());
                }
                if object.get("type").and_then(Value::as_str) == Some("commit") {
                    license_revision = sha;
                    license_reference = api.clone();
                    break;
                }
                value = http
                    .get(format!(
                        "https://api.github.com/repos/{}/git/tags/{sha}",
                        repository.trim_start_matches("https://github.com/")
                    ))
                    .send()?
                    .error_for_status()?
                    .json()?;
            }
        }
    }
    let base = format!(
        "https://raw.githubusercontent.com/{}/{license_revision}",
        repository.trim_start_matches("https://github.com/")
    );
    let license = package.license.as_deref().unwrap_or_default();
    let mut groups = Vec::<Vec<&str>>::new();
    if license.contains("MIT") {
        groups.push(vec![
            "LICENSE-MIT",
            "LICENSE-MIT.txt",
            "LICENSE-MIT.md",
            "LICENSE",
            "LICENSE.txt",
            "LICENSE.md",
        ]);
    }
    if license.contains("Apache-2.0") {
        groups.push(vec![
            "LICENSE-APACHE",
            "LICENSE-APACHE-2.0",
            "LICENSE-APACHE.txt",
            "LICENSE",
            "LICENSE.txt",
            "LICENSE.md",
        ]);
    }
    if groups.is_empty() {
        groups.push(vec![
            "LICENSE",
            "LICENSE.txt",
            "LICENSE.md",
            "COPYING",
            "LICENSE_1_0.txt",
        ]);
    }
    let mut fetched = BTreeMap::<String, (String, Vec<u8>)>::new();
    for group in groups {
        let mut found = false;
        for candidate in group {
            let url = format!("{base}/{candidate}");
            if let Some(bytes) = fetch_url(http, &url, cache)? {
                fetched.insert(candidate.to_owned(), (url, bytes));
                found = true;
                break;
            }
        }
        if !found {
            return Err(format!("Could not find original pinned {} license text for {} {}; add a reviewed snapshot instead of substituting a generic license",license,package.name,package.version).into());
        }
    }
    for name in ["NOTICE", "NOTICE.txt", "COPYRIGHT"] {
        let url = format!("{base}/{name}");
        if let Some(bytes) = fetch_url(http, &url, cache)? {
            fetched.insert(name.to_owned(), (url, bytes));
        }
    }
    fs::create_dir_all(directory)?;
    let mut files = Vec::new();
    for (path, (url, bytes)) in fetched {
        fs::write(directory.join(&path), &bytes)?;
        files.push(SnapshotFile {
            path,
            url,
            sha256: hex_hash(&bytes),
        });
    }
    let provenance = Snapshot {
        package: package.name.clone(),
        version: package.version.clone(),
        repository,
        revision,
        license_revision,
        license_reference,
        files,
    };
    fs::write(
        directory.join("provenance.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    println!(
        "Pinned original license texts for {} {}",
        package.name, package.version
    );
    Ok(())
}
fn fetch_url(
    http: &reqwest::blocking::Client,
    url: &str,
    cache: &mut HashMap<String, Option<Vec<u8>>>,
) -> Result<Option<Vec<u8>>> {
    if let Some(result) = cache.get(url) {
        return Ok(result.clone());
    }
    let response = http.get(url).send()?;
    let result = if response.status() == reqwest::StatusCode::NOT_FOUND {
        None
    } else {
        if !response.status().is_success() {
            return Err(format!(
                "License fetch failed with HTTP {}: {url}",
                response.status().as_u16()
            )
            .into());
        }
        let mut bytes = Vec::new();
        response.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.is_empty() || bytes.len() > 2 * 1024 * 1024 || bytes.contains(&0) {
            return Err(
                format!("Pinned license source is empty, oversized, or binary: {url}").into(),
            );
        }
        Some(bytes)
    };
    cache.insert(url.to_owned(), result.clone());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notice_discovery_includes_embedded_font_licenses_but_not_font_binary() {
        let root = Path::new("crate");
        for path in [
            "crate/LICENSE-MIT",
            "crate/third-party/NOTICE",
            "crate/fonts/OFL.txt",
            "crate/fonts/Hack-Regular.txt",
        ] {
            assert!(is_notice(Path::new(path), root));
        }
        assert!(!is_notice(Path::new("crate/fonts/Hack-Regular.ttf"), root));
    }
    #[test]
    fn pinned_snapshot_rejects_changed_license_bytes() {
        let source = tempfile::tempdir().unwrap();
        let snapshot = tempfile::tempdir().unwrap();
        fs::write(
            source.path().join(".cargo_vcs_info.json"),
            r#"{"git":{"sha1":"0123456789012345678901234567890123456789"}}"#,
        )
        .unwrap();
        let package = Package {
            name: "fixture".into(),
            version: "1.0.0".into(),
            id: "fixture".into(),
            license: Some("MIT".into()),
            license_file: None,
            manifest_path: source.path().join("Cargo.toml"),
            repository: Some("https://github.com/example/fixture".into()),
            source: Some("fixture registry".into()),
        };
        let original = b"original fixture license";
        fs::write(snapshot.path().join("LICENSE"), original).unwrap();
        let mut provenance = Snapshot {
            package: package.name.clone(),
            version: package.version.clone(),
            repository: package.repository.clone().unwrap(),
            revision: "0123456789012345678901234567890123456789".into(),
            license_revision: String::new(),
            license_reference: String::new(),
            files: vec![SnapshotFile {
                path: "LICENSE".into(),
                url: "https://raw.githubusercontent.com/example/fixture/0123456789012345678901234567890123456789/LICENSE".into(),
                sha256: hex_hash(original),
            }],
        };
        fs::write(
            snapshot.path().join("provenance.json"),
            serde_json::to_vec(&provenance).unwrap(),
        )
        .unwrap();
        assert!(read_snapshot(&package, source.path(), snapshot.path()).is_ok());
        provenance.license_revision = "main".into();
        fs::write(
            snapshot.path().join("provenance.json"),
            serde_json::to_vec(&provenance).unwrap(),
        )
        .unwrap();
        assert!(read_snapshot(&package, source.path(), snapshot.path()).is_err());
        provenance.license_revision = "abcdef0123456789012345678901234567890123".into();
        fs::write(
            snapshot.path().join("provenance.json"),
            serde_json::to_vec(&provenance).unwrap(),
        )
        .unwrap();
        assert!(
            read_snapshot(&package, source.path(), snapshot.path())
                .err()
                .unwrap()
                .to_string()
                .contains("release tag")
        );
        provenance.license_revision.clear();
        fs::write(
            snapshot.path().join("provenance.json"),
            serde_json::to_vec(&provenance).unwrap(),
        )
        .unwrap();
        fs::write(snapshot.path().join("LICENSE"), b"changed").unwrap();
        assert!(read_snapshot(&package, source.path(), snapshot.path()).is_err());
    }
}
