//! Blocking server adapters. Call these methods on a worker, never the UI thread.
//! Credentials remain in memory and all public diagnostics redact transport details.
use feishin_core::{Album, Artist, Library, Playlist, ServerKind, ServerProfile, Track};
use md5::{Digest, Md5};
use reqwest::{
    Url,
    blocking::{Client as Http, RequestBuilder, Response},
    redirect::Policy,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fmt,
    io::Read,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

const PAGE: usize = 200;
const MAX_ITEMS: usize = 100_000;
const MAX_JSON: u64 = 16 * 1024 * 1024;
const MAX_COVER: u64 = 8 * 1024 * 1024;
const DEVICE: &str = "feishin-rs-desktop";

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Enter an HTTP or HTTPS server URL without credentials, query parameters, or fragments"
    )]
    InvalidUrl,
    #[error("Server request timed out; check your connection and server address")]
    Timeout,
    #[error("Could not contact the server; check your address, connection, and TLS certificate")]
    Network,
    #[error("Server returned HTTP {0}; check your login and server permissions")]
    Http(u16),
    #[error(
        "Server rejected the request (Subsonic error {0}); check credentials or server capabilities"
    )]
    Api(u64),
    #[error("Server returned an invalid or incomplete response")]
    InvalidResponse,
    #[error("Server response exceeds the allowed size")]
    ResponseTooLarge,
    #[error("Library exceeds 100,000 items or the server did not advance pagination")]
    LibraryLimit,
    #[error("This server does not support that operation")]
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Track,
    Album,
    Artist,
}

/// A playback source may contain credentials. Debug intentionally reveals nothing.
#[derive(Clone)]
pub struct StreamSource {
    pub url: String,
    pub headers: Vec<(String, String)>,
}
impl fmt::Debug for StreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StreamSource([redacted])")
    }
}

#[derive(Clone)]
enum Auth {
    Subsonic {
        username: String,
        salt: String,
        token: String,
    },
    Jellyfin {
        user_id: String,
        token: String,
    },
}

#[derive(Clone)]
pub struct Client {
    http: Http,
    base: Url,
    auth: Auth,
    playback: Arc<Mutex<Option<(String, String)>>>,
}
impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Client([redacted])")
    }
}

impl Client {
    pub fn login(profile: ServerProfile, password: &str) -> Result<Self> {
        let base = parse_base(&profile.url)?;
        let http = Http::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .user_agent("feishin-rs/0.1.0")
            .build()
            .map_err(network_error)?;
        match profile.kind {
            ServerKind::Navidrome | ServerKind::OpenSubsonic => {
                let salt = Uuid::new_v4().simple().to_string();
                let token = subsonic_token(password, &salt);
                let client = Self {
                    http,
                    base,
                    auth: Auth::Subsonic {
                        username: profile.username,
                        salt,
                        token,
                    },
                    playback: Arc::new(Mutex::new(None)),
                };
                client.subsonic("ping", &[])?;
                Ok(client)
            }
            ServerKind::Jellyfin => {
                let endpoint = endpoint(&base, &["Users", "AuthenticateByName"])?;
                let request = http
                    .post(endpoint)
                    .header("Authorization", jellyfin_authorization(None))
                    .json(&json!({"Username":profile.username,"Pw":password}));
                let response = json_response(request)?;
                let token = required_text(&response, "AccessToken")?;
                let user_id = response
                    .get("User")
                    .and_then(|v| v.get("Id"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or(Error::InvalidResponse)?
                    .to_owned();
                // The authorization header and playback header must remain single-line.
                if token.contains(['\r', '\n', '"', '\\']) {
                    return Err(Error::InvalidResponse);
                }
                Ok(Self {
                    http,
                    base,
                    auth: Auth::Jellyfin { token, user_id },
                    playback: Arc::new(Mutex::new(None)),
                })
            }
        }
    }

    pub fn library(&self) -> Result<Library> {
        match self.auth {
            Auth::Subsonic { .. } => self.subsonic_library(),
            Auth::Jellyfin { .. } => self.jellyfin_library(),
        }
    }

    pub fn album_tracks(&self, id: &str) -> Result<Vec<Track>> {
        let mut tracks = match &self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic("getAlbum", &[("id", id.to_owned())])?;
                let album = value.get("album").ok_or(Error::InvalidResponse)?;
                let mut result = Vec::new();
                for song in array(album, "song") {
                    let mut track = subsonic_track(song)?;
                    if track.album_id.is_empty() {
                        track.album_id = id.to_owned();
                    }
                    if track.cover_id.is_empty() {
                        track.cover_id = text(album, "coverArt");
                    }
                    result.push(track);
                }
                result
            }
            Auth::Jellyfin { .. } => self
                .jellyfin_items(
                    "Audio",
                    &[
                        ("ParentId", id.to_owned()),
                        (
                            "SortBy",
                            "ParentIndexNumber,IndexNumber,SortName".to_owned(),
                        ),
                    ],
                )?
                .iter()
                .map(jellyfin_track)
                .collect::<Result<Vec<_>>>()?,
        };
        tracks.sort_by_key(|t| (t.disc, t.number));
        Ok(tracks)
    }

    pub fn artist_albums(&self, id: &str) -> Result<Vec<Album>> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic("getArtist", &[("id", id.to_owned())])?;
                array(value.get("artist").ok_or(Error::InvalidResponse)?, "album")
                    .iter()
                    .map(subsonic_album)
                    .collect()
            }
            Auth::Jellyfin { .. } => self
                .jellyfin_items("MusicAlbum", &[("AlbumArtistIds", id.to_owned())])?
                .iter()
                .map(jellyfin_album)
                .collect(),
        }
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        match self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic("getPlaylists", &[])?;
                array(
                    value.get("playlists").ok_or(Error::InvalidResponse)?,
                    "playlist",
                )
                .iter()
                .map(subsonic_playlist)
                .collect()
            }
            Auth::Jellyfin { .. } => self
                .jellyfin_items("Playlist", &[])?
                .iter()
                .map(jellyfin_playlist)
                .collect(),
        }
    }

    pub fn playlist_tracks(&self, id: &str) -> Result<Vec<Track>> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic("getPlaylist", &[("id", id.to_owned())])?;
                array(
                    value.get("playlist").ok_or(Error::InvalidResponse)?,
                    "entry",
                )
                .iter()
                .map(subsonic_track)
                .collect()
            }
            Auth::Jellyfin { user_id, .. } => {
                let url = self.url(&["Playlists", id, "Items"])?;
                self.jellyfin_pages(
                    url,
                    &[
                        ("UserId", user_id.clone()),
                        ("Fields", jellyfin_fields().to_owned()),
                    ],
                )?
                .iter()
                .map(jellyfin_track)
                .collect()
            }
        }
    }

    pub fn search(&self, query: &str) -> Result<Library> {
        match self.auth {
            Auth::Subsonic { .. } => self.subsonic_search(query),
            Auth::Jellyfin { .. } => {
                let params = [("SearchTerm", query.to_owned())];
                let tracks = self
                    .jellyfin_items("Audio", &params)?
                    .iter()
                    .map(jellyfin_track)
                    .collect::<Result<Vec<_>>>()?;
                let albums = self
                    .jellyfin_items("MusicAlbum", &params)?
                    .iter()
                    .map(jellyfin_album)
                    .collect::<Result<Vec<_>>>()?;
                let artists = self
                    .jellyfin_artists(&params)?
                    .iter()
                    .map(jellyfin_artist)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Library {
                    tracks,
                    albums,
                    artists,
                    ..Default::default()
                })
            }
        }
    }

    pub fn set_favorite(&self, id: &str, kind: ItemKind, favorite: bool) -> Result<()> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let key = match kind {
                    ItemKind::Track => "id",
                    ItemKind::Album => "albumId",
                    ItemKind::Artist => "artistId",
                };
                self.subsonic(
                    if favorite { "star" } else { "unstar" },
                    &[(key, id.to_owned())],
                )?;
            }
            Auth::Jellyfin { user_id, .. } => {
                let url = self.url(&["UserFavoriteItems", id])?;
                let request = if favorite {
                    self.http.post(url)
                } else {
                    self.http.delete(url)
                };
                let result =
                    checked_response(self.jellyfin_auth(request.query(&[("UserId", user_id)])));
                if matches!(result, Err(Error::Http(404))) {
                    let url = self.url(&["Users", user_id, "FavoriteItems", id])?;
                    let request = if favorite {
                        self.http.post(url)
                    } else {
                        self.http.delete(url)
                    };
                    checked_response(self.jellyfin_auth(request))?;
                } else {
                    result?;
                }
            }
        }
        Ok(())
    }

    /// Create a private playlist; IDs refer to tracks on the current server.
    pub fn create_playlist(&self, name: &str, track_ids: &[String]) -> Result<String> {
        if name.trim().is_empty() || name.len() > 512 || track_ids.len() > MAX_ITEMS {
            return Err(Error::InvalidResponse);
        }
        match &self.auth {
            Auth::Subsonic { .. } => {
                let mut params = vec![("name", name.to_owned())];
                for id in track_ids {
                    params.push(("songId", id.clone()));
                }
                let value = self.subsonic("createPlaylist", &params)?;
                required_text(value.get("playlist").ok_or(Error::InvalidResponse)?, "id")
            }
            Auth::Jellyfin { user_id, .. } => {
                let value=json_response(self.jellyfin_auth(self.http.post(self.url(&["Playlists"])?).json(&json!({"Name":name,"Ids":track_ids,"UserId":user_id,"MediaType":"Audio","IsPublic":false}))))?;
                required_text(&value, "Id")
            }
        }
    }

    pub fn delete_playlist(&self, id: &str) -> Result<()> {
        match self.auth {
            Auth::Subsonic { .. } => {
                self.subsonic("deletePlaylist", &[("id", id.to_owned())])?;
            }
            Auth::Jellyfin { .. } => {
                checked_response(self.jellyfin_auth(self.http.delete(self.url(&["Items", id])?)))?;
            }
        }
        Ok(())
    }

    pub fn add_to_playlist(&self, id: &str, track_ids: &[String]) -> Result<()> {
        if track_ids.is_empty() {
            return Ok(());
        }
        if track_ids.len() > MAX_ITEMS {
            return Err(Error::LibraryLimit);
        }
        match &self.auth {
            Auth::Subsonic { .. } => {
                let mut params = vec![("playlistId", id.to_owned())];
                for id in track_ids {
                    params.push(("songIdToAdd", id.clone()));
                }
                self.subsonic("updatePlaylist", &params)?;
            }
            Auth::Jellyfin { user_id, .. } => {
                let request = self
                    .http
                    .post(self.url(&["Playlists", id, "Items"])?)
                    .query(&[("ids", track_ids.join(",")), ("userId", user_id.clone())]);
                checked_response(self.jellyfin_auth(request))?;
            }
        }
        Ok(())
    }

    /// Remove one occurrence by playlist position, not by song ID (duplicates are valid).
    pub fn remove_playlist_track(&self, id: &str, index: usize) -> Result<()> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                self.subsonic(
                    "updatePlaylist",
                    &[
                        ("playlistId", id.to_owned()),
                        ("songIndexToRemove", index.to_string()),
                    ],
                )?;
            }
            Auth::Jellyfin { user_id, .. } => {
                let url = self.url(&["Playlists", id, "Items"])?;
                let values = self.jellyfin_pages(url.clone(), &[("UserId", user_id.clone())])?;
                let item = values.get(index).ok_or(Error::InvalidResponse)?;
                let entry = required_text(item, "PlaylistItemId")?;
                checked_response(
                    self.jellyfin_auth(self.http.delete(url).query(&[("entryIds", entry)])),
                )?;
            }
        }
        Ok(())
    }

    pub fn stream(&self, track: &Track) -> Result<StreamSource> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let mut url = self.subsonic_url("stream")?;
                url.query_pairs_mut()
                    .append_pair("id", &track.id)
                    .append_pair("format", "raw");
                Ok(StreamSource {
                    url: url.into(),
                    headers: vec![],
                })
            }
            Auth::Jellyfin { token, .. } => {
                let mut url = self.url(&["Audio", &track.id, "stream"])?;
                url.query_pairs_mut().append_pair("Static", "true");
                Ok(StreamSource {
                    url: url.into(),
                    headers: vec![(
                        "Authorization".to_owned(),
                        jellyfin_authorization(Some(token)),
                    )],
                })
            }
        }
    }

    pub fn cover(&self, id: &str, size: u32) -> Result<Vec<u8>> {
        let request = match self.auth {
            Auth::Subsonic { .. } => {
                let mut url = self.subsonic_url("getCoverArt")?;
                url.query_pairs_mut()
                    .append_pair("id", id)
                    .append_pair("size", &size.clamp(32, 1024).to_string());
                self.http.get(url)
            }
            Auth::Jellyfin { .. } => {
                let mut url = self.url(&["Items", id, "Images", "Primary"])?;
                url.query_pairs_mut()
                    .append_pair("MaxWidth", &size.clamp(32, 1024).to_string());
                self.jellyfin_auth(self.http.get(url))
            }
        };
        read_bounded(checked_response(request)?, MAX_COVER)
    }

    pub fn lyrics(&self, track: &Track) -> Result<String> {
        match self.auth {
            Auth::Subsonic { .. } => {
                match self.subsonic("getLyricsBySongId", &[("id", track.id.clone())]) {
                    Ok(value) => {
                        if let Some(lyrics) = value
                            .pointer("/lyricsList/structuredLyrics")
                            .and_then(Value::as_array)
                            .and_then(|a| a.first())
                        {
                            let lines = array(lyrics, "line")
                                .iter()
                                .map(|line| text(line, "value"))
                                .collect::<Vec<_>>();
                            if !lines.is_empty() {
                                return Ok(lines.join("\n"));
                            }
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
                Ok(value
                    .get("lyrics")
                    .map(|v| text(v, "value"))
                    .unwrap_or_default())
            }
            Auth::Jellyfin { .. } => {
                let value = self.jellyfin_get(self.url(&["Audio", &track.id, "Lyrics"])?, &[])?;
                Ok(array(&value, "Lyrics")
                    .iter()
                    .map(|line| text(line, "Text"))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
    }

    /// Subsonic now-playing (`false`) or completed scrobble (`true`).
    /// Jellyfin maps these to playback-start and natural-end session reports.
    pub fn scrobble(&self, track: &Track, submission: bool) -> Result<()> {
        match self.auth {
            Auth::Subsonic { .. } => {
                self.subsonic(
                    "scrobble",
                    &[
                        ("id", track.id.clone()),
                        ("submission", submission.to_string()),
                    ],
                )?;
                Ok(())
            }
            Auth::Jellyfin { .. } => {
                if submission {
                    self.playback_stopped(track, track.duration)
                } else {
                    let session = Uuid::new_v4().simple().to_string();
                    let body = json!({"ItemId":track.id,"CanSeek":true,"IsPaused":false,"PositionTicks":0,"PlayMethod":"DirectPlay","PlaySessionId":session});
                    self.jellyfin_post(&["Sessions", "Playing"], &body)?;
                    *self.playback.lock().map_err(|_| Error::InvalidResponse)? =
                        Some((track.id.clone(), session));
                    Ok(())
                }
            }
        }
    }

    /// Report Jellyfin position/pause changes. Subsonic has no equivalent session API.
    pub fn playback_progress(&self, track: &Track, position: f64, paused: bool) -> Result<()> {
        if !matches!(self.auth, Auth::Jellyfin { .. }) {
            return Ok(());
        }
        let Some(session) = self.play_session(&track.id)? else {
            return Ok(());
        };
        self.jellyfin_post(&["Sessions","Playing","Progress"],&json!({"ItemId":track.id,"PositionTicks":ticks(position),"IsPaused":paused,"CanSeek":true,"PlayMethod":"DirectPlay","PlaySessionId":session}))
    }

    /// Report a stop at the observed position, including skips and explicit stops.
    pub fn playback_stopped(&self, track: &Track, position: f64) -> Result<()> {
        if !matches!(self.auth, Auth::Jellyfin { .. }) {
            return Ok(());
        }
        let Some(session) = self.play_session(&track.id)? else {
            return Ok(());
        };
        self.jellyfin_post(&["Sessions","Playing","Stopped"],&json!({"ItemId":track.id,"PositionTicks":ticks(position),"PlaySessionId":session,"Failed":false}))?;
        let mut playing = self.playback.lock().map_err(|_| Error::InvalidResponse)?;
        if playing
            .as_ref()
            .is_some_and(|(id, current_session)| id == &track.id && current_session == &session)
        {
            *playing = None;
        }
        Ok(())
    }

    fn play_session(&self, track_id: &str) -> Result<Option<String>> {
        let playing = self.playback.lock().map_err(|_| Error::InvalidResponse)?;
        Ok(playing
            .as_ref()
            .filter(|(id, _)| id == track_id)
            .map(|(_, session)| session.clone()))
    }

    fn jellyfin_post(&self, segments: &[&str], body: &Value) -> Result<()> {
        checked_response(self.jellyfin_auth(self.http.post(self.url(segments)?).json(body)))?;
        Ok(())
    }

    fn subsonic_library(&self) -> Result<Library> {
        let mut albums = Vec::new();
        let mut seen = HashSet::new();
        loop {
            let value = self.subsonic(
                "getAlbumList2",
                &[
                    ("type", "alphabeticalByName".to_owned()),
                    ("size", PAGE.to_string()),
                    ("offset", albums.len().to_string()),
                ],
            )?;
            let page = array(
                value.get("albumList2").ok_or(Error::InvalidResponse)?,
                "album",
            );
            for item in page {
                let album = subsonic_album(item)?;
                if !seen.insert(album.id.clone()) {
                    return Err(Error::LibraryLimit);
                }
                albums.push(album);
            }
            if albums.len() > MAX_ITEMS {
                return Err(Error::LibraryLimit);
            }
            if page.len() < PAGE {
                break;
            }
        }
        let artists_value = self.subsonic("getArtists", &[])?;
        let artists_container = artists_value.get("artists").ok_or(Error::InvalidResponse)?;
        let artists = array(artists_container, "index")
            .iter()
            .flat_map(|index| array(index, "artist"))
            .map(subsonic_artist)
            .collect::<Result<Vec<_>>>()?;
        let playlists = self.playlists()?;
        // Empty-query search is an OpenSubsonic extension. Older servers may reject it.
        let tracks = match self.subsonic_songs("") {
            Ok(tracks) if !tracks.is_empty() || albums.is_empty() => tracks,
            Ok(_) | Err(Error::Api(0 | 10 | 70)) | Err(Error::Http(404)) => {
                let mut tracks = Vec::new();
                for album in &albums {
                    tracks.extend(self.album_tracks(&album.id)?);
                    if tracks.len() > MAX_ITEMS {
                        return Err(Error::LibraryLimit);
                    }
                }
                tracks
            }
            Err(error) => return Err(error),
        };
        let genres = genres(&albums, &tracks);
        Ok(Library {
            albums,
            tracks,
            artists,
            playlists,
            genres,
        })
    }

    fn subsonic_songs(&self, query: &str) -> Result<Vec<Track>> {
        let mut tracks = Vec::new();
        let mut seen = HashSet::new();
        loop {
            let value = self.subsonic(
                "search3",
                &[
                    ("query", query.to_owned()),
                    ("songCount", PAGE.to_string()),
                    ("songOffset", tracks.len().to_string()),
                    ("albumCount", "0".to_owned()),
                    ("artistCount", "0".to_owned()),
                ],
            )?;
            let page = array(
                value.get("searchResult3").ok_or(Error::InvalidResponse)?,
                "song",
            );
            for item in page {
                let track = subsonic_track(item)?;
                if !seen.insert(track.id.clone()) {
                    return Err(Error::LibraryLimit);
                }
                tracks.push(track);
            }
            if tracks.len() > MAX_ITEMS {
                return Err(Error::LibraryLimit);
            }
            if page.len() < PAGE {
                break;
            }
        }
        Ok(tracks)
    }

    fn subsonic_search(&self, query: &str) -> Result<Library> {
        let tracks = self.subsonic_songs(query)?;
        let mut albums = Vec::new();
        let mut artists = Vec::new();
        let mut album_done = false;
        let mut artist_done = false;
        let mut seen_albums = HashSet::new();
        let mut seen_artists = HashSet::new();
        while !album_done || !artist_done {
            let value = self.subsonic(
                "search3",
                &[
                    ("query", query.to_owned()),
                    ("songCount", "0".to_owned()),
                    (
                        "albumCount",
                        if album_done {
                            "0".to_owned()
                        } else {
                            PAGE.to_string()
                        },
                    ),
                    ("albumOffset", albums.len().to_string()),
                    (
                        "artistCount",
                        if artist_done {
                            "0".to_owned()
                        } else {
                            PAGE.to_string()
                        },
                    ),
                    ("artistOffset", artists.len().to_string()),
                ],
            )?;
            let result = value.get("searchResult3").ok_or(Error::InvalidResponse)?;
            let album_page = array(result, "album");
            let artist_page = array(result, "artist");
            for item in album_page {
                let album = subsonic_album(item)?;
                if !seen_albums.insert(album.id.clone()) {
                    return Err(Error::LibraryLimit);
                }
                albums.push(album);
            }
            for item in artist_page {
                let artist = subsonic_artist(item)?;
                if !seen_artists.insert(artist.id.clone()) {
                    return Err(Error::LibraryLimit);
                }
                artists.push(artist);
            }
            album_done |= album_page.len() < PAGE;
            artist_done |= artist_page.len() < PAGE;
            if albums.len() > MAX_ITEMS || artists.len() > MAX_ITEMS {
                return Err(Error::LibraryLimit);
            }
        }
        Ok(Library {
            albums,
            tracks,
            artists,
            ..Default::default()
        })
    }

    fn jellyfin_library(&self) -> Result<Library> {
        let albums = self
            .jellyfin_items("MusicAlbum", &[])?
            .iter()
            .map(jellyfin_album)
            .collect::<Result<Vec<_>>>()?;
        let tracks = self
            .jellyfin_items("Audio", &[])?
            .iter()
            .map(jellyfin_track)
            .collect::<Result<Vec<_>>>()?;
        let artists = self
            .jellyfin_artists(&[])?
            .iter()
            .map(jellyfin_artist)
            .collect::<Result<Vec<_>>>()?;
        let playlists = self
            .jellyfin_items("Playlist", &[])?
            .iter()
            .map(jellyfin_playlist)
            .collect::<Result<Vec<_>>>()?;
        let genres = genres(&albums, &tracks);
        Ok(Library {
            albums,
            tracks,
            artists,
            playlists,
            genres,
        })
    }

    fn jellyfin_items(&self, kind: &str, extra: &[(&str, String)]) -> Result<Vec<Value>> {
        let Auth::Jellyfin { user_id, .. } = &self.auth else {
            return Err(Error::Unsupported);
        };
        let mut params = vec![
            ("IncludeItemTypes", kind.to_owned()),
            ("Recursive", "true".to_owned()),
            ("Fields", jellyfin_fields().to_owned()),
            ("SortBy", "SortName".to_owned()),
            ("SortOrder", "Ascending".to_owned()),
            ("EnableUserData", "true".to_owned()),
        ];
        for (key, value) in extra {
            params.retain(|(existing, _)| existing != key);
            params.push((*key, value.clone()));
        }
        params.push(("UserId", user_id.clone()));
        self.jellyfin_pages(self.url(&["Items"])?, &params)
    }

    fn jellyfin_artists(&self, extra: &[(&str, String)]) -> Result<Vec<Value>> {
        let Auth::Jellyfin { user_id, .. } = &self.auth else {
            return Err(Error::Unsupported);
        };
        let mut params = vec![
            ("UserId", user_id.clone()),
            ("Recursive", "true".to_owned()),
            ("Fields", jellyfin_fields().to_owned()),
        ];
        params.extend_from_slice(extra);
        self.jellyfin_pages(self.url(&["Artists"])?, &params)
    }

    fn jellyfin_pages(&self, url: Url, params: &[(&str, String)]) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        loop {
            let mut params = params.to_vec();
            params.push(("StartIndex", items.len().to_string()));
            params.push(("Limit", PAGE.to_string()));
            let value = self.jellyfin_get(url.clone(), &params)?;
            let page = value
                .get("Items")
                .and_then(Value::as_array)
                .ok_or(Error::InvalidResponse)?;
            let total = value.get("TotalRecordCount").and_then(Value::as_u64);
            if total.is_some_and(|n| n > MAX_ITEMS as u64) {
                return Err(Error::LibraryLimit);
            }
            for item in page {
                let id = required_text(item, "Id")?;
                let entry_id = text(item, "PlaylistItemId");
                if !seen.insert((id, entry_id)) {
                    return Err(Error::LibraryLimit);
                }
                items.push(item.clone());
            }
            if items.len() > MAX_ITEMS {
                return Err(Error::LibraryLimit);
            }
            if let Some(total) = total {
                if items.len() as u64 >= total {
                    break;
                }
                if page.is_empty() {
                    return Err(Error::InvalidResponse);
                }
            } else if page.len() < PAGE {
                break;
            }
        }
        Ok(items)
    }

    fn subsonic(&self, method: &str, params: &[(&str, String)]) -> Result<Value> {
        let url = self.subsonic_url(method)?;
        let body = json_response(self.http.get(url).query(params))?;
        let response = body
            .get("subsonic-response")
            .ok_or(Error::InvalidResponse)?;
        match response.get("status").and_then(Value::as_str) {
            Some("ok") => Ok(response.clone()),
            Some("failed") => Err(Error::Api(
                response
                    .pointer("/error/code")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            )),
            _ => Err(Error::InvalidResponse),
        }
    }

    fn subsonic_url(&self, method: &str) -> Result<Url> {
        let Auth::Subsonic {
            username,
            salt,
            token,
        } = &self.auth
        else {
            return Err(Error::Unsupported);
        };
        let mut url = self.url(&["rest", &format!("{method}.view")])?;
        url.query_pairs_mut()
            .append_pair("u", username)
            .append_pair("t", token)
            .append_pair("s", salt)
            .append_pair("v", "1.16.1")
            .append_pair("c", "feishin-rs")
            .append_pair("f", "json");
        Ok(url)
    }

    fn jellyfin_get(&self, url: Url, params: &[(&str, String)]) -> Result<Value> {
        json_response(self.jellyfin_auth(self.http.get(url).query(params)))
    }

    fn jellyfin_auth(&self, request: RequestBuilder) -> RequestBuilder {
        let Auth::Jellyfin { token, .. } = &self.auth else {
            return request;
        };
        request.header("Authorization", jellyfin_authorization(Some(token)))
    }

    fn url(&self, segments: &[&str]) -> Result<Url> {
        endpoint(&self.base, segments)
    }
}

fn parse_base(value: &str) -> Result<Url> {
    let mut url = Url::parse(value.trim()).map_err(|_| Error::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidUrl);
    }
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

fn endpoint(base: &Url, segments: &[&str]) -> Result<Url> {
    let mut url = base.clone();
    let mut path = url.path_segments_mut().map_err(|_| Error::InvalidUrl)?;
    path.pop_if_empty();
    for segment in segments {
        if segment.is_empty() || *segment == "." || *segment == ".." {
            return Err(Error::InvalidResponse);
        }
        path.push(segment);
    }
    drop(path);
    Ok(url)
}

fn jellyfin_authorization(token: Option<&str>) -> String {
    let mut value = format!(
        "MediaBrowser Client=\"feishin-rs\", Device=\"Desktop\", DeviceId=\"{DEVICE}\", Version=\"0.1.0\""
    );
    if let Some(token) = token {
        value.push_str(&format!(", Token=\"{token}\""));
    }
    value
}

fn subsonic_token(password: &str, salt: &str) -> String {
    let mut digest = Md5::new();
    digest.update(password.as_bytes());
    digest.update(salt.as_bytes());
    format!("{:x}", digest.finalize())
}

fn checked_response(request: RequestBuilder) -> Result<Response> {
    let response = request.send().map_err(network_error)?;
    if !response.status().is_success() {
        return Err(Error::Http(response.status().as_u16()));
    }
    Ok(response)
}

fn read_bounded(response: Response, limit: u64) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|size| size > limit) {
        return Err(Error::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Network)?;
    if bytes.len() as u64 > limit {
        return Err(Error::ResponseTooLarge);
    }
    Ok(bytes)
}

fn json_response(request: RequestBuilder) -> Result<Value> {
    let bytes = read_bounded(checked_response(request)?, MAX_JSON)?;
    serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)
}

fn network_error(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Network
    }
}
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
fn required_text(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or(Error::InvalidResponse)
}
fn number(value: &Value, key: &str) -> u32 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or_default()
}
fn seconds(value: &Value, key: &str) -> f64 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .unwrap_or_default()
}
fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}
fn subsonic_favorite(value: &Value) -> bool {
    value.get("starred").is_some_and(|v| !v.is_null())
}
fn jellyfin_favorite(value: &Value) -> bool {
    value
        .pointer("/UserData/IsFavorite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
fn jellyfin_fields() -> &'static str {
    "Genres,ParentId,PrimaryImageAspectRatio,MediaSources,SortName,ChildCount,ItemCounts"
}
fn genres(albums: &[Album], tracks: &[Track]) -> Vec<String> {
    let mut genres = albums
        .iter()
        .map(|a| a.genre.clone())
        .chain(tracks.iter().map(|t| t.genre.clone()))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    genres.sort();
    genres.dedup();
    genres
}

fn subsonic_track(value: &Value) -> Result<Track> {
    Ok(Track {
        id: required_text(value, "id")?,
        title: text(value, "title"),
        artist: text(value, "artist"),
        album: text(value, "album"),
        album_id: text(value, "albumId"),
        artist_id: text(value, "artistId"),
        duration: seconds(value, "duration"),
        number: number(value, "track"),
        disc: number(value, "discNumber"),
        year: number(value, "year"),
        genre: text(value, "genre"),
        cover_id: text(value, "coverArt"),
        favorite: subsonic_favorite(value),
    })
}
fn subsonic_album(value: &Value) -> Result<Album> {
    Ok(Album {
        id: required_text(value, "id")?,
        title: if text(value, "name").is_empty() {
            text(value, "title")
        } else {
            text(value, "name")
        },
        artist: text(value, "artist"),
        artist_id: text(value, "artistId"),
        cover_id: text(value, "coverArt"),
        year: number(value, "year"),
        genre: text(value, "genre"),
        song_count: number(value, "songCount"),
        duration: seconds(value, "duration"),
        favorite: subsonic_favorite(value),
    })
}
fn subsonic_artist(value: &Value) -> Result<Artist> {
    Ok(Artist {
        id: required_text(value, "id")?,
        name: text(value, "name"),
        cover_id: text(value, "coverArt"),
        album_count: number(value, "albumCount"),
    })
}
fn subsonic_playlist(value: &Value) -> Result<Playlist> {
    Ok(Playlist {
        id: required_text(value, "id")?,
        name: text(value, "name"),
        cover_id: text(value, "coverArt"),
        song_count: number(value, "songCount"),
        duration: seconds(value, "duration"),
    })
}
fn jellyfin_artist_name(value: &Value) -> String {
    let names = array(value, "Artists")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if names.is_empty() {
        array(value, "AlbumArtists")
            .iter()
            .map(|v| text(v, "Name"))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        names
    }
}
fn jellyfin_artist_id(value: &Value) -> String {
    array(value, "ArtistItems")
        .first()
        .or_else(|| array(value, "AlbumArtists").first())
        .map(|v| text(v, "Id"))
        .unwrap_or_default()
}
fn jellyfin_cover(value: &Value) -> String {
    if value.pointer("/ImageTags/Primary").is_some() {
        text(value, "Id")
    } else {
        text(value, "AlbumId")
    }
}
fn jellyfin_genre(value: &Value) -> String {
    array(value, "Genres")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}
fn jellyfin_track(value: &Value) -> Result<Track> {
    Ok(Track {
        id: required_text(value, "Id")?,
        title: text(value, "Name"),
        artist: jellyfin_artist_name(value),
        album: text(value, "Album"),
        album_id: text(value, "AlbumId"),
        artist_id: jellyfin_artist_id(value),
        duration: seconds(value, "RunTimeTicks") / 10_000_000.0,
        number: number(value, "IndexNumber"),
        disc: number(value, "ParentIndexNumber"),
        year: number(value, "ProductionYear"),
        genre: jellyfin_genre(value),
        cover_id: jellyfin_cover(value),
        favorite: jellyfin_favorite(value),
    })
}
fn jellyfin_album(value: &Value) -> Result<Album> {
    Ok(Album {
        id: required_text(value, "Id")?,
        title: text(value, "Name"),
        artist: jellyfin_artist_name(value),
        artist_id: jellyfin_artist_id(value),
        cover_id: jellyfin_cover(value),
        year: number(value, "ProductionYear"),
        genre: jellyfin_genre(value),
        song_count: number(value, "ChildCount"),
        duration: seconds(value, "RunTimeTicks") / 10_000_000.0,
        favorite: jellyfin_favorite(value),
    })
}
fn jellyfin_artist(value: &Value) -> Result<Artist> {
    Ok(Artist {
        id: required_text(value, "Id")?,
        name: text(value, "Name"),
        cover_id: jellyfin_cover(value),
        album_count: number(value, "AlbumCount"),
    })
}
fn jellyfin_playlist(value: &Value) -> Result<Playlist> {
    Ok(Playlist {
        id: required_text(value, "Id")?,
        name: text(value, "Name"),
        cover_id: jellyfin_cover(value),
        song_count: number(value, "ChildCount"),
        duration: seconds(value, "RunTimeTicks") / 10_000_000.0,
    })
}

fn ticks(position: f64) -> u64 {
    if position.is_finite() && position > 0.0 {
        (position * 10_000_000.0).min(i64::MAX as f64) as u64
    } else {
        0
    }
}
