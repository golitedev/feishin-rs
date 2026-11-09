//! Serializable presentation and behavior preferences. No credentials live here.
use crate::playback::PlaybackSettings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: String,
    pub follow_system_theme: bool,
    pub font_family: String,
    pub home_feature_style: String,
    pub show_ratings: bool,
    pub expand_sidebar_art: bool,
    pub playlist_compact: bool,
    pub accent_rgb: [u8; 3],
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub grid_size: f32,
    pub font_size: f32,
    pub show_feature: bool,
    pub show_genres: bool,
    pub album_background: bool,
    pub artist_background: bool,
    pub language: String,
    pub custom_colors: BTreeMap<String, [u8; 3]>,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "defaultDark".into(),
            follow_system_theme: false,
            font_family: "Inter".into(),
            home_feature_style: "single".into(),
            show_ratings: true,
            expand_sidebar_art: false,
            playlist_compact: false,
            accent_rgb: [53, 116, 252],
            sidebar_width: 400.0,
            sidebar_collapsed: false,
            grid_size: 180.0,
            font_size: 14.0,
            show_feature: true,
            show_genres: true,
            album_background: true,
            artist_background: true,
            language: "en".into(),
            custom_colors: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoDjSettings {
    pub enabled: bool,
    pub item_count: usize,
    pub allow_duplicates: bool,
    pub similar_only: bool,
}
impl Default for AutoDjSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            item_count: 5,
            allow_duplicates: false,
            similar_only: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedFilter {
    pub name: String,
    pub query: String,
    pub genre: String,
    pub favorites_only: bool,
    pub exclude_favorites: bool,
    pub artist: String,
    pub min_rating: u8,
    pub sort: String,
    pub descending: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub mpv_arguments: String,
    pub appearance: Appearance,
    pub playback: PlaybackSettings,
    pub shortcuts: BTreeMap<String, String>,
    pub saved_filters: Vec<SavedFilter>,
    pub auto_dj: AutoDjSettings,
    pub resume_queue: bool,
    pub resume_position: bool,
    pub notifications: bool,
    pub scrobble_enabled: bool,
    pub private_mode: bool,
    pub visualizer_enabled: bool,
    pub stream: crate::features::StreamOptions,
    pub lyrics: LyricsPreferences,
    pub download_directory: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            mpv_arguments: String::new(),
            appearance: Appearance::default(),
            playback: PlaybackSettings::default(),
            shortcuts: [
                ("play_pause", "Space"),
                ("next", "Ctrl+ArrowRight"),
                ("previous", "Ctrl+ArrowLeft"),
                ("stop", "Ctrl+Shift+Space"),
                ("seek_forward", "Alt+ArrowRight"),
                ("seek_backward", "Alt+ArrowLeft"),
                ("volume_up", "Ctrl+ArrowUp"),
                ("volume_down", "Ctrl+ArrowDown"),
                ("search", "Ctrl+F"),
                ("settings", "Ctrl+Comma"),
                ("queue", "Ctrl+Q"),
                ("fullscreen", "F11"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
            saved_filters: Vec::new(),
            auto_dj: AutoDjSettings::default(),
            resume_queue: true,
            resume_position: true,
            notifications: false,
            scrobble_enabled: true,
            private_mode: false,
            visualizer_enabled: false,
            stream: crate::features::StreamOptions::default(),
            lyrics: LyricsPreferences::default(),
            download_directory: String::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricsPreferences {
    pub enabled: bool,
    pub follow: bool,
    pub delay_ms: i64,
    pub lead_ms: u64,
    pub font_size: f32,
    pub alignment: String,
    pub show_provider: bool,
}
impl Default for LyricsPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            follow: true,
            delay_ms: 0,
            lead_ms: 800,
            font_size: 24.0,
            alignment: "center".into(),
            show_provider: true,
        }
    }
}

impl Preferences {
    pub fn normalize(&mut self) {
        fn finite(value: f32, default: f32, min: f32, max: f32) -> f32 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                default
            }
        }
        self.appearance.sidebar_width = finite(self.appearance.sidebar_width, 400.0, 66.0, 600.0);
        self.appearance.grid_size = finite(self.appearance.grid_size, 180.0, 96.0, 360.0);
        self.appearance.font_size = finite(self.appearance.font_size, 14.0, 10.0, 24.0);
        self.auto_dj.item_count = self.auto_dj.item_count.clamp(1, 100);
        self.playback = self.playback.normalized();
        self.lyrics.font_size = finite(self.lyrics.font_size, 24.0, 10.0, 72.0);
        self.lyrics.delay_ms = self.lyrics.delay_ms.clamp(-60000, 60000);
        self.lyrics.lead_ms = self.lyrics.lead_ms.min(10000);
        self.saved_filters.truncate(100);
        for filter in &mut self.saved_filters {
            filter.min_rating = filter.min_rating.min(5);
            if filter.favorites_only {
                filter.exclude_favorites = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_artist_and_favorite_filters_round_trip_and_older_filters_still_load() {
        let older: SavedFilter =
            serde_json::from_str(r#"{"name":"Old","favorites_only":true}"#).expect("old filter");
        assert!(older.artist.is_empty());
        assert!(!older.exclude_favorites);
        let filter = SavedFilter {
            name: "Jazz artist".into(),
            artist: "Performer".into(),
            genre: "Jazz".into(),
            exclude_favorites: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&filter).expect("serialize filter");
        assert_eq!(
            serde_json::from_str::<SavedFilter>(&json).expect("restore filter"),
            filter
        );
    }
    #[test]
    fn invalid_geometry_and_oversized_auto_dj_are_normalized() {
        let mut preferences = Preferences::default();
        preferences.appearance.sidebar_width = f32::NAN;
        preferences.appearance.grid_size = f32::INFINITY;
        preferences.auto_dj.item_count = usize::MAX;
        preferences.normalize();
        assert_eq!(preferences.appearance.sidebar_width, 400.0);
        assert_eq!(preferences.appearance.grid_size, 180.0);
        assert_eq!(preferences.auto_dj.item_count, 100);
    }
}
