//! Persistable playback preferences and MPV-observed audio information.
use serde::{Deserialize, Serialize};

pub const EQ_FREQUENCIES: [u32; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayGain {
    #[default]
    Off,
    Track,
    Album,
}

impl ReplayGain {
    pub const ALL: [Self; 3] = [Self::Off, Self::Track, Self::Album];
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Track => "Track",
            Self::Album => "Album",
        }
    }
    pub fn mpv_value(self) -> &'static str {
        match self {
            Self::Off => "no",
            Self::Track => "track",
            Self::Album => "album",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioChannels {
    #[default]
    Auto,
    Mono,
    Stereo,
    Surround51,
    Surround71,
}

impl AudioChannels {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Mono,
        Self::Stereo,
        Self::Surround51,
        Self::Surround71,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Mono => "Mono",
            Self::Stereo => "Stereo",
            Self::Surround51 => "5.1 surround",
            Self::Surround71 => "7.1 surround",
        }
    }
    pub fn mpv_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mono => "mono",
            Self::Stereo => "stereo",
            Self::Surround51 => "5.1",
            Self::Surround71 => "7.1",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Equalizer {
    pub enabled: bool,
    pub bands_db: [f64; 10],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackSettings {
    pub audio_device: String,
    pub exclusive: bool,
    /// None preserves the source/output device's preferred sample rate.
    pub sample_rate: Option<u32>,
    pub channels: AudioChannels,
    pub replay_gain: ReplayGain,
    pub replay_gain_preamp: f64,
    pub replay_gain_fallback: f64,
    /// When false, MPV lowers ReplayGain to avoid clipping.
    pub replay_gain_clip: bool,
    pub preamp_db: f64,
    pub speed: f64,
    pub preserve_pitch: bool,
    pub volume_max: f64,
    pub equalizer: Equalizer,
    pub normalization: bool,
    /// Keeps the output device open and permits prepared next-track playback.
    pub gapless: bool,
    /// Reopen the output when its format changes, as recommended by MPV.
    pub gapless_weak: bool,
    /// Fade pause/resume transitions without changing the user's volume.
    pub fade_on_pause: bool,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            audio_device: "auto".into(),
            exclusive: false,
            sample_rate: None,
            channels: AudioChannels::Auto,
            replay_gain: ReplayGain::Off,
            replay_gain_preamp: 0.0,
            replay_gain_fallback: 0.0,
            replay_gain_clip: false,
            preamp_db: 0.0,
            speed: 1.0,
            preserve_pitch: true,
            volume_max: 100.0,
            equalizer: Equalizer::default(),
            normalization: false,
            gapless: true,
            gapless_weak: true,
            fade_on_pause: true,
        }
    }
}

impl PlaybackSettings {
    /// Rejects malformed saved values before they reach MPV. This returns a copy
    /// so callers can persist exactly the bounded settings they apply.
    pub fn normalized(&self) -> Self {
        let mut value = self.clone();
        if value.audio_device.is_empty()
            || value.audio_device.len() > 4096
            || value.audio_device.chars().any(char::is_control)
        {
            value.audio_device = "auto".into();
        }
        value.sample_rate = value
            .sample_rate
            .filter(|rate| (8000..=384000).contains(rate));
        value.replay_gain_preamp = finite(value.replay_gain_preamp, 0.0, -24.0, 24.0);
        value.replay_gain_fallback = finite(value.replay_gain_fallback, 0.0, -24.0, 24.0);
        value.preamp_db = finite(value.preamp_db, 0.0, -24.0, 12.0);
        value.speed = finite(value.speed, 1.0, 0.25, 4.0);
        value.volume_max = finite(value.volume_max, 100.0, 100.0, 200.0);
        for gain in &mut value.equalizer.bands_db {
            *gain = finite(*gain, 0.0, -12.0, 12.0);
        }
        value
    }
}

fn finite(value: f64, fallback: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioStats {
    pub codec: String,
    pub sample_format: String,
    pub sample_rate: u32,
    pub channels: String,
    pub channel_count: u32,
    /// Compressed/source bits per second, as reported by MPV.
    pub bitrate: u64,
    pub output_sample_rate: u32,
    pub output_channels: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_preferences_keep_recommended_pitch_and_gapless_defaults() {
        let settings: PlaybackSettings = serde_json::from_str(r#"{"speed":1.25}"#).unwrap();
        assert!(settings.preserve_pitch);
        assert!(settings.gapless);
        assert!(settings.gapless_weak);
        assert!(settings.fade_on_pause);
    }

    #[test]
    fn corrupt_or_extreme_preferences_are_bounded() {
        let mut settings = PlaybackSettings {
            audio_device: "bad\nname".into(),
            sample_rate: Some(u32::MAX),
            speed: f64::NAN,
            preamp_db: 500.0,
            volume_max: 1.0,
            ..PlaybackSettings::default()
        };
        settings.equalizer.bands_db[0] = f64::INFINITY;
        settings.equalizer.bands_db[1] = -100.0;
        let settings = settings.normalized();
        assert_eq!(settings.audio_device, "auto");
        assert_eq!(settings.sample_rate, None);
        assert_eq!(settings.speed, 1.0);
        assert_eq!(settings.preamp_db, 12.0);
        assert_eq!(settings.volume_max, 100.0);
        assert_eq!(settings.equalizer.bands_db[0], 0.0);
        assert_eq!(settings.equalizer.bands_db[1], -12.0);
    }
}
