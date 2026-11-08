//! Versioned, credential-free application preferences and atomic persistence.
use feishin_core::{Queue, ServerProfile};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Cannot access the settings directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("Settings are not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Settings use an unsupported version ({0}); the original file has been preserved")]
    Version(u32),
    #[error("Cannot determine the settings directory; set FEISHIN_RS_CONFIG_DIR")]
    NoDirectory,
    #[error("Settings file exceeds the 8 MiB limit")]
    TooLarge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub server: Option<ServerProfile>,
    pub mpv_path: String,
    pub volume: f64,
    pub queue: Queue,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            server: None,
            mpv_path: "mpv".into(),
            volume: 75.0,
            queue: Queue::default(),
        }
    }
}

pub fn config_dir() -> Result<PathBuf, Error> {
    if let Some(path) = std::env::var_os("FEISHIN_RS_CONFIG_DIR").filter(|s| !s.is_empty()) {
        return Ok(path.into());
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(|p| PathBuf::from(p).join("feishin-rs"))
            .ok_or(Error::NoDirectory)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/feishin-rs"))
            .ok_or(Error::NoDirectory)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
            return Ok(PathBuf::from(path).join("feishin-rs"));
        }
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join(".config/feishin-rs"))
            .ok_or(Error::NoDirectory)
    }
}

pub fn load(dir: &Path) -> Result<Settings, Error> {
    let path = dir.join("settings.json");
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => return Err(e.into()),
    };
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    let mut settings: Settings = serde_json::from_slice(&bytes)?;
    if settings.version != 1 {
        return Err(Error::Version(settings.version));
    }
    settings.volume = if settings.volume.is_finite() {
        settings.volume.clamp(0.0, 100.0)
    } else {
        75.0
    };
    settings.queue.validate();
    Ok(settings)
}

pub fn save(dir: &Path, settings: &Settings) -> Result<(), Error> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    let mut staged = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(&mut staged, settings)?;
    staged.write_all(b"\n")?;
    staged.as_file().sync_all()?;
    staged
        .persist(dir.join("settings.json"))
        .map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_has_no_credentials_and_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            server: Some(ServerProfile {
                name: "Test".into(),
                url: "https://music.example".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        save(dir.path(), &settings).unwrap();
        assert_eq!(load(dir.path()).unwrap().server, settings.server);
        let json = fs::read_to_string(dir.path().join("settings.json")).unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("token"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.path().join("settings.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn corrupted_or_future_settings_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"{broken").unwrap();
        assert!(load(dir.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::write(&path, br#"{"version":99}"#).unwrap();
        assert!(matches!(load(dir.path()), Err(Error::Version(99))));
        assert_eq!(load(&dir.path().join("missing")).unwrap().volume, 75.0);
    }
}
