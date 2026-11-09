//! Optional protocol workflows. No frontend or upstream application code is used.
use super::*;
use feishin_core::features::{
    LyricLine, LyricWord, Lyrics, PlaylistDetails, PlaylistMetadata, RadioStation,
    ServerCapabilities, SmartPlaylist, StreamFormat, StreamOptions, TrackDetails,
};
use std::io::Write;

const MAX_DOWNLOAD: u64 = 8 * 1024 * 1024 * 1024;
const MAX_RULES: usize = 128 * 1024;

/// Never derive Debug: tokens stay out of diagnostics.
pub(super) struct NativeAuth {
    token: String,
    client_id: String,
}

impl Client {
    /// Ratings encountered in existing responses; no network request is made.
    pub fn ratings(&self) -> std::collections::HashMap<String, u8> {
        self.ratings
            .lock()
            .map(|ratings| ratings.clone())
            .unwrap_or_default()
    }

    pub(super) fn cache_ratings(&self, value: &Value) {
        let Ok(mut ratings) = self.ratings.lock() else {
            return;
        };
        let mut pending = vec![(value, 0usize)];
        let mut visited = 0usize;
        while let Some((value, depth)) = pending.pop() {
            visited += 1;
            if visited > 200_000 {
                break;
            }
            if let Some(id) = value.get("id").and_then(Value::as_str)
                && let Some(rating) = value
                    .get("userRating")
                    .and_then(Value::as_u64)
                    .filter(|rating| *rating <= 5)
                && (ratings.len() < MAX_ITEMS || ratings.contains_key(id))
            {
                ratings.insert(id.to_owned(), rating as u8);
            }
            if depth >= 64 {
                continue;
            }
            match value {
                Value::Object(fields) => {
                    pending.extend(
                        fields
                            .values()
                            .take(200_000usize.saturating_sub(pending.len()))
                            .map(|value| (value, depth + 1)),
                    );
                }
                Value::Array(items) => {
                    pending.extend(
                        items
                            .iter()
                            .take(200_000usize.saturating_sub(pending.len()))
                            .map(|value| (value, depth + 1)),
                    );
                }
                _ => (),
            }
        }
    }

    pub fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities {
            numeric_ratings: matches!(self.auth, Auth::Subsonic { .. }),
            radio: matches!(self.auth, Auth::Subsonic { .. }),
            playlist_metadata: true,
            playlist_reorder: true,
            downloads: true,
            transcoding: true,
            timed_lyrics: true,
            smart_playlists: self.kind == ServerKind::Navidrome
                && self.native.lock().is_ok_and(|auth| auth.is_some()),
        }
    }

    pub fn set_rating(&self, id: &str, rating: u8) -> Result<()> {
        if rating > 5 {
            return Err(Error::InvalidInput("Rating must be between 0 and 5"));
        }
        if !matches!(self.auth, Auth::Subsonic { .. }) {
            return Err(Error::Unsupported);
        }
        self.subsonic(
            "setRating",
            &[("id", id.to_owned()), ("rating", rating.to_string())],
        )?;
        self.ratings
            .lock()
            .map_err(|_| Error::InvalidResponse)?
            .insert(id.to_owned(), rating);
        Ok(())
    }

    pub fn track_details(&self, id: &str) -> Result<TrackDetails> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let response = self.subsonic("getSong", &[("id", id.to_owned())])?;
                let song = response.get("song").ok_or(Error::InvalidResponse)?;
                Ok(TrackDetails {
                    track: subsonic_track(song)?,
                    rating: Some(number(song, "userRating").min(5) as u8),
                    bitrate_kbps: number(song, "bitRate"),
                    sample_rate: number(song, "samplingRate"),
                    bit_depth: number(song, "bitDepth"),
                    channels: number(song, "channelCount"),
                    codec: text(song, "suffix"),
                    mime_type: text(song, "contentType"),
                    size_bytes: u64_number(song, "size"),
                    path: text(song, "path"),
                    play_count: u64_number(song, "playCount"),
                    comment: text(song, "comment"),
                    musicbrainz_id: text(song, "musicBrainzId"),
                })
            }
            Auth::Jellyfin { .. } => {
                let song = self.jellyfin_item(id)?;
                let media = array(&song, "MediaSources").first().unwrap_or(&Value::Null);
                let audio = array(&song, "MediaStreams")
                    .iter()
                    .chain(array(media, "MediaStreams"))
                    .find(|stream| text(stream, "Type") == "Audio")
                    .unwrap_or(&Value::Null);
                Ok(TrackDetails {
                    track: jellyfin_track(&song)?,
                    rating: None,
                    bitrate_kbps: number(audio, "BitRate") / 1000,
                    sample_rate: number(audio, "SampleRate"),
                    bit_depth: number(audio, "BitDepth"),
                    channels: number(audio, "Channels"),
                    codec: text(audio, "Codec"),
                    mime_type: text(media, "Container"),
                    size_bytes: u64_number(media, "Size"),
                    path: text(&song, "Path"),
                    play_count: song
                        .pointer("/UserData/PlayCount")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    comment: text(&song, "Overview"),
                    musicbrainz_id: song
                        .pointer("/ProviderIds/MusicBrainzTrack")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                })
            }
        }
    }

    pub fn playlist_details(&self, id: &str) -> Result<PlaylistDetails> {
        match &self.auth {
            Auth::Subsonic { username, .. } => {
                let value = self.subsonic("getPlaylist", &[("id", id.to_owned())])?;
                let list = value.get("playlist").ok_or(Error::InvalidResponse)?;
                let owner = text(list, "owner");
                let mut smart = list
                    .get("isSmartPlaylist")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || list.get("rules").is_some_and(|r| r.is_object());
                // Standard Subsonic playlist metadata has no smart flag. A native
                // Navidrome login lets us identify rule-managed playlists honestly.
                if self.capabilities().smart_playlists {
                    let native =
                        self.native_request(self.http.get(self.url(&["api", "playlist", id])?))?;
                    smart |= native.get("rules").is_some_and(Value::is_object);
                }
                Ok(PlaylistDetails {
                    playlist: subsonic_playlist(list)?,
                    metadata: PlaylistMetadata {
                        name: text(list, "name"),
                        description: text(list, "comment"),
                        public: flag(list, "public"),
                    },
                    editable: owner == *username,
                    owner,
                    smart,
                })
            }
            Auth::Jellyfin { user_id, .. } => {
                let item = self.jellyfin_item(id)?;
                let configuration = self.jellyfin_get(self.url(&["Playlists", id])?, &[])?;
                let editable = array(&configuration, "Shares")
                    .iter()
                    .any(|share| text(share, "UserId") == *user_id && flag(share, "CanEdit"));
                Ok(PlaylistDetails {
                    playlist: jellyfin_playlist(&item)?,
                    metadata: PlaylistMetadata {
                        name: text(&item, "Name"),
                        description: text(&item, "Overview"),
                        public: flag(&configuration, "OpenAccess"),
                    },
                    editable,
                    owner: String::new(),
                    smart: false,
                })
            }
        }
    }

    pub fn update_playlist(&self, id: &str, metadata: &PlaylistMetadata) -> Result<()> {
        validate_playlist_name(&metadata.name)?;
        if metadata.description.len() > 16384 {
            return Err(Error::InvalidInput("Playlist description is too long"));
        }
        match self.auth {
            Auth::Subsonic { .. } => {
                self.subsonic(
                    "updatePlaylist",
                    &[
                        ("playlistId", id.to_owned()),
                        ("name", metadata.name.clone()),
                        ("comment", metadata.description.clone()),
                        ("public", metadata.public.to_string()),
                    ],
                )?;
            }
            Auth::Jellyfin { .. } => {
                // Keep all existing metadata; posting a fresh partial BaseItemDto can
                // reset fields unrelated to the name/description being edited.
                let mut item = self.jellyfin_item(id)?;
                item["Name"] = json!(metadata.name);
                item["Overview"] = json!(metadata.description);
                self.jellyfin_post(&["Items", id], &item)?;
                self.jellyfin_post(
                    &["Playlists", id],
                    &json!({"Name":metadata.name,"IsPublic":metadata.public}),
                )?;
            }
        }
        Ok(())
    }

    /// Move one occurrence by its current position. Repeated song IDs remain distinct.
    pub fn move_playlist_track(&self, id: &str, from: usize, to: usize) -> Result<()> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let mut tracks = self.playlist_tracks(id)?;
                if from >= tracks.len() || to >= tracks.len() {
                    return Err(Error::InvalidInput("Playlist position is out of range"));
                }
                if from == to {
                    return Ok(());
                }
                let track = tracks.remove(from);
                tracks.insert(to, track);
                // The standard API replaces a playlist's entries with createPlaylist
                // + playlistId. This is one request and preserves duplicate songs.
                let mut params = vec![("playlistId", id.to_owned())];
                for track in tracks {
                    params.push(("songId", track.id));
                }
                self.subsonic("createPlaylist", &params)?;
            }
            Auth::Jellyfin { user_id, .. } => {
                let items = self.jellyfin_pages(
                    self.url(&["Playlists", id, "Items"])?,
                    &[("UserId", user_id.clone())],
                )?;
                if from >= items.len() || to >= items.len() {
                    return Err(Error::InvalidInput("Playlist position is out of range"));
                }
                if from == to {
                    return Ok(());
                }
                let entry = required_text(
                    items.get(from).ok_or(Error::InvalidResponse)?,
                    "PlaylistItemId",
                )?;
                checked_response(self.jellyfin_auth(self.http.post(self.url(&[
                    "Playlists",
                    id,
                    "Items",
                    &entry,
                    "Move",
                    &to.to_string(),
                ])?)))?;
            }
        }
        Ok(())
    }

    pub fn radio_stations(&self) -> Result<Vec<RadioStation>> {
        self.require_radio()?;
        let value = self.subsonic("getInternetRadioStations", &[])?;
        let container = value
            .get("internetRadioStations")
            .ok_or(Error::InvalidResponse)?;
        let items = array(container, "internetRadioStation");
        if items.len() > MAX_ITEMS {
            return Err(Error::LibraryLimit);
        }
        items
            .iter()
            .map(|station| {
                Ok(RadioStation {
                    id: required_text(station, "id")?,
                    name: text(station, "name"),
                    stream_url: text(station, "streamUrl"),
                    homepage_url: text(station, "homePageUrl"),
                })
            })
            .collect()
    }

    pub fn create_radio(&self, station: &RadioStation) -> Result<()> {
        self.write_radio(station, false)
    }
    pub fn update_radio(&self, station: &RadioStation) -> Result<()> {
        self.write_radio(station, true)
    }
    pub fn delete_radio(&self, id: &str) -> Result<()> {
        self.require_radio()?;
        self.subsonic("deleteInternetRadioStation", &[("id", id.to_owned())])?;
        Ok(())
    }
    fn require_radio(&self) -> Result<()> {
        if matches!(self.auth, Auth::Subsonic { .. }) {
            Ok(())
        } else {
            Err(Error::Unsupported)
        }
    }
    fn write_radio(&self, station: &RadioStation, update: bool) -> Result<()> {
        self.require_radio()?;
        if station.name.trim().is_empty() || station.name.len() > 512 {
            return Err(Error::InvalidInput(
                "Enter a radio station name (up to 512 bytes)",
            ));
        }
        validate_media_url(&station.stream_url)?;
        if !station.homepage_url.is_empty() {
            validate_media_url(&station.homepage_url)?;
        }
        let mut params = vec![
            ("name", station.name.clone()),
            ("streamUrl", station.stream_url.clone()),
            ("homepageUrl", station.homepage_url.clone()),
        ];
        if update {
            if station.id.is_empty() {
                return Err(Error::InvalidInput("Radio station ID is missing"));
            }
            params.push(("id", station.id.clone()));
        }
        self.subsonic(
            if update {
                "updateInternetRadioStation"
            } else {
                "createInternetRadioStation"
            },
            &params,
        )?;
        Ok(())
    }

    /// Original bytes streamed into a caller-owned staging file, at most 8 GiB.
    /// The caller must discard a partial output when this returns an error.
    pub fn download(&self, track: &Track, writer: &mut impl Write) -> Result<u64> {
        let request = match self.auth {
            Auth::Subsonic { .. } => {
                let mut url = self.subsonic_url("download")?;
                url.query_pairs_mut().append_pair("id", &track.id);
                self.http.get(url)
            }
            Auth::Jellyfin { .. } => {
                self.jellyfin_auth(self.http.get(self.url(&["Items", &track.id, "Download"])?))
            }
        }
        .timeout(Duration::from_secs(600));
        let mut response = checked_response(request)?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DOWNLOAD)
        {
            return Err(Error::ResponseTooLarge);
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|t| t.starts_with("application/json") || t.starts_with("text/"))
        {
            return Err(Error::InvalidResponse);
        }
        let mut count = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let size = response.read(&mut buffer).map_err(|_| Error::Network)?;
            if size == 0 {
                break;
            }
            count = count
                .checked_add(size as u64)
                .ok_or(Error::ResponseTooLarge)?;
            if count > MAX_DOWNLOAD {
                return Err(Error::ResponseTooLarge);
            }
            writer
                .write_all(&buffer[..size])
                .map_err(|_| Error::Write)?;
        }
        Ok(count)
    }

    pub fn stream_with_options(
        &self,
        track: &Track,
        options: &StreamOptions,
    ) -> Result<StreamSource> {
        if options
            .max_bitrate_kbps
            .is_some_and(|rate| !(8..=2000).contains(&rate))
        {
            return Err(Error::InvalidInput(
                "Streaming bitrate must be between 8 and 2000 kbps",
            ));
        }
        if options
            .sample_rate
            .is_some_and(|rate| !(8000..=384000).contains(&rate))
        {
            return Err(Error::InvalidInput(
                "Streaming sample rate must be between 8000 and 384000 Hz",
            ));
        }
        if options.format == StreamFormat::Original
            && (options.max_bitrate_kbps.is_some() || options.sample_rate.is_some())
        {
            return Err(Error::InvalidInput(
                "Choose a transcoding format before limiting bitrate or sample rate",
            ));
        }
        let transcode = options.format != StreamFormat::Original;
        let source = match &self.auth {
            Auth::Subsonic { .. } => {
                if options.sample_rate.is_some() {
                    return Err(Error::Unsupported);
                }
                let mut url = self.subsonic_url("stream")?;
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("id", &track.id)
                    .append_pair("format", format_name(options.format));
                if let Some(rate) = options.max_bitrate_kbps {
                    query.append_pair("maxBitRate", &rate.to_string());
                }
                drop(query);
                StreamSource {
                    url: url.into(),
                    headers: vec![],
                    transcoded: transcode,
                }
            }
            Auth::Jellyfin { token, .. } => {
                let path = if transcode {
                    format!("stream.{}", container_name(options.format))
                } else {
                    "stream".to_owned()
                };
                let mut url = self.url(&["Audio", &track.id, &path])?;
                let mut query = url.query_pairs_mut();
                query.append_pair("Static", if transcode { "false" } else { "true" });
                if transcode {
                    query
                        .append_pair("AudioCodec", format_name(options.format))
                        .append_pair("EnableAutoStreamCopy", "false")
                        .append_pair("AllowAudioStreamCopy", "false");
                }
                if let Some(rate) = options.max_bitrate_kbps {
                    query.append_pair("AudioBitRate", &rate.saturating_mul(1000).to_string());
                }
                if let Some(rate) = options.sample_rate {
                    query.append_pair("AudioSampleRate", &rate.to_string());
                }
                drop(query);
                StreamSource {
                    url: url.into(),
                    headers: vec![("Authorization".into(), jellyfin_authorization(Some(token)))],
                    transcoded: transcode,
                }
            }
        };
        let mut methods = self
            .source_methods
            .lock()
            .map_err(|_| Error::InvalidResponse)?;
        if methods.len() >= 4096 {
            methods.clear();
        }
        methods.insert(track.id.clone(), transcode);
        Ok(source)
    }

    pub(super) fn play_method(&self, track: &Track) -> &'static str {
        if self
            .source_methods
            .lock()
            .is_ok_and(|methods| methods.get(&track.id).copied().unwrap_or(false))
        {
            "Transcode"
        } else {
            "DirectPlay"
        }
    }

    pub fn lyrics_timed(&self, track: &Track) -> Result<Lyrics> {
        match self.auth {
            Auth::Subsonic { .. } => {
                match self.subsonic(
                    "getLyricsBySongId",
                    &[("id", track.id.clone()), ("enhanced", "true".into())],
                ) {
                    Ok(value) => {
                        let layers = value
                            .pointer("/lyricsList/structuredLyrics")
                            .and_then(Value::as_array)
                            .map(Vec::as_slice)
                            .unwrap_or_default();
                        if let Some(layer) = layers
                            .iter()
                            .find(|v| text(v, "kind").is_empty() || text(v, "kind") == "main")
                            .or_else(|| layers.first())
                        {
                            return Ok(subsonic_lyrics(layer));
                        }
                    }
                    Err(Error::Api(0 | 10 | 70)) | Err(Error::Http(404)) => (),
                    Err(error) => return Err(error),
                }
                let value = self.subsonic(
                    "getLyrics",
                    &[
                        ("artist", track.artist.clone()),
                        ("title", track.title.clone()),
                    ],
                )?;
                let text = value
                    .get("lyrics")
                    .map(|v| text(v, "value"))
                    .unwrap_or_default();
                Ok(Lyrics {
                    lines: text
                        .lines()
                        .map(|text| LyricLine {
                            text: text.to_owned(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                })
            }
            Auth::Jellyfin { .. } => {
                let value = self.jellyfin_get(self.url(&["Audio", &track.id, "Lyrics"])?, &[])?;
                let mut lines = array(&value, "Lyrics")
                    .iter()
                    .map(|line| LyricLine {
                        start_ms: line
                            .get("Start")
                            .and_then(Value::as_u64)
                            .map(|v| v / 10_000),
                        text: text(line, "Text"),
                        words: array(line, "Cues")
                            .iter()
                            .map(|cue| LyricWord {
                                start_ms: cue
                                    .get("Start")
                                    .and_then(Value::as_u64)
                                    .map(|v| v / 10_000),
                                end_ms: cue.get("End").and_then(Value::as_u64).map(|v| v / 10_000),
                                text: if cue.get("Text").is_some() {
                                    text(cue, "Text")
                                } else {
                                    jellyfin_lyric_fragment(
                                        &text(line, "Text"),
                                        number(cue, "Position"),
                                        number(cue, "EndPosition"),
                                    )
                                },
                            })
                            .collect(),
                        ..Default::default()
                    })
                    .collect::<Vec<_>>();
                let synced = lines.iter().any(|line| line.start_ms.is_some());
                if synced {
                    lines.sort_by_key(|line| line.start_ms);
                }
                Ok(Lyrics {
                    synced,
                    lines,
                    language: value
                        .pointer("/Metadata/Language")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    kind: "main".into(),
                    ..Default::default()
                })
            }
        }
    }

    fn jellyfin_item(&self, id: &str) -> Result<Value> {
        let Auth::Jellyfin { user_id, .. } = &self.auth else {
            return Err(Error::Unsupported);
        };
        self.jellyfin_get(self.url(&["Items", id])?, &[("UserId", user_id.clone())])
    }
}

fn validate_playlist_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 512 {
        Err(Error::InvalidInput(
            "Enter a playlist name (up to 512 bytes)",
        ))
    } else {
        Ok(())
    }
}
fn validate_media_url(value: &str) -> Result<()> {
    let url =
        Url::parse(value).map_err(|_| Error::InvalidInput("Enter an HTTP or HTTPS radio URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::InvalidInput(
            "Enter an HTTP or HTTPS radio URL without embedded credentials",
        ));
    }
    Ok(())
}
fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn u64_number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn format_name(format: StreamFormat) -> &'static str {
    match format {
        StreamFormat::Original => "raw",
        StreamFormat::Mp3 => "mp3",
        StreamFormat::Aac => "aac",
        StreamFormat::Opus => "opus",
        StreamFormat::Flac => "flac",
    }
}
fn container_name(format: StreamFormat) -> &'static str {
    if format == StreamFormat::Opus {
        "ogg"
    } else {
        format_name(format)
    }
}

fn subsonic_lyrics(value: &Value) -> Lyrics {
    let cues = array(value, "cueLine");
    let enhanced = !cues.is_empty();
    let mut lines = if enhanced { cues } else { array(value, "line") }
        .iter()
        .map(|line| LyricLine {
            start_ms: line.get("start").and_then(Value::as_u64),
            end_ms: line.get("end").and_then(Value::as_u64),
            text: text(line, "value"),
            agent_id: text(line, "agentId"),
            words: array(line, "cue")
                .iter()
                .map(|cue| LyricWord {
                    start_ms: cue.get("start").and_then(Value::as_u64),
                    end_ms: cue.get("end").and_then(Value::as_u64),
                    text: text(cue, "value"),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let synced = flag(value, "synced");
    if synced {
        lines.sort_by_key(|line| line.start_ms);
    }
    Lyrics {
        lines,
        synced,
        language: text(value, "lang"),
        offset_ms: value.get("offset").and_then(Value::as_i64).unwrap_or(0),
        kind: if text(value, "kind").is_empty() {
            "main".into()
        } else {
            text(value, "kind")
        },
    }
}

impl Client {
    /// Opt-in native Navidrome login for smart rules. Password is not retained.
    pub fn enable_navidrome(&self, password: &str) -> Result<()> {
        if self.kind != ServerKind::Navidrome {
            return Err(Error::Unsupported);
        }
        let Auth::Subsonic { username, .. } = &self.auth else {
            return Err(Error::Unsupported);
        };
        let client_id = Uuid::new_v4().to_string();
        let value = json_response(
            self.http
                .post(self.url(&["auth", "login"])?)
                .header("X-ND-Client-Unique-Id", &client_id)
                .json(&json!({"username":username,"password":password})),
        )?;
        let token = required_text(&value, "token")?;
        if reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).is_err() {
            return Err(Error::InvalidResponse);
        }
        *self.native.lock().map_err(|_| Error::InvalidResponse)? =
            Some(NativeAuth { token, client_id });
        Ok(())
    }

    pub fn smart_playlists(&self) -> Result<Vec<SmartPlaylist>> {
        let mut result = Vec::new();
        let mut offset = 0usize;
        let mut seen = HashSet::new();
        loop {
            let response =
                self.native_request(self.http.get(self.url(&["api", "playlist"])?).query(&[
                    ("_start", offset.to_string()),
                    ("_end", offset.saturating_add(PAGE).to_string()),
                    ("_sort", "name".into()),
                    ("_order", "ASC".into()),
                ]))?;
            let items = response.as_array().ok_or(Error::InvalidResponse)?;
            for item in items {
                let id = required_text(item, "id")?;
                if !seen.insert(id) {
                    return Err(Error::LibraryLimit);
                }
                if item.get("rules").is_some_and(Value::is_object) {
                    result.push(smart_playlist(item)?);
                }
            }
            offset = offset.checked_add(items.len()).ok_or(Error::LibraryLimit)?;
            if offset > MAX_ITEMS {
                return Err(Error::LibraryLimit);
            }
            if items.len() < PAGE {
                break;
            }
        }
        Ok(result)
    }

    pub fn smart_playlist(&self, id: &str) -> Result<SmartPlaylist> {
        smart_playlist(&self.native_request(self.http.get(self.url(&["api", "playlist", id])?))?)
    }

    /// Empty ID creates a playlist; existing IDs update in place, retaining metadata.
    pub fn save_smart_playlist(&self, playlist: &SmartPlaylist) -> Result<String> {
        validate_playlist_name(&playlist.name)?;
        if playlist.description.len() > 16384 {
            return Err(Error::InvalidInput("Playlist description is too long"));
        }
        let rules = validated_rules(&playlist.rules_json)?;
        let (request, mut value) = if playlist.id.is_empty() {
            (self.http.post(self.url(&["api", "playlist"])?), json!({}))
        } else {
            let value = self.native_request(self.http.get(self.url(&[
                "api",
                "playlist",
                &playlist.id,
            ])?))?;
            // Do not silently convert an ordinary playlist into a smart one.
            if !value.get("rules").is_some_and(Value::is_object) {
                return Err(Error::InvalidInput(
                    "The selected playlist is not a smart playlist",
                ));
            }
            (
                self.http.put(self.url(&["api", "playlist", &playlist.id])?),
                value,
            )
        };
        value["name"] = json!(playlist.name);
        value["comment"] = json!(playlist.description);
        value["public"] = json!(playlist.public);
        value["rules"] = rules;
        let response = self.native_request(request.json(&value))?;
        if playlist.id.is_empty() {
            required_text(&response, "id")
        } else {
            Ok(playlist.id.clone())
        }
    }

    pub fn delete_smart_playlist(&self, id: &str) -> Result<()> {
        // Check the type before deleting, so the smart editor cannot delete an ordinary list.
        self.smart_playlist(id)?;
        checked_response(self.native_auth(self.http.delete(self.url(&["api", "playlist", id])?))?)?;
        Ok(())
    }

    fn native_auth(&self, request: RequestBuilder) -> Result<RequestBuilder> {
        if self.kind != ServerKind::Navidrome {
            return Err(Error::Unsupported);
        }
        let auth = self.native.lock().map_err(|_| Error::InvalidResponse)?;
        let auth = auth.as_ref().ok_or(Error::InvalidInput(
            "Sign in to Navidrome's native API to edit smart playlists",
        ))?;
        Ok(request
            .header("X-ND-Authorization", format!("Bearer {}", auth.token))
            .header("X-ND-Client-Unique-Id", &auth.client_id))
    }
    fn native_request(&self, request: RequestBuilder) -> Result<Value> {
        json_response(self.native_auth(request)?)
    }
}

fn smart_playlist(value: &Value) -> Result<SmartPlaylist> {
    let rules = value
        .get("rules")
        .filter(|v| v.is_object())
        .ok_or(Error::InvalidInput(
            "The selected playlist is not a smart playlist",
        ))?;
    Ok(SmartPlaylist {
        id: required_text(value, "id")?,
        name: text(value, "name"),
        description: text(value, "comment"),
        public: flag(value, "public"),
        rules_json: serde_json::to_string_pretty(rules).map_err(|_| Error::InvalidResponse)?,
    })
}

fn validated_rules(text: &str) -> Result<Value> {
    if text.len() > MAX_RULES {
        return Err(Error::InvalidInput("Smart playlist rules exceed 128 KiB"));
    }
    let value: Value = serde_json::from_str(text)
        .map_err(|_| Error::InvalidInput("Smart playlist rules must be valid JSON"))?;
    let object = value.as_object().ok_or(Error::InvalidInput(
        "Smart playlist rules must be a JSON object",
    ))?;
    let criteria = object
        .get("all")
        .or_else(|| object.get("any"))
        .ok_or(Error::InvalidInput(
            "Smart playlist rules need a nonempty all or any group",
        ))?;
    validate_group(criteria, 0, &mut 0)?;
    if object
        .get("limit")
        .is_some_and(|limit| !limit.is_u64() && !limit.is_string())
    {
        return Err(Error::InvalidInput(
            "Smart playlist limit must be a positive number or percentage",
        ));
    }
    Ok(value)
}

fn validate_group(value: &Value, depth: usize, count: &mut usize) -> Result<()> {
    if depth > 16 {
        return Err(Error::InvalidInput(
            "Smart playlist rules are nested too deeply",
        ));
    }
    let group = value
        .as_array()
        .filter(|group| !group.is_empty())
        .ok_or(Error::InvalidInput(
            "Each smart playlist group needs at least one rule",
        ))?;
    for rule in group {
        *count = count
            .checked_add(1)
            .ok_or(Error::InvalidInput("Too many smart playlist rules"))?;
        if *count > 512 {
            return Err(Error::InvalidInput(
                "Smart playlist rules are limited to 512 conditions",
            ));
        }
        let object =
            rule.as_object()
                .filter(|object| object.len() == 1)
                .ok_or(Error::InvalidInput(
                    "Each smart playlist condition must contain exactly one operator",
                ))?;
        if let Some(group) = object.get("all").or_else(|| object.get("any")) {
            validate_group(group, depth + 1, count)?;
        } else if !object
            .values()
            .all(|value| value.as_object().is_some_and(|fields| !fields.is_empty()))
        {
            return Err(Error::InvalidInput(
                "Each smart playlist operator needs a field and value",
            ));
        }
    }
    Ok(())
}

// Jellyfin positions are .NET string indices (UTF-16 code units). Never slice UTF-8
// at these offsets, and reject offsets landing inside a surrogate pair.
fn jellyfin_lyric_fragment(text: &str, start: u32, end: u32) -> String {
    fn boundary(text: &str, target: u32) -> Option<usize> {
        let mut units = 0u32;
        for (byte, character) in text.char_indices() {
            if units == target {
                return Some(byte);
            }
            units = units.checked_add(character.len_utf16() as u32)?;
            if units > target {
                return None;
            }
        }
        (units == target).then_some(text.len())
    }
    if end < start {
        return String::new();
    }
    let Some(start) = boundary(text, start) else {
        return String::new();
    };
    let Some(end) = boundary(text, end) else {
        return String::new();
    };
    text.get(start..end).unwrap_or_default().to_owned()
}
