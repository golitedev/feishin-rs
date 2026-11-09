//! Shared music models and queue policy, independent of transports and UI.
pub mod playback;
pub mod preferences;

use serde::{Deserialize, Serialize};

pub mod features;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerKind {
    #[default]
    Navidrome,
    OpenSubsonic,
    Jellyfin,
}

impl ServerKind {
    pub const ALL: [Self; 3] = [Self::Navidrome, Self::OpenSubsonic, Self::Jellyfin];
    pub fn label(self) -> &'static str {
        match self {
            Self::Navidrome => "Navidrome",
            Self::OpenSubsonic => "OpenSubsonic",
            Self::Jellyfin => "Jellyfin",
        }
    }
}

/// Deliberately excludes credentials; safe to persist as preferences.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerProfile {
    pub name: String,
    pub url: String,
    pub username: String,
    pub kind: ServerKind,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: String,
    pub artist_id: String,
    pub duration: f64,
    pub number: u32,
    pub disc: u32,
    pub year: u32,
    pub genre: String,
    pub cover_id: String,
    pub favorite: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub cover_id: String,
    pub year: u32,
    pub genre: String,
    pub song_count: u32,
    pub duration: f64,
    pub favorite: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub cover_id: String,
    pub album_count: u32,
    #[serde(default)]
    pub favorite: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub song_count: u32,
    pub duration: f64,
    pub cover_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct Library {
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    pub artists: Vec<Artist>,
    pub playlists: Vec<Playlist>,
    pub genres: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Queue {
    pub tracks: Vec<Track>,
    pub current: Option<usize>,
    pub repeat: Repeat,
    pub shuffle: bool,
    #[serde(default)]
    history: Vec<usize>,
    #[serde(default)]
    remaining: Vec<usize>,
    #[serde(default)]
    forced_next: Vec<usize>,
}

impl Queue {
    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    pub fn replace(&mut self, tracks: Vec<Track>, start: usize) -> bool {
        self.tracks = tracks;
        self.current = (start < self.tracks.len()).then_some(start);
        self.history.clear();
        self.forced_next.clear();
        self.reset_remaining();
        self.current.is_some()
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.tracks.len() {
            return false;
        }
        if let Some(previous) = self.current {
            self.history.push(previous);
        }
        self.current = Some(index);
        self.remaining.retain(|i| *i != index);
        self.forced_next.retain(|i| *i != index);
        true
    }

    pub fn append(&mut self, tracks: Vec<Track>) {
        let start = self.tracks.len();
        self.tracks.extend(tracks);
        self.remaining.extend(start..self.tracks.len());
    }

    pub fn insert_next(&mut self, tracks: Vec<Track>) {
        let count = tracks.len();
        if count == 0 {
            return;
        }
        let index = self
            .current
            .map_or(0, |i| i.saturating_add(1).min(self.tracks.len()));
        self.tracks.splice(index..index, tracks);
        let mut forced: Vec<_> = (index..index.saturating_add(count)).collect();
        forced.extend(self.forced_next.iter().map(|i| {
            if *i >= index {
                i.saturating_add(count)
            } else {
                *i
            }
        }));
        self.forced_next = forced;
        for previous in &mut self.history {
            if *previous >= index {
                *previous = previous.saturating_add(count);
            }
        }
        for remaining in &mut self.remaining {
            if *remaining >= index {
                *remaining = remaining.saturating_add(count);
            }
        }
        self.remaining.extend(index..index.saturating_add(count));
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.tracks.len() {
            return false;
        }
        let was_current = self.current == Some(index);
        self.tracks.remove(index);
        self.current = match self.current {
            _ if self.tracks.is_empty() => None,
            Some(i) if i > index => Some(i - 1),
            Some(i) => Some(i.min(self.tracks.len().saturating_sub(1))),
            None => None,
        };
        self.history.clear();
        self.forced_next = self
            .forced_next
            .iter()
            .filter(|i| **i != index)
            .map(|i| if *i > index { i - 1 } else { *i })
            .collect();
        self.forced_next.retain(|i| Some(*i) != self.current);
        self.reset_remaining();
        was_current
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
        self.current = None;
        self.history.clear();
        self.remaining.clear();
        self.forced_next.clear();
    }

    /// Moves one occurrence; current/history/forced-next identity follows that
    /// occurrence even when a queue contains the same song more than once.
    pub fn move_track(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tracks.len() || to >= self.tracks.len() || from == to {
            return false;
        }
        let remap = |index: usize| {
            if index == from {
                to
            } else if from < to && index > from && index <= to {
                index - 1
            } else if to < from && index >= to && index < from {
                index + 1
            } else {
                index
            }
        };
        let track = self.tracks.remove(from);
        self.tracks.insert(to, track);
        self.current = self.current.map(remap);
        for index in self
            .history
            .iter_mut()
            .chain(&mut self.remaining)
            .chain(&mut self.forced_next)
        {
            *index = remap(*index);
        }
        true
    }

    pub fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
        self.reset_remaining();
    }

    fn reset_remaining(&mut self) {
        self.remaining = (0..self.tracks.len())
            .filter(|i| Some(*i) != self.current)
            .collect();
    }

    /// `automatic` distinguishes a natural EOF from the Next button for repeat-one.
    pub fn advance(&mut self, automatic: bool) -> Option<&Track> {
        let current = self.current?;
        if automatic && self.repeat == Repeat::One {
            return self.current_track();
        }
        let next = if !self.forced_next.is_empty() {
            let index = self.forced_next.remove(0);
            self.remaining.retain(|i| *i != index);
            Some(index)
        } else if self.shuffle {
            if self.remaining.is_empty() && self.repeat == Repeat::All {
                self.reset_remaining();
            }
            if self.remaining.is_empty() {
                if self.repeat == Repeat::All {
                    Some(current)
                } else {
                    None
                }
            } else {
                use rand::Rng;
                let slot = rand::rng().random_range(0..self.remaining.len());
                Some(self.remaining.swap_remove(slot))
            }
        } else if current.saturating_add(1) < self.tracks.len() {
            Some(current + 1)
        } else if self.repeat == Repeat::All && !self.tracks.is_empty() {
            Some(0)
        } else {
            None
        };
        if let Some(next) = next {
            self.history.push(current);
            self.current = Some(next);
        }
        next.and_then(|_| self.current_track())
    }

    pub fn previous(&mut self) -> Option<&Track> {
        let current = self.current?;
        if self.shuffle {
            if let Some(index) = self.history.pop() {
                self.remaining.push(current);
                self.current = Some(index);
            }
        } else {
            self.current = Some(current.saturating_sub(1));
        }
        self.current_track()
    }

    pub fn validate(&mut self) {
        if self.current.is_some_and(|i| i >= self.tracks.len()) {
            self.current = None;
        }
        self.history.retain(|i| *i < self.tracks.len());
        self.forced_next
            .retain(|i| *i < self.tracks.len() && Some(*i) != self.current);
        self.reset_remaining();
    }
}

pub fn duration_text(seconds: f64) -> String {
    let seconds = if seconds.is_finite() && seconds > 0.0 {
        seconds.min(3599999.0) as u64
    } else {
        0
    };
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tracks() -> Vec<Track> {
        (0..3)
            .map(|i| Track {
                id: i.to_string(),
                ..Default::default()
            })
            .collect()
    }
    #[test]
    fn explicit_next_items_override_shuffle_in_order() {
        let mut q = Queue::default();
        q.replace(tracks(), 0);
        q.set_shuffle(true);
        q.insert_next(vec![
            Track {
                id: "next-a".into(),
                ..Default::default()
            },
            Track {
                id: "next-b".into(),
                ..Default::default()
            },
        ]);
        assert_eq!(q.advance(false).map(|t| t.id.as_str()), Some("next-a"));
        assert_eq!(q.advance(false).map(|t| t.id.as_str()), Some("next-b"));
        assert!(q.advance(false).is_some_and(|t| t.id == "1" || t.id == "2"));
    }
    #[test]
    fn reordering_duplicate_occurrences_preserves_current_and_forced_next() {
        let mut q = Queue::default();
        q.replace(tracks(), 1);
        q.insert_next(vec![Track {
            id: "0".into(),
            ..Default::default()
        }]);
        assert!(q.move_track(1, 3));
        assert_eq!(q.current, Some(3));
        assert_eq!(q.current_track().unwrap().id, "1");
        assert_eq!(q.advance(false).unwrap().id, "0");
        assert!(!q.move_track(usize::MAX, 0));
    }
    #[test]
    fn natural_eof_and_manual_next_have_distinct_repeat_policy() {
        let mut q = Queue::default();
        q.replace(tracks(), 0);
        q.repeat = Repeat::One;
        assert_eq!(q.advance(true).map(|t| t.id.as_str()), Some("0"));
        assert_eq!(q.advance(false).map(|t| t.id.as_str()), Some("1"));
        q.advance(false);
        assert!(q.advance(false).is_none());
        q.repeat = Repeat::All;
        assert_eq!(q.advance(true).map(|t| t.id.as_str()), Some("0"));
    }
    #[test]
    fn shuffled_cycle_visits_each_track_once_and_previous_uses_history() {
        let mut q = Queue::default();
        q.replace(tracks(), 0);
        q.set_shuffle(true);
        let first = q.advance(false).map(|t| t.id.clone()).unwrap();
        let second = q.advance(false).map(|t| t.id.clone()).unwrap();
        assert_ne!(first, second);
        assert_ne!(first, "0");
        assert_ne!(second, "0");
        assert!(q.advance(true).is_none());
        assert_eq!(q.previous().map(|t| t.id.as_str()), Some(first.as_str()));
    }
    #[test]
    fn removing_current_and_invalid_restored_indices_are_safe() {
        let mut q = Queue::default();
        q.replace(tracks(), 2);
        assert!(q.remove(2));
        assert_eq!(q.current_track().map(|t| t.id.as_str()), Some("1"));
        q.current = Some(usize::MAX);
        q.validate();
        assert!(q.current_track().is_none());
        q.clear();
        assert!(q.advance(true).is_none());
        assert!(q.previous().is_none());
    }
}
