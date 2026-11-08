//! Native application composition. No network or MPV I/O runs in the UI pass.
use feishin_core::{Album, Library, Playlist, Queue, Repeat, ServerProfile, Track};
use feishin_desktop::{Desktop, DesktopCommand, DesktopState};
use feishin_platform::Settings;
use feishin_player_mpv::{Command, Config, EndReason, Event, Player};
use feishin_servers::{Client, ItemKind, StreamSource};
use feishin_ui_egui::{Action, Page, View};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

enum Message {
    Connected(u64, ServerProfile, Box<Result<(Client, Library), String>>),
    Library(u64, u64, bool, Result<Library, String>),
    Detail(u64, u64, Result<Vec<Track>, String>),
    Artist(u64, u64, Result<Vec<Album>, String>),
    Stream(u64, u64, Result<StreamSource, String>),
    Artwork(
        u64,
        String,
        Result<(egui::ColorImage, egui::Color32), String>,
    ),
    Lyrics(u64, String, Result<String, String>),
    Favorite(u64, String, bool, Result<(), String>),
    Playlists(u64, Result<PlaylistUpdate, String>),
    Notice(u64, Result<(), String>),
}

struct PlaylistUpdate {
    playlists: Vec<Playlist>,
    detail: Option<(String, Vec<Track>)>,
    deleted: Option<String>,
}

enum PlaylistMutation {
    Create(String),
    Delete(String),
    Add(String, Vec<String>),
    Remove(String, usize),
}

enum ReportKind {
    Start,
    Complete,
    Progress(f64, bool),
    Stop(f64),
}
struct Report {
    session: u64,
    client: Client,
    track: Track,
    kind: ReportKind,
}
enum ReportTask {
    Send(Box<Report>),
    Flush(mpsc::Sender<()>),
}

struct Application {
    view: View,
    client: Option<Client>,
    normal_library: Library,
    search_library: Option<Library>,
    queue_profile: Option<ServerProfile>,
    player: Option<Player>,
    player_ready: bool,
    desktop: Option<Desktop>,
    seek_target: Option<f64>,
    active_track: Option<Track>,
    report_tx: mpsc::SyncSender<ReportTask>,
    desired_paused: bool,
    reported_at: Instant,
    pending_source: Option<StreamSource>,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    session: u64,
    browse: u64,
    stream: u64,
    artwork_pending: HashSet<String>,
    artwork_done: HashSet<String>,
    config_dir: Option<PathBuf>,
    persistence_enabled: bool,
    saved_at: Instant,
    dirty: bool,
    playlist_busy: bool,
    screenshot: Option<PathBuf>,
    frame_count: u32,
    screenshot_requested: bool,
    started_at: Instant,
}

impl Application {
    fn new(ctx: &egui::Context, demo: bool, screenshot: Option<PathBuf>) -> Self {
        View::install(ctx);
        let (tx, rx) = mpsc::channel();
        let (report_tx, report_rx) = mpsc::sync_channel::<ReportTask>(128);
        let notices = tx.clone();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(task) = report_rx.recv() {
                let report = match task {
                    ReportTask::Send(report) => *report,
                    ReportTask::Flush(done) => {
                        let _ = done.send(());
                        continue;
                    }
                };
                let result = match report.kind {
                    ReportKind::Start => report.client.scrobble(&report.track, false),
                    ReportKind::Complete => report.client.scrobble(&report.track, true),
                    ReportKind::Progress(position, paused) => {
                        report
                            .client
                            .playback_progress(&report.track, position, paused)
                    }
                    ReportKind::Stop(position) => {
                        report.client.playback_stopped(&report.track, position)
                    }
                };
                if let Err(error) = result {
                    let _ = notices.send(Message::Notice(report.session, Err(error.to_string())));
                    wake.request_repaint();
                }
            }
        });
        let mut view = View::default();
        let config_dir = feishin_platform::config_dir().ok();
        let mut persistence_enabled = true;
        let mut queue_profile = None;
        if !demo {
            match config_dir
                .as_deref()
                .map(feishin_platform::load)
                .transpose()
            {
                Ok(Some(settings)) => {
                    if let Some(profile) = settings.server {
                        queue_profile = Some(profile.clone());
                        view.profile = profile;
                    }
                    view.mpv_path = settings.mpv_path;
                    view.playback.volume = settings.volume;
                    view.queue = settings.queue;
                }
                Ok(None) => {
                    view.error =
                        Some("Cannot find a settings directory. Set FEISHIN_RS_CONFIG_DIR.".into());
                    persistence_enabled = false;
                }
                Err(error) => {
                    view.error = Some(format!(
                        "{error}. Settings will not be overwritten this session."
                    ));
                    persistence_enabled = false;
                }
            }
            view.page = Page::Settings;
        } else {
            populate_demo(&mut view);
        }
        let normal_library = view.library.clone();
        let desktop = if cfg!(target_os = "linux") && !demo {
            Desktop::start().ok()
        } else {
            None
        };
        Self {
            view,
            client: None,
            normal_library,
            search_library: None,
            queue_profile,
            player: None,
            player_ready: false,
            desktop,
            seek_target: None,
            active_track: None,
            report_tx,
            desired_paused: true,
            reported_at: Instant::now(),
            pending_source: None,
            tx,
            rx,
            session: 0,
            browse: 0,
            stream: 0,
            artwork_pending: HashSet::new(),
            artwork_done: HashSet::new(),
            config_dir,
            persistence_enabled,
            saved_at: Instant::now(),
            dirty: false,
            playlist_busy: false,
            screenshot,
            frame_count: 0,
            screenshot_requested: false,
            started_at: Instant::now(),
        }
    }

    fn background(&self, ctx: &egui::Context, work: impl FnOnce() -> Message + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let message = work();
            let _ = tx.send(message);
            ctx.request_repaint();
        });
    }

    fn send(&mut self, command: Command) {
        if let Some(player) = &self.player
            && let Err(error) = player.send(command)
        {
            self.view.error = Some(error.to_string());
        }
    }

    fn stop(&mut self) {
        self.finish_active(false);
        self.seek_target = None;
        self.stream = self.stream.wrapping_add(1);
        self.pending_source = None;
        self.send(Command::Stop);
        self.view.playback.paused = true;
        self.desired_paused = true;
        self.view.playback.buffering = false;
        self.view.playback.position = 0.0;
    }

    fn report(&mut self, track: Track, kind: ReportKind) {
        if let Some(client) = self.client.clone()
            && self
                .report_tx
                .try_send(ReportTask::Send(Box::new(Report {
                    session: self.session,
                    client,
                    track,
                    kind,
                })))
                .is_err()
        {
            self.view.message =
                Some("Playback reporting is busy; some listening updates were not sent.".into());
        }
    }

    fn finish_active(&mut self, completed: bool) {
        if let Some(track) = self.active_track.take() {
            self.report(
                track,
                if completed {
                    ReportKind::Complete
                } else {
                    ReportKind::Stop(self.view.playback.position)
                },
            );
        }
    }

    fn play_current(&mut self, ctx: &egui::Context) {
        let Some(track) = self.view.queue.current_track().cloned() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            self.view.error = Some(
                if self.view.demo {
                    "Demo mode shows sample data. Connect a music server to play real audio."
                } else {
                    "Connect to your music server before playing the restored queue."
                }
                .into(),
            );
            self.view.page = Page::Settings;
            return;
        };
        self.finish_active(false);
        self.stream = self.stream.wrapping_add(1);
        self.pending_source = None;
        self.send(Command::Stop);
        self.view.playback.position = 0.0;
        self.view.playback.duration = track.duration;
        self.view.playback.paused = false;
        self.desired_paused = false;
        self.view.playback.buffering = true;
        let (session, stream) = (self.session, self.stream);
        self.background(ctx, move || {
            Message::Stream(
                session,
                stream,
                client.stream(&track).map_err(|e| e.to_string()),
            )
        });
        self.dirty = true;
    }

    fn load_pending(&mut self) {
        if self.player_ready
            && let Some(source) = self.pending_source.take()
        {
            self.send(Command::Load {
                generation: self.stream,
                paused: self.desired_paused,
                url: source.url,
                headers: source.headers,
            });
            self.send(Command::Volume(self.view.playback.volume));
        }
    }

    fn action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Connect { profile, password } => {
                self.stop();
                self.session = self.session.wrapping_add(1);
                self.browse = self.browse.wrapping_add(1);
                self.client = None;
                self.view.connected = false;
                self.view.clear_navigation_history();
                self.playlist_busy = false;
                self.view.loading = true;
                self.view.error = None;
                if self.view.demo || self.queue_profile.as_ref() != Some(&profile) {
                    self.view.queue.clear();
                }
                self.view.demo = false;
                self.view.library = Library::default();
                self.normal_library = Library::default();
                self.search_library = None;
                self.view.detail_tracks.clear();
                self.view.artist_albums.clear();
                self.view.clear_artwork();
                self.artwork_pending.clear();
                self.artwork_done.clear();
                self.view.profile = profile.clone();
                self.view.password.clear();
                self.view.lyrics.clear();
                let session = self.session;
                self.background(ctx, move || {
                    let result = Client::login(profile.clone(), &password).and_then(|client| {
                        let library = client.library()?;
                        Ok((client, library))
                    });
                    Message::Connected(
                        session,
                        profile,
                        Box::new(result.map_err(|e| e.to_string())),
                    )
                });
            }
            Action::Disconnect => {
                self.stop();
                self.session = self.session.wrapping_add(1);
                self.client = None;
                self.view.connected = false;
                self.view.clear_navigation_history();
                self.view.loading = false;
                self.playlist_busy = false;
                self.normal_library = Library::default();
                self.search_library = None;
                self.view.library = Library::default();
                self.view.queue.clear();
                self.view.detail_tracks.clear();
                self.view.artist_albums.clear();
                self.view.selected_album = None;
                self.view.selected_artist = None;
                self.view.selected_playlist = None;
                self.view.lyrics.clear();
                self.view.password.clear();
                self.artwork_done.clear();
                self.artwork_pending.clear();
                self.view.clear_artwork();
                self.view.page = Page::Settings;
                self.dirty = true;
            }
            Action::Refresh | Action::Search(_) => {
                if matches!(action, Action::Refresh) {
                    self.artwork_done.clear();
                }
                if let Some(client) = self.client.clone() {
                    self.browse = self.browse.wrapping_add(1);
                    let (session, browse) = (self.session, self.browse);
                    let query = match action {
                        Action::Search(query) => Some(query),
                        _ => None,
                    };
                    self.view.loading = true;
                    let searching = query.is_some();
                    self.background(ctx, move || {
                        Message::Library(
                            session,
                            browse,
                            searching,
                            match query {
                                Some(query) => client.search(&query),
                                None => client.library(),
                            }
                            .map_err(|e| e.to_string()),
                        )
                    });
                }
            }
            Action::OpenAlbum(id) => self.open_detail(ctx, id, false),
            Action::OpenPlaylist(id) => self.open_detail(ctx, id, true),
            Action::OpenArtist(id) => {
                self.view.selected_artist = self
                    .view
                    .library
                    .artists
                    .iter()
                    .chain(&self.normal_library.artists)
                    .find(|a| a.id == id)
                    .cloned();
                self.view.page = Page::Artist;
                if let Some(client) = self.client.clone() {
                    self.browse = self.browse.wrapping_add(1);
                    let (session, browse) = (self.session, self.browse);
                    self.view.artist_albums.clear();
                    self.view.loading = true;
                    self.view.detail_tracks.clear();
                    self.background(ctx, move || {
                        Message::Artist(
                            session,
                            browse,
                            client.artist_albums(&id).map_err(|e| e.to_string()),
                        )
                    });
                } else if self.view.demo {
                    self.view.artist_albums = self
                        .view
                        .library
                        .albums
                        .iter()
                        .filter(|a| a.artist_id == id)
                        .cloned()
                        .collect();
                }
            }
            Action::Play { tracks, start } => {
                if self.view.queue.replace(tracks, start) {
                    self.play_current(ctx);
                }
            }
            Action::QueueNext(tracks) => {
                self.view.queue.insert_next(tracks);
                self.dirty = true;
            }
            Action::QueueLast(tracks) => {
                self.view.queue.append(tracks);
                self.dirty = true;
            }
            Action::Next => {
                if self.view.queue.advance(false).is_some() {
                    self.play_current(ctx);
                } else {
                    self.stop();
                }
            }
            Action::Previous => {
                if self.view.playback.position > 3.0 {
                    self.send(Command::Seek(0.0));
                } else if self.view.queue.previous().is_some() {
                    self.play_current(ctx);
                }
            }
            Action::TogglePause => {
                if self.view.queue.current_track().is_none() && !self.view.queue.tracks.is_empty() {
                    self.view.queue.select(0);
                }
                if self.active_track.is_none() && !self.view.playback.buffering {
                    self.play_current(ctx);
                } else {
                    self.desired_paused = !self.desired_paused;
                    self.view.playback.paused = self.desired_paused;
                    if self.active_track.is_some() {
                        self.send(Command::Pause(self.desired_paused));
                    }
                    if let Some(track) = self.active_track.clone() {
                        self.report(
                            track,
                            ReportKind::Progress(
                                self.view.playback.position,
                                self.view.playback.paused,
                            ),
                        );
                    }
                }
            }
            Action::Stop => self.stop(),
            Action::Seek(position) => {
                if position.is_finite() {
                    self.seek_target = Some(position.max(0.0));
                    self.send(Command::Seek(position.max(0.0)));
                }
            }
            Action::Volume(volume) => {
                if volume.is_finite() {
                    self.view.playback.volume = volume.clamp(0.0, 100.0);
                    self.send(Command::Volume(self.view.playback.volume));
                    self.dirty = true;
                }
            }
            Action::ToggleShuffle => {
                self.view.queue.set_shuffle(!self.view.queue.shuffle);
                self.dirty = true;
            }
            Action::CycleRepeat => {
                self.view.queue.repeat = match self.view.queue.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };
                self.dirty = true;
            }
            Action::SelectQueue(index) => {
                if self.view.queue.select(index) {
                    self.play_current(ctx);
                }
            }
            Action::RemoveQueue(index) => {
                if self.view.queue.remove(index) {
                    if self.view.queue.current_track().is_some() {
                        self.play_current(ctx);
                    } else {
                        self.stop();
                    }
                }
                self.dirty = true;
            }
            Action::ClearQueue => {
                self.stop();
                self.view.queue.clear();
                self.dirty = true;
            }
            Action::Favorite { track_id, favorite } => {
                if let Some(client) = self.client.clone() {
                    let session = self.session;
                    self.background(ctx, move || {
                        Message::Favorite(
                            session,
                            track_id.clone(),
                            favorite,
                            client
                                .set_favorite(&track_id, ItemKind::Track, favorite)
                                .map_err(|e| e.to_string()),
                        )
                    });
                }
            }
            Action::Lyrics(id) => {
                let track = self
                    .view
                    .queue
                    .tracks
                    .iter()
                    .chain(&self.view.library.tracks)
                    .chain(&self.view.detail_tracks)
                    .find(|t| t.id == id)
                    .cloned();
                if let (Some(client), Some(track)) = (self.client.clone(), track) {
                    let session = self.session;
                    self.view.lyrics = "Loading lyrics…".into();
                    self.background(ctx, move || {
                        Message::Lyrics(
                            session,
                            id,
                            client.lyrics(&track).map_err(|e| e.to_string()),
                        )
                    });
                }
            }
            Action::SaveSettings { mpv_path } => {
                if mpv_path.trim().is_empty() {
                    self.view.error = Some("Choose an MPV executable path.".into());
                    return;
                }
                self.stop();
                self.player.take();
                self.player_ready = false;
                self.view.mpv_path = mpv_path;
                self.dirty = true;
                self.save();
            }
            Action::CreatePlaylist { name } => {
                self.mutate_playlist(ctx, PlaylistMutation::Create(name))
            }
            Action::DeletePlaylist { id } => {
                self.mutate_playlist(ctx, PlaylistMutation::Delete(id))
            }
            Action::AddToPlaylist {
                playlist_id,
                track_ids,
            } => self.mutate_playlist(ctx, PlaylistMutation::Add(playlist_id, track_ids)),
            Action::RemovePlaylistTrack { playlist_id, index } => {
                self.mutate_playlist(ctx, PlaylistMutation::Remove(playlist_id, index))
            }
        }
    }

    fn mutate_playlist(&mut self, ctx: &egui::Context, mutation: PlaylistMutation) {
        let Some(client) = self.client.clone() else {
            self.view.error = Some("Connect a music server to edit playlists.".into());
            return;
        };
        if self.playlist_busy {
            self.view.message = Some("Wait for the previous playlist change to finish.".into());
            return;
        }
        self.playlist_busy = true;
        let session = self.session;
        self.view.loading = true;
        self.background(ctx, move || {
            Message::Playlists(
                session,
                (|| {
                    let (detail, deleted) = match mutation {
                        PlaylistMutation::Create(name) => {
                            client.create_playlist(&name, &[])?;
                            (None, None)
                        }
                        PlaylistMutation::Delete(id) => {
                            client.delete_playlist(&id)?;
                            (None, Some(id))
                        }
                        PlaylistMutation::Add(id, tracks) => {
                            client.add_to_playlist(&id, &tracks)?;
                            let tracks = client.playlist_tracks(&id)?;
                            (Some((id, tracks)), None)
                        }
                        PlaylistMutation::Remove(id, index) => {
                            client.remove_playlist_track(&id, index)?;
                            let tracks = client.playlist_tracks(&id)?;
                            (Some((id, tracks)), None)
                        }
                    };
                    Ok::<_, feishin_servers::Error>(PlaylistUpdate {
                        playlists: client.playlists()?,
                        detail,
                        deleted,
                    })
                })()
                .map_err(|e| e.to_string()),
            )
        });
    }

    fn open_detail(&mut self, ctx: &egui::Context, id: String, playlist: bool) {
        if playlist {
            self.view.page = Page::Playlist;
            self.view.selected_playlist = self
                .view
                .library
                .playlists
                .iter()
                .chain(&self.normal_library.playlists)
                .find(|p| p.id == id)
                .cloned();
        } else {
            self.view.page = Page::Album;
            self.view.selected_album = self
                .view
                .library
                .albums
                .iter()
                .chain(&self.normal_library.albums)
                .chain(&self.view.artist_albums)
                .find(|a| a.id == id)
                .cloned();
        }
        self.view.detail_tracks.clear();
        if let Some(client) = self.client.clone() {
            self.browse = self.browse.wrapping_add(1);
            let (session, browse) = (self.session, self.browse);
            self.view.loading = true;
            self.background(ctx, move || {
                Message::Detail(
                    session,
                    browse,
                    if playlist {
                        client.playlist_tracks(&id)
                    } else {
                        client.album_tracks(&id)
                    }
                    .map_err(|e| e.to_string()),
                )
            });
        } else if self.view.demo {
            self.view.detail_tracks = self
                .normal_library
                .tracks
                .iter()
                .filter(|t| playlist || t.album_id == id)
                .take(if playlist { 18 } else { usize::MAX })
                .cloned()
                .collect();
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let mut commands = Vec::new();
        if let Some(desktop) = &self.desktop {
            while let Ok(command) = desktop.try_recv() {
                commands.push(command);
            }
        }
        for command in commands {
            let action = match command {
                DesktopCommand::Play
                    if self.active_track.is_none() || self.view.playback.paused =>
                {
                    Some(Action::TogglePause)
                }
                DesktopCommand::Pause
                    if self.active_track.is_some() && !self.view.playback.paused =>
                {
                    Some(Action::TogglePause)
                }
                DesktopCommand::TogglePause => Some(Action::TogglePause),
                DesktopCommand::Stop => Some(Action::Stop),
                DesktopCommand::Next => Some(Action::Next),
                DesktopCommand::Previous => Some(Action::Previous),
                DesktopCommand::Seek(position) => Some(Action::Seek(position)),
                DesktopCommand::Volume(volume) => Some(Action::Volume(volume)),
                DesktopCommand::Shuffle(shuffle) if self.view.queue.shuffle != shuffle => {
                    Some(Action::ToggleShuffle)
                }
                DesktopCommand::Repeat(repeat) => {
                    self.view.queue.repeat = repeat;
                    self.dirty = true;
                    None
                }
                DesktopCommand::Raise => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    None
                }
                DesktopCommand::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    None
                }
                DesktopCommand::Error(error) => {
                    self.view.message = Some(error);
                    None
                }
                _ => None,
            };
            if let Some(action) = action {
                self.action(ctx, action);
            }
        }
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Message::Connected(session, profile, result) if session == self.session => {
                    self.view.loading = false;
                    match *result {
                        Ok((client, library)) => {
                            self.client = Some(client);
                            self.normal_library = library.clone();
                            self.view.library = library;
                            self.view.connected = true;
                            self.queue_profile = Some(profile.clone());
                            self.view.profile = profile;
                            self.view.page = Page::Home;
                            self.view.message =
                                Some("Connected. Select an album to start listening.".into());
                            self.dirty = true;
                        }
                        Err(error) => self.view.error = Some(error),
                    }
                }
                Message::Library(session, browse, searching, result)
                    if session == self.session && browse == self.browse =>
                {
                    self.view.loading = false;
                    match result {
                        Ok(mut library) if searching => {
                            library.playlists = self.normal_library.playlists.clone();
                            if self.view.page == Page::Search {
                                self.view.library = library.clone();
                            }
                            self.search_library = Some(library);
                        }
                        Ok(library) => {
                            self.normal_library = library.clone();
                            self.view.library = library;
                        }
                        Err(e) => self.view.error = Some(e),
                    }
                }
                Message::Detail(session, browse, result)
                    if session == self.session && browse == self.browse =>
                {
                    self.view.loading = false;
                    match result {
                        Ok(tracks) => self.view.detail_tracks = tracks,
                        Err(e) => self.view.error = Some(e),
                    }
                }
                Message::Artist(session, browse, result)
                    if session == self.session && browse == self.browse =>
                {
                    self.view.loading = false;
                    match result {
                        Ok(albums) => {
                            let ids: HashSet<_> = albums.iter().map(|a| a.id.as_str()).collect();
                            self.view.detail_tracks = self
                                .normal_library
                                .tracks
                                .iter()
                                .filter(|t| ids.contains(t.album_id.as_str()))
                                .cloned()
                                .collect();
                            self.view.detail_tracks.sort_by(|a, b| {
                                a.album
                                    .cmp(&b.album)
                                    .then(a.disc.cmp(&b.disc))
                                    .then(a.number.cmp(&b.number))
                            });
                            self.view.artist_albums = albums;
                        }
                        Err(e) => self.view.error = Some(e),
                    }
                }
                Message::Stream(session, stream, result)
                    if session == self.session && stream == self.stream =>
                {
                    match result {
                        Ok(source) => {
                            self.pending_source = Some(source);
                            if self.player.is_none() {
                                let config = Config {
                                    executable: self.view.mpv_path.clone(),
                                    ..Default::default()
                                };
                                match Player::spawn(config) {
                                    Ok(player) => self.player = Some(player),
                                    Err(error) => self.view.error = Some(error.to_string()),
                                }
                            }
                            self.load_pending();
                        }
                        Err(error) => {
                            self.view.error = Some(error);
                            self.view.playback.buffering = false;
                            self.view.playback.paused = true;
                        }
                    }
                }
                Message::Artwork(session, id, result) if session == self.session => {
                    self.artwork_pending.remove(&id);
                    self.artwork_done.insert(id.clone());
                    if let Ok((image, tint)) = result {
                        self.view.set_artwork_tint(id.clone(), tint);
                        self.view.set_artwork(
                            id.clone(),
                            ctx.load_texture(id, image, egui::TextureOptions::LINEAR),
                        );
                    } else {
                        self.view.mark_artwork_unavailable(id);
                    }
                }
                Message::Lyrics(session, id, result) if session == self.session => {
                    if self.view.queue.current_track().is_some_and(|t| t.id == id) {
                        self.view.lyrics = match result {
                            Ok(text) if !text.trim().is_empty() => text,
                            Ok(_) => "No lyrics available for this track.".into(),
                            Err(e) => e,
                        };
                    }
                }
                Message::Favorite(session, id, favorite, result) if session == self.session => {
                    match result {
                        Ok(()) => {
                            for track in self
                                .view
                                .library
                                .tracks
                                .iter_mut()
                                .chain(&mut self.view.detail_tracks)
                                .chain(&mut self.view.queue.tracks)
                                .chain(&mut self.normal_library.tracks)
                            {
                                if track.id == id {
                                    track.favorite = favorite;
                                }
                            }
                            self.dirty = true;
                        }
                        Err(e) => self.view.error = Some(e),
                    }
                }
                Message::Playlists(session, result) if session == self.session => {
                    self.view.loading = false;
                    self.playlist_busy = false;
                    match result {
                        Ok(update) => {
                            self.normal_library.playlists = update.playlists.clone();
                            self.view.library.playlists = update.playlists;
                            if let Some((id, tracks)) = update.detail
                                && self.view.page == Page::Playlist
                                && self
                                    .view
                                    .selected_playlist
                                    .as_ref()
                                    .is_some_and(|p| p.id == id)
                            {
                                self.view.detail_tracks = tracks;
                                self.view.selected_playlist = self
                                    .normal_library
                                    .playlists
                                    .iter()
                                    .find(|p| p.id == id)
                                    .cloned();
                            }
                            if let Some(id) = update.deleted
                                && self.view.page == Page::Playlist
                                && self
                                    .view
                                    .selected_playlist
                                    .as_ref()
                                    .is_some_and(|p| p.id == id)
                            {
                                self.view.selected_playlist = None;
                                self.view.detail_tracks.clear();
                                self.view.page = Page::Home;
                            }
                            self.view.message = Some("Playlist updated.".into());
                        }
                        Err(error) => self.view.error = Some(error),
                    }
                }
                Message::Notice(session, Err(error)) if session == self.session => {
                    self.view.message = Some(error);
                }
                _ => {}
            }
        }
        let mut events = Vec::new();
        if let Some(player) = &self.player {
            while let Ok(event) = player.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            self.player_event(ctx, event);
        }
        if self.reported_at.elapsed() > Duration::from_secs(10) {
            if let Some(track) = self.active_track.clone() {
                self.report(
                    track,
                    ReportKind::Progress(self.view.playback.position, self.view.playback.paused),
                );
            }
            self.reported_at = Instant::now();
        }
        if let Some(client) = self.client.clone() {
            for id in self.view.needed_artwork() {
                if self.artwork_pending.len() >= 4 || self.artwork_done.len() >= 512 {
                    break;
                }
                if id.is_empty()
                    || self.artwork_done.contains(&id)
                    || !self.artwork_pending.insert(id.clone())
                {
                    continue;
                }
                let client = client.clone();
                let session = self.session;
                self.background(ctx, move || {
                    let image =
                        client
                            .cover(&id, 512)
                            .map_err(|e| e.to_string())
                            .and_then(|bytes| {
                                use image::ImageReader;
                                let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
                                    .with_guessed_format()
                                    .map_err(|_| "Unsupported artwork".to_owned())?;
                                let mut limits = image::Limits::default();
                                limits.max_image_width = Some(4096);
                                limits.max_image_height = Some(4096);
                                limits.max_alloc = Some(64 * 1024 * 1024);
                                reader.limits(limits);
                                let image = reader
                                    .decode()
                                    .map_err(|_| "Cannot decode artwork".to_owned())?
                                    .thumbnail(512, 512)
                                    .to_rgba8();
                                let image = egui::ColorImage::from_rgba_unmultiplied(
                                    [image.width() as usize, image.height() as usize],
                                    image.as_raw(),
                                );
                                let (mut r, mut g, mut b, mut count) = (0_u32, 0_u32, 0_u32, 0_u32);
                                for pixel in image.pixels.iter().step_by(16).filter(|p| p.a() > 0) {
                                    r += u32::from(pixel.r());
                                    g += u32::from(pixel.g());
                                    b += u32::from(pixel.b());
                                    count += 1;
                                }
                                let [r, g, b] = [r, g, b]
                                    .map(|channel| channel.checked_div(count).unwrap_or(40) as u8);
                                let tint = egui::Color32::from_rgb(r, g, b);
                                Ok((image, tint))
                            });
                    Message::Artwork(session, id, image)
                });
            }
        }
        if self.dirty && self.saved_at.elapsed() > Duration::from_secs(5) {
            self.save();
        }
        if let Some(desktop) = &self.desktop {
            let state = DesktopState {
                track: self.active_track.clone(),
                paused: self.view.playback.paused,
                position: self.view.playback.position,
                volume: self.view.playback.volume,
                can_next: self.view.queue.current.is_some_and(|i| {
                    i.saturating_add(1) < self.view.queue.tracks.len()
                        || self.view.queue.repeat == Repeat::All
                        || self.view.queue.shuffle && self.view.queue.tracks.len() > 1
                }),
                can_previous: self.view.queue.current_track().is_some(),
                shuffle: self.view.queue.shuffle,
                repeat: self.view.queue.repeat,
            };
            if let Err(error) = desktop.update(state) {
                self.view.message = Some(error.to_string());
            }
        }
    }

    fn player_event(&mut self, ctx: &egui::Context, event: Event) {
        match event {
            Event::Ready => {
                self.player_ready = true;
                self.load_pending();
            }
            Event::Pause(paused) if self.active_track.is_some() => {
                self.view.playback.paused = paused;
                self.desired_paused = paused;
            }
            Event::Position(value) => {
                self.view.playback.position = value;
                if self
                    .seek_target
                    .is_some_and(|target| (target - value).abs() < 1.0)
                {
                    self.seek_target = None;
                    if let Some(desktop) = &self.desktop {
                        let _ = desktop.seeked(value);
                    }
                }
            }
            Event::Duration(value) => self.view.playback.duration = value,
            Event::Volume(value) => self.view.playback.volume = value,
            Event::Buffering(value) => self.view.playback.buffering = value,
            Event::FileLoaded(generation) if generation == self.stream => {
                self.view.playback.buffering = false;
                self.send(Command::Pause(self.desired_paused));
                self.view.playback.paused = self.desired_paused;
                if let Some(track) = self.view.queue.current_track().cloned() {
                    self.active_track = Some(track.clone());
                    self.report(track, ReportKind::Start);
                    self.reported_at = Instant::now();
                }
            }
            Event::EndFile {
                generation,
                reason: EndReason::Eof,
            } if generation == self.stream => {
                self.finish_active(true);
                if self.view.queue.advance(true).is_some() {
                    self.play_current(ctx);
                } else {
                    self.view.playback.paused = true;
                    self.view.playback.buffering = false;
                }
            }
            Event::EndFile {
                generation,
                reason: EndReason::Error,
            } if generation == self.stream => {
                self.finish_active(false);
                self.view.error = Some("MPV could not play this stream. Check your server connection and media format.".into());
                self.view.playback.paused = true;
                self.view.playback.buffering = false;
            }
            Event::Error(error) => {
                self.view.error = Some(error);
                if self.active_track.is_none() {
                    self.view.playback.buffering = false;
                    self.view.playback.paused = true;
                }
            }
            Event::Disconnected => {
                self.finish_active(false);
                self.player.take();
                self.player_ready = false;
                self.pending_source = None;
                self.view.playback.paused = true;
                self.view.playback.buffering = false;
            }
            _ => {}
        }
    }

    fn save(&mut self) {
        if !self.persistence_enabled || self.view.demo {
            return;
        }
        if let Some(dir) = &self.config_dir {
            let settings = Settings {
                version: 1,
                server: self.queue_profile.clone(),
                mpv_path: self.view.mpv_path.clone(),
                volume: self.view.playback.volume,
                queue: self.view.queue.clone(),
            };
            match feishin_platform::save(dir, &settings) {
                Ok(()) => self.dirty = false,
                Err(error) => self.view.error = Some(error.to_string()),
            }
            self.saved_at = Instant::now();
        }
    }

    fn flush_reports(&self) {
        let (done, ack) = mpsc::channel();
        if self.report_tx.try_send(ReportTask::Flush(done)).is_ok() {
            let _ = ack.recv_timeout(Duration::from_secs(2));
        }
    }
}

impl eframe::App for Application {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        let old_page = self.view.page.clone();
        let actions = self.view.ui(ui);
        if self.view.page != old_page {
            if self.view.page == Page::Search {
                if let Some(library) = &self.search_library {
                    self.view.library = library.clone();
                }
            } else if old_page == Page::Search {
                self.view.library = self.normal_library.clone();
            }
        }
        for action in actions {
            self.action(&ctx, action);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        self.frame_count = self.frame_count.saturating_add(1);
        if self.screenshot.is_some() && !self.screenshot_requested && self.frame_count > 10 {
            self.screenshot_requested = true;
            if let Some(path) = self.screenshot.take() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::ScreenshotCallback::new(move |result| {
                        let rgba: Vec<u8> =
                            result.pixels.iter().flat_map(|c| c.to_array()).collect();
                        if let Err(e) = image::save_buffer(
                            &path,
                            &rgba,
                            result.width() as u32,
                            result.height() as u32,
                            image::ColorType::Rgba8,
                        ) {
                            eprintln!("Screenshot could not be saved: {e}");
                        }
                    }),
                ));
            }
        }
        if self.screenshot_requested && self.frame_count > 18
            || self.screenshot.is_some() && self.started_at.elapsed() > Duration::from_secs(20)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.finish_active(false);
        self.save();
        self.flush_reports();
    }
}

fn populate_demo(view: &mut View) {
    use feishin_core::{Artist, Playlist};
    view.demo = true;
    view.profile = ServerProfile {
        name: "Demo library".into(),
        url: "https://music.example".into(),
        ..Default::default()
    };
    let names = [
        ("After Hours", "Mira Vale", "Electronic"),
        ("Golden Days", "The Northern Lights", "Alternative"),
        ("Blue Horizon", "Luca Rivers", "Jazz"),
        ("First Light", "Paper Satellites", "Indie"),
        ("Quiet Places", "June & The Pines", "Folk"),
        ("Night Drive", "Mira Vale", "Electronic"),
        ("Soft Focus", "Luca Rivers", "Jazz"),
        ("Somewhere Else", "Paper Satellites", "Indie"),
    ];
    for (index, (title, artist, genre)) in names.into_iter().enumerate() {
        let id = format!("demo-{index}");
        let artist_id = artist.to_string();
        view.library.albums.push(Album {
            id: id.clone(),
            title: title.into(),
            artist: artist.into(),
            artist_id: artist_id.clone(),
            year: 2024 + (index % 3) as u32,
            genre: genre.into(),
            cover_id: id.clone(),
            song_count: 9,
            duration: 2127.0,
            ..Default::default()
        });
        if !view.library.artists.iter().any(|a| a.id == artist_id) {
            view.library.artists.push(Artist {
                id: artist_id.clone(),
                name: artist.into(),
                album_count: 2,
                cover_id: id.clone(),
            });
        }
        for (number, song) in [
            "Overture",
            "A Little Closer",
            "Between the Lines",
            "Daydream",
            "All We Know",
            "Interlude",
            "Stay a While",
            "Home Again",
            "Last Light",
        ]
        .iter()
        .enumerate()
        {
            view.library.tracks.push(Track {
                id: format!("{id}-{number}"),
                title: (*song).into(),
                artist: artist.into(),
                album: title.into(),
                album_id: id.clone(),
                artist_id: artist_id.clone(),
                number: (number + 1) as u32,
                disc: 1,
                duration: 194.0 + number as f64 * 8.0,
                cover_id: id.clone(),
                genre: genre.into(),
                year: 2025,
                ..Default::default()
            });
        }
    }
    view.library.genres = [
        "Electronic",
        "Alternative",
        "Jazz",
        "Indie",
        "Folk",
        "Ambient",
        "Pop",
        "Rock",
        "Soul",
    ]
    .map(String::from)
    .to_vec();
    view.library.playlists = ["On repeat", "Late nights", "Sunday morning"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Playlist {
            id: format!("playlist-{i}"),
            name: name.into(),
            song_count: 18,
            duration: 4254.0,
            cover_id: format!("demo-{i}"),
        })
        .collect();
    view.queue = Queue::default();
    view.queue
        .replace(view.library.tracks.iter().take(9).cloned().collect(), 0);
    view.playback.duration = 194.0;
    view.page = Page::Home;
    view.message = Some(
        "Demo mode · sample library only · connect your server in Settings to play music".into(),
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut demo = false;
    let mut demo_page = None;
    let mut screenshot = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--demo-page" => demo_page = Some(args.next().ok_or("--demo-page needs a view name")?),
            "--screenshot" => {
                screenshot = Some(PathBuf::from(
                    args.next().ok_or("--screenshot needs a PNG path")?,
                ))
            }
            "--help" | "-h" => {
                println!(
                    "feishin-rs\n\nNative Rust music client with MPV playback.\n\n  --demo                  Show an explicitly labeled sample library\n  --demo-page VIEW        Select a demo view (requires --demo)\n  --screenshot FILE.png   Capture the native window and exit\n  --version               Show the version\n\nSettings: FEISHIN_RS_CONFIG_DIR overrides the platform config directory.\nPasswords are kept in memory only."
                );
                return Ok(());
            }
            "--version" | "-V" => {
                println!("feishin-rs {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}. Use --help.").into()),
        }
    }
    if demo_page.is_some() && !demo {
        return Err("--demo-page requires --demo".into());
    }
    if let Some(page) = &demo_page
        && ![
            "home",
            "albums",
            "album",
            "artists",
            "artist",
            "tracks",
            "playlist",
            "settings",
            "queue",
            "now-playing",
            "lyrics",
        ]
        .contains(&page.as_str())
    {
        return Err(format!("Unknown demo page: {page}").into());
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Feishin")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "feishin-rs",
        options,
        Box::new(move |cc| {
            let mut app = Application::new(&cc.egui_ctx, demo, screenshot);
            if let Some(page) = &demo_page {
                app.view.set_preview(page);
            }
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (Application, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = Application::new(&ctx, true, None);
        app.config_dir = None;
        app.persistence_enabled = false;
        (app, ctx)
    }

    #[test]
    fn previous_sessions_and_superseded_searches_cannot_replace_current_data() {
        let (mut app, ctx) = app();
        app.session = 2;
        app.browse = 4;
        let initial_count = app.view.library.albums.len();
        app.tx
            .send(Message::Library(1, 4, false, Ok(Library::default())))
            .unwrap();
        app.tx
            .send(Message::Library(2, 3, false, Ok(Library::default())))
            .unwrap();
        app.tx
            .send(Message::Connected(
                1,
                ServerProfile::default(),
                Box::new(Err("stale login error".into())),
            ))
            .unwrap();
        app.poll(&ctx);
        assert_eq!(app.view.library.albums.len(), initial_count);
        assert!(app.view.error.is_none());
    }

    #[test]
    fn old_stream_sources_and_eof_do_not_start_or_skip_a_new_selection() {
        let (mut app, ctx) = app();
        app.stream = 3;
        app.view.queue.select(2);
        app.tx
            .send(Message::Stream(
                0,
                2,
                Ok(StreamSource {
                    url: "http://127.0.0.1/old-stream".into(),
                    headers: vec![],
                }),
            ))
            .unwrap();
        app.poll(&ctx);
        assert!(app.player.is_none());
        assert!(app.pending_source.is_none());
        app.player_event(&ctx, Event::FileLoaded(2));
        assert!(app.active_track.is_none());
        app.player_event(
            &ctx,
            Event::EndFile {
                generation: 2,
                reason: EndReason::Eof,
            },
        );
        assert_eq!(app.view.queue.current, Some(2));
        app.player_event(
            &ctx,
            Event::EndFile {
                generation: 3,
                reason: EndReason::Stop,
            },
        );
        assert_eq!(app.view.queue.current, Some(2));
        app.player_event(
            &ctx,
            Event::EndFile {
                generation: 3,
                reason: EndReason::Eof,
            },
        );
        assert_eq!(app.view.queue.current, Some(3));
    }

    #[test]
    fn delayed_playlist_update_cannot_overwrite_an_album_opened_after_it() {
        let (mut app, ctx) = app();
        app.view.page = Page::Album;
        app.view.selected_playlist = Some(Playlist {
            id: "p".into(),
            ..Default::default()
        });
        app.view.detail_tracks = vec![Track {
            id: "album-song".into(),
            ..Default::default()
        }];
        app.tx
            .send(Message::Playlists(
                0,
                Ok(PlaylistUpdate {
                    playlists: vec![],
                    detail: Some(("p".into(), vec![])),
                    deleted: Some("p".into()),
                }),
            ))
            .unwrap();
        app.poll(&ctx);
        assert_eq!(app.view.page, Page::Album);
        assert_eq!(app.view.detail_tracks.first().unwrap().id, "album-song");
    }

    #[test]
    fn artist_playback_collects_album_tracks_and_search_keeps_playlists() {
        let (mut app, ctx) = app();
        let album = app.normal_library.albums.first().unwrap().clone();
        app.tx
            .send(Message::Artist(0, 0, Ok(vec![album.clone()])))
            .unwrap();
        app.poll(&ctx);
        assert_eq!(app.view.detail_tracks.len(), 9);
        assert!(
            app.view
                .detail_tracks
                .iter()
                .all(|t| t.album_id == album.id)
        );
        app.view.page = Page::Search;
        app.tx
            .send(Message::Library(0, 0, true, Ok(Library::default())))
            .unwrap();
        app.poll(&ctx);
        assert_eq!(app.view.library.playlists.len(), 3);
        assert_eq!(app.normal_library.albums.len(), 8);
    }

    #[test]
    fn play_selects_first_item_added_to_an_empty_queue() {
        let (mut app, ctx) = app();
        let tracks = app.view.library.tracks.iter().take(2).cloned().collect();
        app.view.queue.clear();
        app.action(&ctx, Action::QueueLast(tracks));
        assert_eq!(app.view.queue.current, None);
        app.action(&ctx, Action::TogglePause);
        assert_eq!(app.view.queue.current, Some(0));
    }

    #[test]
    fn pause_intent_survives_idle_properties_and_delayed_file_loaded() {
        let (mut app, ctx) = app();
        app.stream = 5;
        app.desired_paused = false;
        app.view.playback.buffering = true;
        app.action(&ctx, Action::TogglePause);
        app.player_event(&ctx, Event::Pause(false));
        assert!(app.desired_paused);
        app.player_event(&ctx, Event::FileLoaded(5));
        assert!(app.view.playback.paused);
        assert!(app.desired_paused);
        app.flush_reports();
    }
}
