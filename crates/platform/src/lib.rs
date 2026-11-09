//! Versioned, credential-free application preferences and atomic persistence.
pub mod credentials;
pub mod diagnostics;
pub mod notifications;
use feishin_core::preferences::Preferences;
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
    #[error("Settings contain an invalid server profile; embedded URL credentials are not allowed")]
    InvalidProfile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub server: Option<ServerProfile>,
    pub mpv_path: String,
    pub volume: f64,
    pub queue: Queue,
    pub servers: Vec<ServerProfile>,
    pub preferences: Preferences,
    pub playback_position: f64,
    pub remembered_servers: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 2,
            server: None,
            mpv_path: "mpv".into(),
            volume: 75.0,
            queue: Queue::default(),
            servers: Vec::new(),
            preferences: Preferences::default(),
            playback_position: 0.0,
            remembered_servers: Vec::new(),
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
    if settings.version != 1 && settings.version != 2 {
        return Err(Error::Version(settings.version));
    }
    settings.version = 2;
    if settings.servers.is_empty()
        && let Some(profile) = &settings.server
    {
        settings.servers.push(profile.clone());
    }
    settings.servers.truncate(100);
    settings.preferences.normalize();
    settings.servers = settings
        .servers
        .into_iter()
        .map(credentials::validate_profile)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::InvalidProfile)?;
    settings.server = settings
        .server
        .map(credentials::validate_profile)
        .transpose()
        .map_err(|_| Error::InvalidProfile)?;
    settings.playback_position = if settings.playback_position.is_finite() {
        settings.playback_position.clamp(0.0, 31_536_000.0)
    } else {
        0.0
    };
    settings.volume = if settings.volume.is_finite() {
        settings
            .volume
            .clamp(0.0, settings.preferences.playback.volume_max)
    } else {
        75.0
    };
    settings.queue.validate();
    Ok(settings)
}

pub fn save(dir: &Path, settings: &Settings) -> Result<(), Error> {
    for profile in settings.servers.iter().chain(settings.server.iter()) {
        credentials::validate_profile(profile.clone()).map_err(|_| Error::InvalidProfile)?;
    }
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
                username: "alice".into(),
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
    #[test]
    fn profile_url_credentials_are_rejected_before_any_settings_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        settings.servers.push(ServerProfile {
            url: "https://user:secret@music.example".into(),
            username: "user".into(),
            ..Default::default()
        });
        assert!(matches!(
            save(dir.path(), &settings),
            Err(Error::InvalidProfile)
        ));
        assert!(!dir.path().join("settings.json").exists());
    }

    #[test]
    fn schema_one_migrates_profiles_and_new_preferences_without_starting_audio() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), br#"{
            "version":1,
            "server":{"name":"Old server","url":"https://music.example/base/","username":"alice","kind":"Navidrome"},
            "volume":150,
            "queue":{"tracks":[{"id":"saved","title":"Saved track","artist":"Artist","album":"Album","album_id":"album","artist_id":"artist","duration":180,"number":1,"disc":1,"year":2026,"genre":"Jazz","cover_id":"cover","favorite":true}],"current":0,"repeat":"All","shuffle":false}
        }"#).unwrap();
        let settings = load(dir.path()).unwrap();
        assert_eq!(settings.version, 2);
        assert_eq!(settings.servers.len(), 1);
        assert_eq!(settings.servers[0].url, "https://music.example/base");
        assert_eq!(settings.queue.current_track().unwrap().id, "saved");
        assert_eq!(settings.volume, 100.0);
        assert!(settings.preferences.resume_position);
        assert!(settings.remembered_servers.is_empty());
        save(dir.path(), &settings).unwrap();
        assert_eq!(load(dir.path()).unwrap().version, 2);
    }
}
