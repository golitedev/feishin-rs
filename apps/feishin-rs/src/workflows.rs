//! Optional server and desktop workflows, kept separate from playback composition.
use super::*;
use feishin_core::features::{Lyrics, PlaylistDetails, RadioStation, SmartPlaylist, TrackDetails};

pub(super) enum FeatureResult {
    Details(String, Result<TrackDetails, String>),
    Playlist(u64, String, Result<PlaylistDetails, String>),
    Rated(String, u8, Result<(), String>),
    Radios(Result<Vec<RadioStation>, String>),
    Smart(Result<Vec<SmartPlaylist>, String>),
    Lyrics(String, Result<Lyrics, String>),
    Credentials(u64, ServerProfile, Result<Option<String>, String>),
    CredentialSaved(u64, ServerProfile, Result<(), String>),
    Notice(Result<String, String>),
    Folders(Result<Vec<feishin_core::features::MusicFolder>, String>),
    Directory(u64, Result<feishin_core::features::Directory, String>),
    AlbumArtists(Result<Vec<feishin_core::Artist>, String>),
}

impl Application {
    pub(super) fn clear_server_features(&mut self) {
        self.view.capabilities = Default::default();
        self.view.ratings.clear();
        self.view.track_details = None;
        self.view.playlist_details = None;
        self.view.radio_stations.clear();
        self.view.smart_playlists.clear();
        self.view.structured_lyrics = Default::default();
        self.view.music_folders.clear();
        self.view.directory = None;
        self.view.album_artists.clear();
        self.inspected_track = None;
    }
    pub(super) fn feature_action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::MoveQueue { from, to } => {
                self.invalidate_preparation();
                if self.view.queue.move_track(from, to) {
                    self.dirty = true;
                }
            }
            Action::EnableAnalysis(enabled) => {
                self.view.preferences.visualizer_enabled = enabled;
                self.ensure_player();
                self.send(Command::EnableAnalysis(enabled));
                if !enabled {
                    self.view.audio_levels = [-120.0; 4];
                }
                self.dirty = true;
            }
            Action::LoadFolders => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.view.loading = true;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Folders(
                                client.music_folders().map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::OpenFolder { id, root } => {
                if let Some(client) = self.client.clone() {
                    self.browse = self.browse.wrapping_add(1);
                    let (session, browse) = (self.session, self.browse);
                    self.view.loading = true;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Directory(
                                browse,
                                (if root {
                                    client.folder_root(&id)
                                } else {
                                    client.directory(&id)
                                })
                                .map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::LoadAlbumArtists => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.view.loading = true;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::AlbumArtists(
                                client.album_artists().map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::SavePreferences(preferences) => {
                let mut preferences = *preferences;
                preferences.normalize();
                preferences.playback = preferences.playback.normalized();
                self.view.preferences = preferences;
                self.send(Command::ApplySettings(
                    self.view.preferences.playback.clone(),
                ));
                self.send(Command::EnableAnalysis(
                    self.view.preferences.visualizer_enabled,
                ));
                self.view.playback.volume = self
                    .view
                    .playback
                    .volume
                    .min(self.view.preferences.playback.volume_max);
                self.send(Command::Volume(self.view.playback.volume));
                self.dirty = true;
                self.save();
            }
            Action::ApplyPlaybackSettings(settings) => {
                self.view.preferences.playback = settings.normalized();
                self.send(Command::ApplySettings(
                    self.view.preferences.playback.clone(),
                ));
                self.dirty = true;
                self.save();
            }
            Action::RequestAudioDevices => {
                self.ensure_player();
                if self.player_ready {
                    self.send(Command::ListAudioDevices);
                }
            }
            Action::SaveServer(profile) => {
                if self.upsert_profile(profile) {
                    self.dirty = true;
                    self.save();
                }
            }
            Action::RemoveServer(index) => {
                if let Some(profile) = self.view.profiles.get(index).cloned() {
                    self.view.profiles.remove(index);
                    if self.queue_profile.as_ref().is_some_and(|p| {
                        feishin_platform::credentials::account_key(p)
                            == feishin_platform::credentials::account_key(&profile)
                    }) {
                        self.action(ctx, Action::Disconnect);
                        self.queue_profile = None;
                        self.view.profile = self.view.profiles.first().cloned().unwrap_or_default();
                    }
                    self.remembered_servers
                        .remove(&feishin_platform::credentials::account_key(&profile));
                    if let Some(worker) = &self.keyring
                        && let Err(error) = worker.delete(self.session, profile)
                    {
                        self.view.message = Some(error);
                    }
                    self.dirty = true;
                    self.save();
                }
            }
            Action::SelectServer(index) => {
                if let Some(profile) = self.view.profiles.get(index).cloned() {
                    self.view.profile = profile.clone();
                    self.view.password.clear();
                    self.view.remember_password = self
                        .remembered_servers
                        .contains(&feishin_platform::credentials::account_key(&profile));
                    self.view.page = Page::Settings;
                    if let Some(worker) = &self.keyring {
                        if self.view.remember_password {
                            if let Err(error) = worker.load(self.session, profile) {
                                self.view.message = Some(error);
                            }
                        } else {
                            worker.advance();
                        }
                    }
                }
            }
            Action::ForgetPassword => {
                let profile = self.view.profile.clone();
                self.remembered_servers
                    .remove(&feishin_platform::credentials::account_key(&profile));
                if let Some(worker) = &self.keyring
                    && let Err(error) = worker.delete(self.session, profile)
                {
                    self.view.message = Some(error);
                }
                self.view.remember_password = false;
                self.dirty = true;
                self.save();
            }
            Action::RateTrack { id, rating }
            | Action::RateAlbum { id, rating }
            | Action::RateArtist { id, rating } => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Rated(
                                id.clone(),
                                rating,
                                client.set_rating(&id, rating).map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::InspectTrack(id) => {
                self.inspected_track = Some(id.clone());
                self.view.track_details = None;
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Details(
                                id.clone(),
                                client.track_details(&id).map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::UpdatePlaylist { id, metadata } => {
                self.mutate_playlist(ctx, PlaylistMutation::Update(id, metadata))
            }
            Action::MovePlaylistTrack { id, from, to } => {
                self.mutate_playlist(ctx, PlaylistMutation::Move(id, from, to))
            }
            Action::LoadRadios => self.load_radios(ctx),
            Action::SaveRadio(station) => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.view.loading = true;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Radios(
                                (|| {
                                    if station.id.is_empty() {
                                        client.create_radio(&station)?;
                                    } else {
                                        client.update_radio(&station)?;
                                    }
                                    client.radio_stations()
                                })()
                                .map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::DeleteRadio(id) => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Radios(
                                client
                                    .delete_radio(&id)
                                    .and_then(|_| client.radio_stations())
                                    .map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::PlayRadio(station) => {
                let valid = reqwest::Url::parse(&station.stream_url).is_ok_and(|url| {
                    matches!(url.scheme(), "https" | "http") && url.host_str().is_some()
                });
                if !valid {
                    self.view.error = Some("Radio requires an HTTP or HTTPS stream URL.".into());
                    return;
                }
                self.stop();
                self.stream = self.allocate_generation();
                self.radio = true;
                self.view.queue.replace(
                    vec![Track {
                        id: format!("radio:{}", station.id),
                        title: station.name,
                        artist: "Internet radio".into(),
                        ..Default::default()
                    }],
                    0,
                );
                self.desired_paused = false;
                self.view.playback.paused = false;
                self.view.playback.buffering = true;
                self.pending_source = Some(StreamSource {
                    url: station.stream_url,
                    headers: vec![],
                    transcoded: false,
                });
                self.ensure_player();
                self.load_pending();
            }
            Action::DownloadTrack { track, destination } => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    let default_directory = self.view.preferences.download_directory.clone();
                    self.view.message = Some("Downloading the original audio file…".into());
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Notice(download(
                                &client,
                                &track,
                                &destination,
                                &default_directory,
                            )),
                        )
                    });
                }
            }
            Action::LoadSmartPlaylists => self.load_smart(ctx),
            Action::SaveSmartPlaylist(playlist) => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.view.loading = true;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Smart(
                                client
                                    .save_smart_playlist(&playlist)
                                    .and_then(|_| client.smart_playlists())
                                    .map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            Action::DeleteSmartPlaylist(id) => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.background(ctx, move || {
                        Message::feature(
                            session,
                            FeatureResult::Smart(
                                client
                                    .delete_smart_playlist(&id)
                                    .and_then(|_| client.smart_playlists())
                                    .map_err(|e| e.to_string()),
                            ),
                        )
                    });
                }
            }
            _ => self.view.error = Some("This action has no application handler yet.".into()),
        }
    }

    pub(super) fn upsert_profile(&mut self, profile: ServerProfile) -> bool {
        let profile = match feishin_platform::credentials::validate_profile(profile) {
            Ok(profile) => profile,
            Err(error) => {
                self.view.error = Some(error.to_string());
                return false;
            }
        };
        let key = feishin_platform::credentials::account_key(&profile);
        if let Some(existing) = self
            .view
            .profiles
            .iter_mut()
            .find(|p| feishin_platform::credentials::account_key(p) == key)
        {
            *existing = profile;
        } else {
            self.view.profiles.push(profile);
        }
        true
    }
    pub(super) fn ensure_player(&mut self) {
        if self.player.is_none() {
            let extra_args = match mpv_arguments::parse(&self.view.preferences.mpv_arguments) {
                Ok(arguments) => arguments,
                Err(error) => {
                    self.view.error = Some(error.into());
                    return;
                }
            };
            let config = Config {
                executable: self.view.mpv_path.clone(),
                extra_args,
                settings: self.view.preferences.playback.clone(),
                ..Default::default()
            };
            match Player::spawn(config) {
                Ok(player) => self.player = Some(player),
                Err(e) => self.view.error = Some(e.to_string()),
            }
        }
    }
    fn load_radios(&mut self, ctx: &egui::Context) {
        if let Some(client) = self.client.clone() {
            let session = self.session;
            self.view.loading = true;
            self.background(ctx, move || {
                Message::feature(
                    session,
                    FeatureResult::Radios(client.radio_stations().map_err(|e| e.to_string())),
                )
            });
        }
    }
    fn load_smart(&mut self, ctx: &egui::Context) {
        if let Some(client) = self.client.clone() {
            let session = self.session;
            self.view.loading = true;
            self.background(ctx, move || {
                Message::feature(
                    session,
                    FeatureResult::Smart(client.smart_playlists().map_err(|e| e.to_string())),
                )
            });
        }
    }
    pub(super) fn load_playlist_metadata(&self, ctx: &egui::Context, id: String) {
        if let Some(client) = self.client.clone() {
            let (session, browse) = (self.session, self.browse);
            self.background(ctx, move || {
                Message::feature(
                    session,
                    FeatureResult::Playlist(
                        browse,
                        id.clone(),
                        client.playlist_details(&id).map_err(|e| e.to_string()),
                    ),
                )
            });
        }
    }
    pub(super) fn handle_feature(&mut self, ctx: &egui::Context, result: FeatureResult) {
        match result {
            FeatureResult::Folders(result) => {
                self.view.loading = false;
                match result {
                    Ok(folders) => self.view.music_folders = folders,
                    Err(e) => self.view.error = Some(e),
                }
            }
            FeatureResult::Directory(browse, result) if browse == self.browse => {
                self.view.loading = false;
                match result {
                    Ok(directory) => self.view.directory = Some(directory),
                    Err(e) => self.view.error = Some(e),
                }
            }
            FeatureResult::AlbumArtists(result) => {
                self.view.loading = false;
                match result {
                    Ok(artists) => self.view.album_artists = artists,
                    Err(e) => self.view.error = Some(e),
                }
            }
            FeatureResult::Details(id, result) if self.inspected_track.as_ref() == Some(&id) => {
                match result {
                    Ok(details) => {
                        if let Some(rating) = details.rating {
                            self.view.ratings.insert(id, rating);
                        }
                        self.view.track_details = Some(details);
                    }
                    Err(e) => self.view.error = Some(e),
                }
            }
            FeatureResult::Playlist(browse, id, result)
                if browse == self.browse
                    && self
                        .view
                        .selected_playlist
                        .as_ref()
                        .is_some_and(|p| p.id == id) =>
            {
                match result {
                    Ok(details) => self.view.playlist_details = Some(details),
                    Err(e) => self.view.message = Some(e),
                }
            }
            FeatureResult::Rated(id, rating, result) => match result {
                Ok(()) => {
                    self.view.ratings.insert(id.clone(), rating);
                    if let Some(details) = self
                        .view
                        .track_details
                        .as_mut()
                        .filter(|d| d.track.id == id)
                    {
                        details.rating = Some(rating);
                    }
                }
                Err(e) => self.view.error = Some(e),
            },
            FeatureResult::Radios(result) => {
                self.view.loading = false;
                match result {
                    Ok(radios) => self.view.radio_stations = radios,
                    Err(e) => self.view.error = Some(e),
                }
            }
            FeatureResult::Smart(result) => {
                self.view.loading = false;
                match result {
                    Ok(lists) => {
                        self.view.smart_playlists = lists;
                        self.view.message = Some("Smart playlists updated.".into());
                    }
                    Err(e) => self.view.error = Some(e),
                }
            }
            FeatureResult::Lyrics(id, result)
                if self.view.queue.current_track().is_some_and(|t| t.id == id) =>
            {
                match result {
                    Ok(lyrics) => {
                        self.view.lyrics = lyrics.plain_text();
                        self.view.structured_lyrics = lyrics;
                    }
                    Err(e) => self.view.lyrics = e,
                }
            }
            FeatureResult::Credentials(revision, profile, result)
                if profile == self.view.profile
                    && self
                        .keyring
                        .as_ref()
                        .is_some_and(|worker| worker.current() == revision) =>
            {
                match result {
                    Ok(Some(password)) => {
                        self.view.remember_password = true;
                        self.action(ctx, Action::Connect { profile, password });
                    }
                    Ok(None) => {
                        self.view.message =
                            Some("Enter your password to connect to this server.".into())
                    }
                    Err(e) => self.view.message = Some(e),
                }
            }
            FeatureResult::CredentialSaved(revision, profile, result)
                if self
                    .keyring
                    .as_ref()
                    .is_some_and(|worker| worker.current() == revision) =>
            {
                match result {
                    Ok(()) => {
                        self.remembered_servers
                            .insert(feishin_platform::credentials::account_key(&profile));
                        self.dirty = true;
                        self.save();
                    }
                    Err(error) => self.view.message = Some(error),
                }
            }
            FeatureResult::Notice(result) => match result {
                Ok(message) => self.view.message = Some(message),
                Err(e) => self.view.error = Some(e),
            },
            _ => {}
        }
    }
}

fn download(
    client: &Client,
    track: &Track,
    destination: &str,
    default_directory: &str,
) -> Result<String, String> {
    let details = client.track_details(&track.id).map_err(|e| e.to_string())?;
    let extension: String = details
        .codec
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .collect();
    let title: String = track
        .title
        .chars()
        .filter(|c| c.is_alphanumeric() || " -_".contains(*c))
        .take(100)
        .collect();
    let filename = format!(
        "{}.{}",
        if title.is_empty() {
            "track"
        } else {
            title.trim()
        },
        if extension.is_empty() {
            "audio"
        } else {
            &extension
        }
    );
    let destination = if destination.trim().is_empty() {
        let dir = if default_directory.trim().is_empty() {
            std::env::var_os("HOME")
                .map(|p| PathBuf::from(p).join("Downloads/feishin-rs"))
                .ok_or("Choose a download directory")?
        } else {
            PathBuf::from(default_directory)
        };
        dir.join(filename)
    } else {
        let path = PathBuf::from(destination);
        if path.is_dir() {
            path.join(filename)
        } else {
            path
        }
    };
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(parent).map_err(|_| "Cannot create the download directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Cannot create the download staging file")?;
    let bytes = client
        .download(track, &mut file)
        .map_err(|e| e.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|_| "Cannot flush downloaded audio")?;
    file.persist_noclobber(&destination)
        .map_err(|_| "Cannot save download: the destination already exists or is not writable")?;
    Ok(format!(
        "Downloaded {bytes} bytes to {}",
        destination.display()
    ))
}
