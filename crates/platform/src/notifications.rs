//! Optional desktop notifications; callers run this on a background worker.
use feishin_core::Track;

#[derive(Debug, thiserror::Error)]
#[error("Desktop notifications are unavailable in this graphical session")]
pub struct NotificationError;

#[cfg(target_os = "linux")]
pub fn now_playing(track: &Track) -> Result<(), NotificationError> {
    use std::collections::HashMap;
    let connection = zbus::blocking::Connection::session().map_err(|_| NotificationError)?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .map_err(|_| NotificationError)?;
    let hints: HashMap<&str, zbus::zvariant::OwnedValue> = HashMap::new();
    let actions: Vec<&str> = Vec::new();
    let body = escape_markup(&format!("{}\n{}", track.artist, track.album));
    let _: u32 = proxy
        .call(
            "Notify",
            &(
                "Feishin",
                0_u32,
                "audio-x-generic",
                escape_markup(&track.title),
                body,
                actions,
                hints,
                5000_i32,
            ),
        )
        .map_err(|_| NotificationError)?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn now_playing(_track: &Track) -> Result<(), NotificationError> {
    Err(NotificationError)
}

fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_cannot_insert_notification_markup() {
        assert_eq!(
            escape_markup("<a>&song</a>"),
            "&lt;a&gt;&amp;song&lt;/a&gt;"
        );
    }
}
