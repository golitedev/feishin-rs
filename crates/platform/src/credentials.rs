//! Opt-in OS credential storage. Never falls back to a plaintext file.
use feishin_core::ServerProfile;
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
#[error(
    "The operating-system credential store is unavailable or locked. Unlock your keyring, or reconnect without remembering the password."
)]
pub struct CredentialError;

#[derive(Debug, thiserror::Error)]
#[error(
    "Enter an HTTP or HTTPS server URL without embedded credentials, query parameters, or fragments, and a non-empty username"
)]
pub struct ProfileError;

pub fn validate_profile(mut profile: ServerProfile) -> Result<ServerProfile, ProfileError> {
    let url = url::Url::parse(profile.url.trim()).map_err(|_| ProfileError)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || profile.url.len() > 8192
        || profile.username.trim().is_empty()
        || profile.username.len() > 1024
        || profile.name.len() > 1024
    {
        return Err(ProfileError);
    }
    profile.url = url.as_str().trim_end_matches('/').to_owned();
    Ok(profile)
}

pub trait CredentialStore {
    fn load(&self, profile: &ServerProfile) -> Result<Option<String>, CredentialError>;
    fn save(&self, profile: &ServerProfile, password: &str) -> Result<(), CredentialError>;
    fn delete(&self, profile: &ServerProfile) -> Result<(), CredentialError>;
}

#[derive(Clone, Copy, Default)]
pub struct OsCredentialStore;

/// Names and display labels do not affect credential identity. Never include
/// a password/token in a keyring label, error message, log, or serialized settings.
pub fn account_key(profile: &ServerProfile) -> String {
    let mut digest = Sha256::new();
    for value in [
        profile.kind.label(),
        profile.url.trim().trim_end_matches('/'),
        profile.username.as_str(),
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("server-{:x}", digest.finalize())
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn entry(profile: &ServerProfile) -> Result<keyring::Entry, CredentialError> {
    keyring::Entry::new("feishin-rs", &account_key(profile)).map_err(|_| CredentialError)
}

impl CredentialStore for OsCredentialStore {
    fn load(&self, profile: &ServerProfile) -> Result<Option<String>, CredentialError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            match entry(profile)?.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(CredentialError),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = profile;
            Err(CredentialError)
        }
    }
    fn save(&self, profile: &ServerProfile, password: &str) -> Result<(), CredentialError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            entry(profile)?
                .set_password(password)
                .map_err(|_| CredentialError)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = (profile, password);
            Err(CredentialError)
        }
    }
    fn delete(&self, profile: &ServerProfile) -> Result<(), CredentialError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            match entry(profile)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(CredentialError),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = profile;
            Err(CredentialError)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_survive_renames_and_do_not_expose_server_details() {
        let mut profile = ServerProfile {
            name: "Original".into(),
            url: "https://music.example/".into(),
            username: "alice".into(),
            ..Default::default()
        };
        let original = account_key(&profile);
        profile.name = "Renamed".into();
        profile.url.pop();
        assert_eq!(account_key(&profile), original);
        profile.username = "bob".into();
        assert_ne!(account_key(&profile), original);
        assert!(!original.contains("alice"));
        assert!(!original.contains("music.example"));
    }
    #[test]
    fn saved_profiles_reject_credentials_in_urls() {
        for url in [
            "https://alice:secret@music.example",
            "https://music.example?token=secret",
            "file:///tmp/music",
            "https://music.example/#secret",
        ] {
            assert!(
                validate_profile(ServerProfile {
                    url: url.into(),
                    username: "alice".into(),
                    ..Default::default()
                })
                .is_err()
            );
        }
        assert!(
            validate_profile(ServerProfile {
                url: "https://music.example/base".into(),
                username: "alice".into(),
                ..Default::default()
            })
            .is_ok()
        );
    }
}
