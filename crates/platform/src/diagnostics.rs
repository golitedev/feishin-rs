//! Bounded, structured local diagnostics without free-form user/server data.
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticEvent {
    ConnectStarted,
    Connected,
    ConnectionFailed,
    PlaybackStarted,
    PlaybackStopped,
    PlaybackFailed,
    SettingsSaved,
    SettingsFailed,
    CredentialStored,
    CredentialUnavailable,
}

pub struct Diagnostics {
    tx: mpsc::SyncSender<DiagnosticEvent>,
}
impl Diagnostics {
    pub fn start(dir: &Path) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(256);
        let dir = dir.to_path_buf();
        std::thread::Builder::new()
            .name("feishin-diagnostics".into())
            .spawn(move || {
                if fs::create_dir_all(&dir).is_err() {
                    return;
                }
                let path = dir.join("diagnostics.jsonl");
                let mut file = match open_log(&path) {
                    Ok(file) => file,
                    Err(_) => return,
                };
                while let Ok(event) = rx.recv() {
                    if file.metadata().is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
                        drop(file);
                        if fs::rename(&path, dir.join("diagnostics.previous.jsonl")).is_err() {
                            return;
                        }
                        file = match open_log(&path) {
                            Ok(file) => file,
                            Err(_) => return,
                        };
                    }
                    let _ = write_event(&mut file, event);
                }
            })?;
        Ok(Self { tx })
    }
    pub fn event(&self, event: DiagnosticEvent) {
        let _ = self.tx.try_send(event);
    }
}

fn open_log(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn write_event(writer: &mut impl Write, event: DiagnosticEvent) -> std::io::Result<()> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    serde_json::to_writer(
        &mut *writer,
        &serde_json::json!({"timestamp":seconds,"event":event}),
    )?;
    writer.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_schema_cannot_capture_credentials_urls_or_metadata() {
        let mut bytes = Vec::new();
        write_event(&mut bytes, DiagnosticEvent::ConnectionFailed).unwrap();
        let object: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(object.as_object().unwrap().len(), 2);
        assert_eq!(object["event"], "connection_failed");
        assert!(!String::from_utf8(bytes).unwrap().contains("password"));
    }
}
