//! Optional server features shared by native views and adapters.
use crate::{Playlist, Track};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackDetails {
    pub track: Track,
    /// 0 clears a rating; absent means numeric ratings are unavailable.
    pub rating: Option<u8>,
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub channels: u32,
    pub codec: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub path: String,
    pub play_count: u64,
    pub comment: String,
    pub musicbrainz_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistMetadata {
    pub name: String,
    pub description: String,
    pub public: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistDetails {
    pub playlist: Playlist,
    pub metadata: PlaylistMetadata,
    pub owner: String,
    pub smart: bool,
    pub editable: bool,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadioStation {
    pub id: String,
    pub name: String,
    pub stream_url: String,
    pub homepage_url: String,
}
impl std::fmt::Debug for RadioStation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioStation")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("urls", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamFormat {
    #[default]
    Original,
    Mp3,
    Aac,
    Opus,
    Flac,
}
impl StreamFormat {
    pub const ALL: [Self; 5] = [Self::Original, Self::Mp3, Self::Aac, Self::Opus, Self::Flac];
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::Opus => "Opus",
            Self::Flac => "FLAC",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamOptions {
    pub format: StreamFormat,
    pub max_bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricWord {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub text: String,
    pub words: Vec<LyricWord>,
    pub agent_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
    pub language: String,
    /// Positive means lyrics display sooner, matching OpenSubsonic.
    pub offset_ms: i64,
    pub kind: String,
}
impl Lyrics {
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn active_line(&self, position_ms: u64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let time = i128::from(position_ms) + i128::from(self.offset_ms);
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                line.start_ms
                    .filter(|start| i128::from(*start) <= time)
                    .map(|_| i)
            })
            .next_back()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartPlaylist {
    pub id: String,
    pub name: String,
    pub description: String,
    pub public: bool,
    /// Navidrome's criteria object as editable JSON; unknown operators survive a round trip.
    pub rules_json: String,
}

/// Protocol support, not a promise of user permissions or server extensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServerCapabilities {
    pub numeric_ratings: bool,
    pub radio: bool,
    pub playlist_metadata: bool,
    pub playlist_reorder: bool,
    pub downloads: bool,
    pub transcoding: bool,
    pub timed_lyrics: bool,
    /// True only after a successful native Navidrome authentication.
    pub smart_playlists: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicFolder {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FolderEntry {
    pub id: String,
    pub name: String,
    pub is_directory: bool,
    pub cover_id: String,
    pub track: Option<Track>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Directory {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub entries: Vec<FolderEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synchronized_lyrics_honor_signed_offset_without_overflow() {
        let mut lyrics = Lyrics {
            synced: true,
            offset_ms: 100,
            lines: vec![
                LyricLine {
                    start_ms: Some(1000),
                    text: "First".into(),
                    ..Default::default()
                },
                LyricLine {
                    start_ms: Some(2000),
                    text: "Second".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(lyrics.active_line(899), None);
        assert_eq!(lyrics.active_line(900), Some(0));
        assert_eq!(lyrics.active_line(1900), Some(1));
        lyrics.offset_ms = -100;
        assert_eq!(lyrics.active_line(1000), None);
        assert_eq!(lyrics.active_line(1100), Some(0));
        assert_eq!(lyrics.active_line(u64::MAX), Some(1));
        lyrics.synced = false;
        assert_eq!(lyrics.active_line(u64::MAX), None);
        assert_eq!(lyrics.plain_text(), "First\nSecond");
    }
}
