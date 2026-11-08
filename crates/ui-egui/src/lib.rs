//! Rust-native presentation, independently drawn from observed Feishin screenshots.
//! No networking, media transport, credentials persistence, or background workers live here.
use egui::{
    Align, Color32, FontFamily, FontId, Frame, Layout, Pos2, Rect, RichText, Sense, Stroke, pos2,
    vec2,
};
use feishin_core::{
    Album, Artist, Library, Playlist, Queue, Repeat, ServerKind, ServerProfile, Track,
    duration_text,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

mod icons;
use icons::{Icon, icon_button};

const BG: Color32 = Color32::from_rgb(10, 10, 10);
const SIDE: Color32 = Color32::from_rgb(8, 8, 8);
const CARD: Color32 = Color32::from_rgb(20, 20, 20);
const LINE: Color32 = Color32::from_rgb(30, 30, 30);
const TEXT: Color32 = Color32::from_rgb(222, 222, 224);
const MUTED: Color32 = Color32::from_rgb(145, 145, 149);
const BLUE: Color32 = Color32::from_rgb(48, 113, 255);

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Home,
    Tracks,
    Albums,
    Artists,
    Settings,
    Album,
    Artist,
    Playlist,
    Search,
}

#[derive(Clone, Debug)]
pub struct PlaybackState {
    pub paused: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub buffering: bool,
}
impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            paused: true,
            position: 0.0,
            duration: 0.0,
            volume: 75.0,
            buffering: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    CreatePlaylist {
        name: String,
    },
    DeletePlaylist {
        id: String,
    },
    AddToPlaylist {
        playlist_id: String,
        track_ids: Vec<String>,
    },
    RemovePlaylistTrack {
        playlist_id: String,
        index: usize,
    },
    Connect {
        profile: ServerProfile,
        password: String,
    },
    Disconnect,
    Refresh,
    Search(String),
    OpenAlbum(String),
    OpenArtist(String),
    OpenPlaylist(String),
    Play {
        tracks: Vec<Track>,
        start: usize,
    },
    QueueNext(Vec<Track>),
    QueueLast(Vec<Track>),
    Previous,
    Next,
    TogglePause,
    Stop,
    Seek(f64),
    Volume(f64),
    ToggleShuffle,
    CycleRepeat,
    SelectQueue(usize),
    RemoveQueue(usize),
    ClearQueue,
    Favorite {
        track_id: String,
        favorite: bool,
    },
    Lyrics(String),
    SaveSettings {
        mpv_path: String,
    },
}

pub struct View {
    pub library: Library,
    pub queue: Queue,
    pub profile: ServerProfile,
    pub password: String,
    pub mpv_path: String,
    pub connected: bool,
    pub loading: bool,
    pub demo: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub playback: PlaybackState,
    pub lyrics: String,
    pub page: Page,
    pub selected_album: Option<Album>,
    pub selected_artist: Option<Artist>,
    pub selected_playlist: Option<Playlist>,
    pub detail_tracks: Vec<Track>,
    pub artist_albums: Vec<Album>,
    pub search: String,
    artwork: HashMap<String, egui::TextureHandle>,
    artwork_tints: HashMap<String, Color32>,
    unavailable_artwork: HashSet<String>,
    visible_artwork: RefCell<Vec<String>>,
    queue_open: bool,
    fullscreen: bool,
    lyrics_open: bool,
    collapsed_sidebar: bool,
    filter: String,
    genre: Option<String>,
    favorites_only: bool,
    sort: Sort,
    history: Vec<Route>,
    back_navigation: bool,
    playlist_name: String,
    creating_playlist: bool,
    delete_playlist_confirm: bool,
    seek_preview: Option<f64>,
}

#[derive(Clone)]
struct Route {
    page: Page,
    entity_id: Option<String>,
    filter: String,
    genre: Option<String>,
    search: String,
    favorites_only: bool,
    sort: Sort,
}
impl Route {
    fn same_destination(&self, other: &Self) -> bool {
        self.page == other.page && self.entity_id == other.entity_id
    }
}

struct Hero<'a> {
    kind: &'a str,
    title: &'a str,
    artist: &'a str,
    cover: &'a str,
    info: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Sort {
    #[default]
    Title,
    Artist,
    Year,
}

impl Default for View {
    fn default() -> Self {
        Self {
            library: Library::default(),
            queue: Queue::default(),
            profile: ServerProfile::default(),
            password: String::new(),
            mpv_path: "mpv".into(),
            connected: false,
            loading: false,
            demo: false,
            error: None,
            message: None,
            playback: PlaybackState::default(),
            lyrics: String::new(),
            page: Page::Home,
            selected_album: None,
            selected_artist: None,
            selected_playlist: None,
            detail_tracks: vec![],
            artist_albums: vec![],
            search: String::new(),
            artwork: HashMap::new(),
            artwork_tints: HashMap::new(),
            unavailable_artwork: HashSet::new(),
            visible_artwork: RefCell::new(vec![]),
            queue_open: false,
            fullscreen: false,
            lyrics_open: false,
            collapsed_sidebar: false,
            filter: String::new(),
            genre: None,
            favorites_only: false,
            sort: Sort::Title,
            history: vec![],
            back_navigation: false,
            playlist_name: String::new(),
            creating_playlist: false,
            delete_playlist_confirm: false,
            seek_preview: None,
        }
    }
}

/// Installs a consistent native theme and the separately licensed Poppins typeface.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Poppins".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Poppins-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "Poppins Bold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Poppins-Bold.ttf")).into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Poppins".into());
    fonts.families.insert(
        FontFamily::Name("bold".into()),
        vec!["Poppins Bold".into(), "Poppins".into()],
    );
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = CARD;
    style.visuals.extreme_bg_color = CARD;
    style.visuals.faint_bg_color = Color32::from_rgb(16, 16, 16);
    style.visuals.selection.bg_fill = BLUE;
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.inactive.bg_fill = CARD;
    style.visuals.widgets.inactive.weak_bg_fill = CARD;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(33, 33, 33);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(33, 33, 33);
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(42, 42, 42);
    style.visuals.widgets.active.bg_stroke = Stroke::NONE;
    style.spacing.item_spacing = vec2(10.0, 8.0);
    style.spacing.button_padding = vec2(12.0, 7.0);
    style.spacing.interact_size = vec2(28.0, 30.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.0));
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(24.0, FontFamily::Name("bold".into())),
    );
    ctx.set_style_of(egui::Theme::Dark, style);
}

impl View {
    pub fn install(ctx: &egui::Context) {
        install(ctx);
    }
    pub fn set_artwork(&mut self, id: String, texture: egui::TextureHandle) {
        self.artwork.insert(id, texture);
    }
    pub fn mark_artwork_unavailable(&mut self, id: String) {
        self.unavailable_artwork.insert(id);
    }
    pub fn set_artwork_tint(&mut self, id: String, color: Color32) {
        self.artwork_tints.insert(id, color);
    }
    pub fn clear_artwork(&mut self) {
        self.artwork.clear();
        self.artwork_tints.clear();
        self.unavailable_artwork.clear();
        if let Ok(mut visible) = self.visible_artwork.try_borrow_mut() {
            visible.clear();
        }
    }
    /// Ordered pending artwork: current track, selected detail, visible covers, then library.
    /// The cap bounds prefetching without excluding covers reached by scrolling later.
    pub fn needed_artwork(&self) -> Vec<String> {
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        let mut push = |id: &str| {
            if !id.is_empty()
                && ids.len() < 512
                && !self.artwork.contains_key(id)
                && !self.unavailable_artwork.contains(id)
                && seen.insert(id.to_owned())
            {
                ids.push(id.to_owned());
            }
        };
        if let Some(track) = self.queue.current_track() {
            push(&track.cover_id);
        }
        match self.page {
            Page::Album => {
                if let Some(album) = &self.selected_album {
                    push(&album.cover_id);
                }
            }
            Page::Artist => {
                if let Some(artist) = &self.selected_artist {
                    push(&artist.cover_id);
                }
            }
            Page::Playlist => {
                if let Some(playlist) = &self.selected_playlist {
                    push(&playlist.cover_id);
                }
            }
            _ => {}
        }
        if let Ok(visible) = self.visible_artwork.try_borrow() {
            for id in visible.iter() {
                push(id);
            }
        }
        if self.page == Page::Artist {
            for album in &self.artist_albums {
                push(&album.cover_id);
            }
        }
        for album in &self.library.albums {
            push(&album.cover_id);
        }
        for playlist in &self.library.playlists {
            push(&playlist.cover_id);
        }
        ids
    }

    /// Selects a deterministic visual-check view using this View's existing demo library.
    /// This helper never fetches data, connects to a server, or starts playback.
    pub fn set_preview(&mut self, name: &str) -> bool {
        if !self.demo {
            return false;
        }
        let page = match name {
            "home" | "queue" | "now-playing" | "lyrics" => Page::Home,
            "albums" => Page::Albums,
            "album" => Page::Album,
            "artists" => Page::Artists,
            "artist" => Page::Artist,
            "tracks" => Page::Tracks,
            "playlist" => Page::Playlist,
            "settings" => Page::Settings,
            _ => return false,
        };
        self.page = page;
        self.fullscreen = matches!(name, "now-playing" | "lyrics");
        self.queue_open = name == "queue";
        self.lyrics_open = name == "lyrics";
        self.filter.clear();
        self.genre = None;
        self.favorites_only = false;
        self.history.clear();
        self.delete_playlist_confirm = false;
        if name == "album" {
            self.selected_album = self.library.albums.first().cloned();
            self.detail_tracks = self
                .selected_album
                .as_ref()
                .map(|a| {
                    self.library
                        .tracks
                        .iter()
                        .filter(|t| t.album_id == a.id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
        } else if name == "artist" {
            self.selected_artist = self.library.artists.first().cloned();
            self.artist_albums = self
                .selected_artist
                .as_ref()
                .map(|a| {
                    self.library
                        .albums
                        .iter()
                        .filter(|album| album.artist_id == a.id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            self.detail_tracks = self
                .selected_artist
                .as_ref()
                .map(|a| {
                    self.library
                        .tracks
                        .iter()
                        .filter(|t| t.artist_id == a.id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
        } else if name == "playlist" {
            self.selected_playlist = self.library.playlists.first().cloned();
            // Demo playlists use the existing sample collection, never invented server results.
            let count = self
                .selected_playlist
                .as_ref()
                .map_or(0, |p| p.song_count as usize);
            self.detail_tracks = self.library.tracks.iter().take(count).cloned().collect();
        }
        if name == "lyrics" && self.lyrics.is_empty() {
            self.lyrics = "Sample lyrics for visual preview\n\nA quiet room, a little light\nThe record turns into the night\n\nDemo text written for feishin-rs".into();
        }
        true
    }
    pub fn navigate(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.filter.clear();
            self.genre = None;
            self.delete_playlist_confirm = false;
        }
    }
    fn route(&self) -> Route {
        let entity_id = match self.page {
            Page::Album => self.selected_album.as_ref().map(|a| a.id.clone()),
            Page::Artist => self.selected_artist.as_ref().map(|a| a.id.clone()),
            Page::Playlist => self.selected_playlist.as_ref().map(|p| p.id.clone()),
            _ => None,
        };
        Route {
            page: self.page.clone(),
            entity_id,
            filter: self.filter.clone(),
            genre: self.genre.clone(),
            search: self.search.clone(),
            favorites_only: self.favorites_only,
            sort: self.sort,
        }
    }
    pub fn clear_navigation_history(&mut self) {
        self.history.clear();
    }
    fn finish_navigation(&mut self, from: Route, actions: &[Action]) {
        if self.back_navigation {
            return;
        }
        let mut to = self.route();
        for action in actions {
            match action {
                Action::OpenAlbum(id) => {
                    to.page = Page::Album;
                    to.entity_id = Some(id.clone());
                }
                Action::OpenArtist(id) => {
                    to.page = Page::Artist;
                    to.entity_id = Some(id.clone());
                }
                Action::OpenPlaylist(id) => {
                    to.page = Page::Playlist;
                    to.entity_id = Some(id.clone());
                }
                _ => {}
            }
        }
        if !from.same_destination(&to) {
            if self.page == from.page {
                self.filter.clear();
                self.genre = None;
            }
            self.page = to.page;
            self.delete_playlist_confirm = false;
            self.history.push(from);
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
    }
    fn go_back(&mut self, actions: &mut Vec<Action>) {
        let Some(route) = self.history.pop() else {
            return;
        };
        self.back_navigation = true;
        self.page = route.page.clone();
        self.filter = route.filter;
        self.genre = route.genre;
        self.search = route.search;
        self.favorites_only = route.favorites_only;
        self.sort = route.sort;
        self.delete_playlist_confirm = false;
        match (route.page, route.entity_id) {
            (Page::Album, Some(id)) => {
                self.detail_tracks.clear();
                self.selected_album = self.library.albums.iter().find(|a| a.id == id).cloned();
                actions.push(Action::OpenAlbum(id));
            }
            (Page::Artist, Some(id)) => {
                self.detail_tracks.clear();
                self.artist_albums.clear();
                self.selected_artist = self.library.artists.iter().find(|a| a.id == id).cloned();
                actions.push(Action::OpenArtist(id));
            }
            (Page::Playlist, Some(id)) => {
                self.detail_tracks.clear();
                self.selected_playlist =
                    self.library.playlists.iter().find(|p| p.id == id).cloned();
                actions.push(Action::OpenPlaylist(id));
            }
            (Page::Search, _) => actions.push(Action::Search(self.search.clone())),
            _ => {}
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let from_route = self.route();
        self.back_navigation = false;
        if let Ok(mut visible) = self.visible_artwork.try_borrow_mut() {
            visible.clear();
        }
        let mut actions = vec![];
        self.keyboard(ui, &mut actions);
        egui::Panel::bottom("playback")
            .exact_size(88.0)
            .frame(
                Frame::new()
                    .fill(BG)
                    .stroke(Stroke::new(1.0, LINE))
                    .inner_margin(12),
            )
            .show(ui, |ui| self.playback_bar(ui, &mut actions));
        if self.fullscreen {
            egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(Color32::from_rgb(18, 23, 32))
                        .inner_margin(24),
                )
                .show(ui, |ui| self.now_playing(ui, &mut actions));
        } else {
            let width = if self.collapsed_sidebar { 66.0 } else { 270.0 };
            egui::Panel::left("library-sidebar")
                .exact_size(width)
                .resizable(false)
                .frame(
                    Frame::new()
                        .fill(SIDE)
                        .stroke(Stroke::new(1.0, LINE))
                        .inner_margin(10),
                )
                .show(ui, |ui| self.sidebar(ui, &mut actions));
            if self.queue_open {
                egui::Panel::right("queue")
                    .default_size(320.0)
                    .min_size(230.0)
                    .frame(Frame::new().fill(SIDE).inner_margin(16))
                    .show(ui, |ui| self.queue_panel(ui, &mut actions));
            }
            egui::CentralPanel::default()
                .frame(Frame::new().fill(BG).inner_margin(24))
                .show(ui, |ui| {
                    self.banners(ui);
                    egui::ScrollArea::vertical()
                        .id_salt("main-content")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            match self.page {
                                Page::Home => self.home(ui, &mut actions),
                                Page::Albums => self.albums_page(ui, &mut actions),
                                Page::Tracks => self.tracks_page(ui, &mut actions),
                                Page::Artists => self.artists_page(ui, &mut actions),
                                Page::Settings => self.settings(ui, &mut actions),
                                Page::Album => self.album_detail(ui, &mut actions),
                                Page::Artist => self.artist_detail(ui, &mut actions),
                                Page::Playlist => self.playlist_detail(ui, &mut actions),
                                Page::Search => self.search_page(ui, &mut actions),
                            }
                        });
                });
        }
        self.finish_navigation(from_route, &actions);
        actions
    }
    fn keyboard(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
                actions.push(Action::TogglePause);
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowRight) {
                actions.push(Action::Next);
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowLeft) {
                actions.push(Action::Previous);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                self.fullscreen = false;
                self.queue_open = false;
            }
        });
    }
    fn banners(&mut self, ui: &mut egui::Ui) {
        if self.demo {
            ui.colored_label(
                Color32::from_rgb(240, 190, 86),
                "DEMO LIBRARY · Generated sample metadata, not connected to a server",
            );
            ui.add_space(8.0);
        }
        if let Some(error) = self.error.clone() {
            Frame::new()
                .fill(Color32::from_rgb(48, 20, 23))
                .inner_margin(12)
                .corner_radius(4)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(Color32::from_rgb(255, 170, 176), &error);
                        if ui.small_button("Dismiss").clicked() {
                            self.error = None;
                        }
                    });
                });
            ui.add_space(10.0);
        }
        if let Some(message) = &self.message {
            ui.label(RichText::new(message).color(MUTED));
        }
        if self.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading your music…").color(MUTED));
            });
            ui.add_space(10.0);
        }
    }
    fn sidebar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if self.collapsed_sidebar {
            if icon_button(ui, Icon::Menu, "Expand sidebar", false, false).clicked() {
                self.collapsed_sidebar = false;
            }
        } else {
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [ui.available_width() - 78.0, 32.0],
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search")
                        .margin(vec2(9.0, 6.0)),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.navigate(Page::Search);
                    actions.push(Action::Search(self.search.clone()));
                }
                if icon_button(ui, Icon::Menu, "Collapse sidebar", false, false).clicked() {
                    self.collapsed_sidebar = true;
                }
                if icon_button(ui, Icon::Back, "Back", false, false).clicked() {
                    self.go_back(actions);
                }
            });
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                ui.label("My Library");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    icons::paint_icon(ui, Icon::Down, 14.0, TEXT);
                });
            });
            ui.add_space(4.0);
        }
        for (page, icon, label) in [
            (Page::Home, Icon::Home, "Home"),
            (Page::Tracks, Icon::Music, "Tracks"),
            (Page::Albums, Icon::Disc, "Albums"),
            (Page::Artists, Icon::Artist, "Artists"),
            (Page::Settings, Icon::Settings, "Settings"),
        ] {
            let active = self.page == page;
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, 4.0, CARD);
            }
            let color = if active { BLUE } else { TEXT };
            icons::draw(
                ui.painter(),
                icon,
                Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), vec2(14.0, 14.0)),
                color,
            );
            if !self.collapsed_sidebar {
                ui.painter().text(
                    pos2(rect.left() + 35.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(13.0),
                    color,
                );
            }
            if response.clicked() {
                self.navigate(page);
            }
            response.on_hover_text(label);
        }
        if !self.collapsed_sidebar {
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.label("Playlists");
                if ui
                    .small_button("+")
                    .on_hover_text("Create playlist")
                    .clicked()
                {
                    self.creating_playlist = !self.creating_playlist;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Refresh, "Refresh library", false, false).clicked() {
                        actions.push(Action::Refresh);
                    }
                });
            });
            if self.creating_playlist {
                ui.add(
                    egui::TextEdit::singleline(&mut self.playlist_name).hint_text("Playlist name"),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.connected && !self.playlist_name.trim().is_empty(),
                            egui::Button::new("Create"),
                        )
                        .clicked()
                    {
                        actions.push(Action::CreatePlaylist {
                            name: self.playlist_name.trim().to_owned(),
                        });
                        self.playlist_name.clear();
                        self.creating_playlist = false;
                    }
                    if ui.small_button("Cancel").clicked() {
                        self.creating_playlist = false;
                    }
                });
            }
            let available = (ui.available_height() - 100.0).max(60.0);
            egui::ScrollArea::vertical()
                .id_salt("sidebar-playlists")
                .max_height(available)
                .show(ui, |ui| {
                    let playlists = self.library.playlists.clone();
                    if playlists.is_empty() {
                        ui.label(RichText::new("No playlists yet").small().color(MUTED));
                    }
                    for playlist in playlists {
                        let (rect, response) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
                        if response.hovered() {
                            ui.painter().rect_filled(rect, 4.0, CARD);
                        }
                        self.paint_cover(
                            ui,
                            Rect::from_min_size(rect.min + vec2(0.0, 3.0), vec2(42.0, 42.0)),
                            &playlist.cover_id,
                            &playlist.name,
                        );
                        clipped_text(
                            ui,
                            Rect::from_min_size(
                                rect.min + vec2(52.0, 4.0),
                                vec2(rect.width() - 55.0, 22.0),
                            ),
                            &playlist.name,
                            13.0,
                            TEXT,
                        );
                        clipped_text(
                            ui,
                            Rect::from_min_size(
                                rect.min + vec2(52.0, 26.0),
                                vec2(rect.width() - 55.0, 18.0),
                            ),
                            &format!(
                                "{} tracks · {}",
                                playlist.song_count,
                                duration_text(playlist.duration)
                            ),
                            10.0,
                            MUTED,
                        );
                        if response.clicked() {
                            self.selected_playlist = Some(playlist.clone());
                            self.delete_playlist_confirm = false;
                            self.detail_tracks.clear();
                            self.navigate(Page::Playlist);
                            actions.push(Action::OpenPlaylist(playlist.id));
                        }
                    }
                });
            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
                ui.painter().rect_filled(rect, 4.0, CARD);
                icons::draw(
                    ui.painter(),
                    Icon::Disc,
                    Rect::from_center_size(
                        pos2(rect.left() + 25.0, rect.center().y),
                        vec2(32.0, 32.0),
                    ),
                    BLUE,
                );
                let name = if self.connected || self.demo {
                    if self.profile.name.is_empty() {
                        self.profile.kind.label()
                    } else {
                        &self.profile.name
                    }
                } else {
                    "Add a music server"
                };
                clipped_text(
                    ui,
                    Rect::from_min_size(
                        rect.min + vec2(50.0, 9.0),
                        vec2(rect.width() - 55.0, 20.0),
                    ),
                    name,
                    12.0,
                    TEXT,
                );
                clipped_text(
                    ui,
                    Rect::from_min_size(
                        rect.min + vec2(50.0, 29.0),
                        vec2(rect.width() - 55.0, 18.0),
                    ),
                    if self.connected {
                        "Music Library"
                    } else {
                        "Settings and connection"
                    },
                    10.0,
                    MUTED,
                );
                if response.clicked() {
                    self.navigate(Page::Settings);
                }
            });
        }
    }
    fn home(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if !self.connected && !self.demo && self.library.albums.is_empty() {
            ui.add_space(50.0);
            heading(ui, "Your music, right here", 30.0);
            ui.add_space(8.0);
            ui.label(RichText::new("Connect your music server to start listening.").color(MUTED));
            ui.add_space(20.0);
            if primary(ui, "Add music server").clicked() {
                self.navigate(Page::Settings);
            }
            ui.add_space(40.0);
            self.settings_form(ui, actions);
            return;
        }
        let albums = self
            .library
            .albums
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>();
        self.featured(ui, &albums, actions);
        ui.add_space(26.0);
        heading(ui, "Genres", 23.0);
        ui.add_space(8.0);
        let genres = self.library.genres.clone();
        if genres.is_empty() {
            ui.label(RichText::new("Genre information is unavailable.").color(MUTED));
        }
        ui.horizontal_wrapped(|ui| {
            for (i, genre) in genres.iter().enumerate() {
                let w = ((ui.available_width() + 8.0) / 4.0).max(120.0);
                let (r, response) =
                    ui.allocate_exact_size(vec2(w.min(190.0), 38.0), Sense::click());
                ui.painter().rect_filled(r, 3.0, CARD);
                ui.painter().rect_filled(
                    Rect::from_min_size(r.min, vec2(5.0, r.height())),
                    2.0,
                    palette(i),
                );
                ui.painter().text(
                    r.left_center() + vec2(16.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    genre,
                    FontId::proportional(12.0),
                    TEXT,
                );
                if response.clicked() {
                    self.navigate(Page::Albums);
                    self.genre = Some(genre.clone());
                }
            }
        });
        ui.add_space(28.0);
        ui.horizontal(|ui| {
            heading(ui, "Albums", 23.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("View more").clicked() {
                    self.navigate(Page::Albums);
                }
            });
        });
        ui.add_space(10.0);
        let albums = self
            .library
            .albums
            .iter()
            .take(18)
            .cloned()
            .collect::<Vec<_>>();
        self.album_grid(ui, &albums, actions);
    }
    fn featured(&mut self, ui: &mut egui::Ui, albums: &[Album], actions: &mut Vec<Action>) {
        if albums.is_empty() {
            empty(
                ui,
                "No albums found",
                "Refresh your library or check the server connection.",
            );
            return;
        }
        let cols = albums
            .len()
            .min(((ui.available_width() + 10.0) / 185.0).floor().max(1.0) as usize);
        let width = (ui.available_width() - 10.0 * (cols.saturating_sub(1) as f32)) / cols as f32;
        ui.horizontal_top(|ui| {
            for (i, album) in albums.iter().take(cols).enumerate() {
                let height = width * 1.20 + 50.0;
                let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
                let tint = self
                    .artwork_tints
                    .get(&album.cover_id)
                    .copied()
                    .unwrap_or_else(|| palette(i));
                let top =
                    Color32::from_rgb(tint.r() / 2 + 25, tint.g() / 2 + 20, tint.b() / 2 + 20);
                gradient(
                    ui,
                    rect,
                    top,
                    Color32::from_rgb(top.r() / 3, top.g() / 3, top.b() / 3),
                );
                let title_rect =
                    Rect::from_min_size(rect.min + vec2(12.0, 14.0), vec2(width - 24.0, 40.0));
                centered_text(ui, title_rect, &album.title, 13.0, TEXT, true);
                let size = (width * 0.68).min(190.0);
                self.paint_cover(
                    ui,
                    Rect::from_center_size(
                        pos2(rect.center().x, rect.top() + height * 0.49),
                        vec2(size, size),
                    ),
                    &album.cover_id,
                    &album.title,
                );
                centered_text(
                    ui,
                    Rect::from_min_size(
                        pos2(rect.left() + 10.0, rect.bottom() - 58.0),
                        vec2(width - 20.0, 24.0),
                    ),
                    &album.artist,
                    12.0,
                    TEXT,
                    false,
                );
                let info = if album.year > 0 {
                    format!("{}   {}", album.genre, album.year)
                } else {
                    album.genre.clone()
                };
                centered_text(
                    ui,
                    Rect::from_min_size(
                        pos2(rect.left() + 10.0, rect.bottom() - 30.0),
                        vec2(width - 20.0, 20.0),
                    ),
                    &info,
                    9.0,
                    TEXT,
                    false,
                );
                if response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        Stroke::new(1.0, Color32::from_white_alpha(70)),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    self.open_album(album.clone(), actions);
                }
            }
        });
    }
    fn open_album(&mut self, album: Album, actions: &mut Vec<Action>) {
        let id = album.id.clone();
        self.selected_album = Some(album);
        self.detail_tracks.clear();
        self.navigate(Page::Album);
        actions.push(Action::OpenAlbum(id));
    }
    fn album_grid(&mut self, ui: &mut egui::Ui, albums: &[Album], actions: &mut Vec<Action>) {
        let cols = ((ui.available_width() + 12.0) / 165.0).floor().max(1.0) as usize;
        let width = ((ui.available_width() - 12.0 * (cols.saturating_sub(1) as f32)) / cols as f32)
            .max(80.0);
        for row in albums.chunks(cols) {
            ui.horizontal_top(|ui| {
                for album in row {
                    ui.allocate_ui_with_layout(
                        vec2(width, width + 48.0),
                        Layout::top_down(Align::LEFT),
                        |ui| {
                            let (rect, response) =
                                ui.allocate_exact_size(vec2(width, width), Sense::click());
                            self.paint_cover(ui, rect, &album.cover_id, &album.title);
                            if response.hovered() {
                                ui.painter().rect_filled(
                                    Rect::from_center_size(
                                        rect.right_bottom() - vec2(25.0, 25.0),
                                        vec2(34.0, 34.0),
                                    ),
                                    17.0,
                                    BLUE,
                                );
                                icons::draw(
                                    ui.painter(),
                                    Icon::Play,
                                    Rect::from_center_size(
                                        rect.right_bottom() - vec2(25.0, 25.0),
                                        vec2(16.0, 16.0),
                                    ),
                                    Color32::WHITE,
                                );
                            }
                            if response.clicked() {
                                self.open_album(album.clone(), actions);
                            }
                            response.context_menu(|ui| {
                                if ui.button("Open album").clicked() {
                                    self.open_album(album.clone(), actions);
                                    ui.close();
                                }
                            });
                            ui.add(
                                egui::Label::new(RichText::new(&album.title).size(12.0)).truncate(),
                            );
                            if ui
                                .add(
                                    egui::Label::new(
                                        RichText::new(&album.artist).size(11.0).color(MUTED),
                                    )
                                    .truncate()
                                    .sense(Sense::click()),
                                )
                                .clicked()
                                && !album.artist_id.is_empty()
                            {
                                self.navigate(Page::Artist);
                                actions.push(Action::OpenArtist(album.artist_id.clone()));
                            }
                        },
                    );
                }
            });
            ui.add_space(10.0);
        }
    }
    fn toolbar(&mut self, ui: &mut egui::Ui, title: &str, count: usize, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            heading(ui, title, 28.0);
            ui.label(RichText::new(count.to_string()).color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Refresh, "Refresh library", false, false).clicked() {
                    actions.push(Action::Refresh);
                }
            });
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width().min(450.0) - 145.0, 32.0],
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search this view")
                    .margin(vec2(10.0, 7.0)),
            );
            egui::ComboBox::from_id_salt("sort")
                .selected_text(match self.sort {
                    Sort::Title => "Title",
                    Sort::Artist => "Artist",
                    Sort::Year => "Year",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sort, Sort::Title, "Title");
                    ui.selectable_value(&mut self.sort, Sort::Artist, "Artist");
                    ui.selectable_value(&mut self.sort, Sort::Year, "Year");
                });
            if icon_button(
                ui,
                Icon::Heart,
                "Favorites only",
                self.favorites_only,
                false,
            )
            .clicked()
            {
                self.favorites_only = !self.favorites_only;
            }
            if let Some(genre) = self.genre.clone()
                && ui.button(format!("{genre} ×")).clicked()
            {
                self.genre = None;
            }
        });
        ui.add_space(18.0);
    }
    fn albums_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        self.toolbar(ui, "Albums", self.library.albums.len(), actions);
        let query = self.filter.to_lowercase();
        let mut albums = self
            .library
            .albums
            .iter()
            .filter(|a| {
                (a.title.to_lowercase().contains(&query)
                    || a.artist.to_lowercase().contains(&query))
                    && (!self.favorites_only || a.favorite)
                    && self
                        .genre
                        .as_ref()
                        .is_none_or(|g| a.genre.eq_ignore_ascii_case(g))
            })
            .cloned()
            .collect::<Vec<_>>();
        match self.sort {
            Sort::Title => albums.sort_by_key(|a| a.title.to_lowercase()),
            Sort::Artist => albums.sort_by_key(|a| a.artist.to_lowercase()),
            Sort::Year => albums.sort_by_key(|a| std::cmp::Reverse(a.year)),
        };
        if albums.is_empty() {
            empty(
                ui,
                "No matching albums",
                "Try another search or refresh your library.",
            );
        }
        self.album_grid(ui, &albums, actions);
    }
    fn tracks_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        self.toolbar(ui, "Tracks", self.library.tracks.len(), actions);
        let tracks = self.filtered_tracks(&self.library.tracks);
        self.track_table(ui, &tracks, actions, true);
    }
    fn filtered_tracks<'a>(&self, tracks: &'a [Track]) -> Vec<&'a Track> {
        let q = self.filter.to_lowercase();
        let mut out = tracks
            .iter()
            .filter(|t| {
                (t.title.to_lowercase().contains(&q)
                    || t.artist.to_lowercase().contains(&q)
                    || t.album.to_lowercase().contains(&q))
                    && (!self.favorites_only || t.favorite)
            })
            .collect::<Vec<_>>();
        match self.sort {
            Sort::Title => out.sort_by_cached_key(|t| t.title.to_lowercase()),
            Sort::Artist => out.sort_by_cached_key(|t| t.artist.to_lowercase()),
            Sort::Year => out.sort_by_cached_key(|t| std::cmp::Reverse(t.year)),
        };
        out
    }
    fn artists_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        self.toolbar(ui, "Artists", self.library.artists.len(), actions);
        let q = self.filter.to_lowercase();
        let artists = self
            .library
            .artists
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&q))
            .cloned()
            .collect::<Vec<_>>();
        let cols = ((ui.available_width() + 14.0) / 180.0).floor().max(1.0) as usize;
        let width = (ui.available_width() - 14.0 * (cols.saturating_sub(1) as f32)) / cols as f32;
        for row in artists.chunks(cols) {
            ui.horizontal_top(|ui| {
                for artist in row {
                    ui.allocate_ui_with_layout(
                        vec2(width, width + 50.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            let (r, response) =
                                ui.allocate_exact_size(vec2(width, width), Sense::click());
                            self.paint_cover(ui, r, &artist.cover_id, &artist.name);
                            if response.clicked() {
                                self.selected_artist = Some(artist.clone());
                                self.artist_albums.clear();
                                self.navigate(Page::Artist);
                                actions.push(Action::OpenArtist(artist.id.clone()));
                            }
                            ui.label(&artist.name);
                            ui.label(
                                RichText::new(format!("{} albums", artist.album_count))
                                    .small()
                                    .color(MUTED),
                            );
                        },
                    );
                }
            });
            ui.add_space(14.0);
        }
        if artists.is_empty() {
            empty(
                ui,
                "No artists found",
                "Your artists will appear here after connecting.",
            );
        }
    }
    fn album_detail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(album) = self.selected_album.clone() else {
            empty(
                ui,
                "Choose an album",
                "Browse your library to open an album.",
            );
            return;
        };
        self.hero(
            ui,
            Hero {
                kind: "ALBUM",
                title: &album.title,
                artist: &album.artist,
                cover: &album.cover_id,
                info: format!(
                    "{} tracks · {}{}",
                    album.song_count,
                    duration_text(album.duration),
                    if album.year > 0 {
                        format!(" · {}", album.year)
                    } else {
                        String::new()
                    }
                ),
            },
            actions,
        );
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 120.0, 32.0],
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search")
                    .margin(vec2(10.0, 7.0)),
            );
            if !album.genre.is_empty() {
                ui.label(RichText::new(&album.genre).small().color(MUTED));
            }
        });
        ui.add_space(14.0);
        let tracks = self
            .detail_tracks
            .iter()
            .filter(|t| t.title.to_lowercase().contains(&self.filter.to_lowercase()))
            .collect::<Vec<_>>();
        self.track_table(ui, &tracks, actions, false);
    }
    fn artist_detail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let artist = self.selected_artist.clone().unwrap_or_else(|| Artist {
            name: self
                .artist_albums
                .first()
                .map(|a| a.artist.clone())
                .unwrap_or_else(|| "Artist".into()),
            ..Default::default()
        });
        self.hero(
            ui,
            Hero {
                kind: "ALBUM ARTIST",
                title: &artist.name,
                artist: "",
                cover: &artist.cover_id,
                info: format!("{} albums", self.artist_albums.len()),
            },
            actions,
        );
        ui.add_space(24.0);
        heading(ui, "Discography", 23.0);
        ui.add_space(14.0);
        let albums = self.artist_albums.clone();
        self.album_grid(ui, &albums, actions);
    }
    fn playlist_detail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(p) = self.selected_playlist.clone() else {
            empty(
                ui,
                "Choose a playlist",
                "Your playlists appear in the sidebar.",
            );
            return;
        };
        self.hero(
            ui,
            Hero {
                kind: "PLAYLIST",
                title: &p.name,
                artist: "",
                cover: &p.cover_id,
                info: format!("{} tracks · {}", p.song_count, duration_text(p.duration)),
            },
            actions,
        );
        ui.add_space(12.0);
        if self.connected {
            if !self.delete_playlist_confirm {
                if ui.button("Delete playlist…").clicked() {
                    self.delete_playlist_confirm = true;
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label("Delete this playlist from the server?");
                    if ui.button("Delete").clicked() {
                        actions.push(Action::DeletePlaylist { id: p.id.clone() });
                        self.delete_playlist_confirm = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.delete_playlist_confirm = false;
                    }
                });
            }
        }
        ui.add_space(24.0);
        let tracks = self.detail_tracks.iter().collect::<Vec<_>>();
        self.track_table(ui, &tracks, actions, true);
    }
    fn hero(&mut self, ui: &mut egui::Ui, hero: Hero<'_>, actions: &mut Vec<Action>) {
        let Hero {
            kind,
            title,
            artist,
            cover,
            info,
        } = hero;
        let width = ui.available_width();
        let height = if width > 650.0 { 250.0 } else { 200.0 };
        let (rect, _) = ui.allocate_exact_size(vec2(width, height + 30.0), Sense::hover());
        gradient(
            ui,
            rect.expand2(vec2(24.0, 24.0)),
            self.artwork_tints
                .get(cover)
                .copied()
                .map(|color| color.gamma_multiply(0.40))
                .unwrap_or(Color32::from_rgb(36, 33, 34)),
            BG,
        );
        let size = height - 24.0;
        self.paint_cover(
            ui,
            Rect::from_min_size(rect.min + vec2(0.0, 12.0), vec2(size, size)),
            cover,
            title,
        );
        let r = Rect::from_min_max(rect.min + vec2(size + 20.0, 35.0), rect.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
            ui.add_space(18.0);
            ui.label(RichText::new(kind).size(12.0));
            heading(ui, title, if width > 850.0 { 40.0 } else { 29.0 });
            ui.label(info);
            if !artist.is_empty() {
                ui.label(RichText::new(artist).strong());
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if primary_icon(ui, Icon::Play, "Play").clicked() && !self.detail_tracks.is_empty()
                {
                    actions.push(Action::Play {
                        tracks: self.detail_tracks.clone(),
                        start: 0,
                    });
                }
                if primary_icon(ui, Icon::Next, "Next").clicked() {
                    actions.push(Action::QueueNext(self.detail_tracks.clone()));
                }
                if primary_icon(ui, Icon::Queue, "Last").clicked() {
                    actions.push(Action::QueueLast(self.detail_tracks.clone()));
                }
            });
        });
    }
    fn track_table(
        &self,
        ui: &mut egui::Ui,
        tracks: &[&Track],
        actions: &mut Vec<Action>,
        show_album: bool,
    ) {
        if tracks.is_empty() {
            if !self.loading {
                empty(
                    ui,
                    "No tracks to show",
                    "Try another search or choose a different album.",
                );
            }
            return;
        }
        let width = ui.available_width();
        let artist_x = width * 0.43;
        let album_x = width * 0.65;
        let duration_x = width - 90.0;
        let (rect, _) = ui.allocate_exact_size(vec2(width, 31.0), Sense::hover());
        let p = ui.painter();
        p.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, LINE),
        );
        for (x, text) in [(12.0, "#"), (50.0, "TITLE"), (duration_x, "TIME")] {
            p.text(
                rect.min + vec2(x, 11.0),
                egui::Align2::LEFT_CENTER,
                text,
                FontId::proportional(10.0),
                TEXT,
            );
        }
        if show_album && width > 700.0 {
            p.text(
                rect.min + vec2(artist_x, 11.0),
                egui::Align2::LEFT_CENTER,
                "ARTIST",
                FontId::proportional(10.0),
                TEXT,
            );
            p.text(
                rect.min + vec2(album_x, 11.0),
                egui::Align2::LEFT_CENTER,
                "ALBUM",
                FontId::proportional(10.0),
                TEXT,
            );
        }
        // The outer page already owns scrolling. Reserve off-screen rows instead of
        // introducing nested scrolling or constructing widgets for every library item.
        let stride = 38.0 + ui.spacing().item_spacing.y;
        let top = ui.cursor().top();
        let first =
            (((ui.clip_rect().top() - top) / stride).floor().max(0.0) as usize).min(tracks.len());
        let last = ((((ui.clip_rect().bottom() - top) / stride).ceil().max(0.0) as usize)
            .saturating_add(1))
        .min(tracks.len())
        .max(first);
        ui.add_space(first as f32 * stride);
        ui.skip_ahead_auto_ids(first);
        for (index, track) in tracks.iter().enumerate().skip(first).take(last - first) {
            let playing = self.queue.current_track().is_some_and(|t| t.id == track.id);
            let (rect, response) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, 3.0, CARD);
            }
            let color = if playing { BLUE } else { TEXT };
            let title_end = if show_album && width > 700.0 {
                artist_x - 12.0
            } else {
                duration_x - 14.0
            };
            ui.painter().text(
                rect.min + vec2(12.0, 19.0),
                egui::Align2::LEFT_CENTER,
                if track.number > 0 {
                    track.number.to_string()
                } else {
                    (index + 1).to_string()
                },
                FontId::proportional(12.0),
                MUTED,
            );
            clipped_text(
                ui,
                Rect::from_min_max(
                    rect.min + vec2(50.0, 0.0),
                    pos2(rect.left() + title_end, rect.bottom()),
                ),
                &track.title,
                13.0,
                color,
            );
            if show_album && width > 700.0 {
                clipped_text(
                    ui,
                    Rect::from_min_max(
                        rect.min + vec2(artist_x, 0.0),
                        pos2(rect.left() + album_x - 12.0, rect.bottom()),
                    ),
                    &track.artist,
                    12.0,
                    MUTED,
                );
                clipped_text(
                    ui,
                    Rect::from_min_max(
                        rect.min + vec2(album_x, 0.0),
                        pos2(rect.left() + duration_x - 12.0, rect.bottom()),
                    ),
                    &track.album,
                    12.0,
                    MUTED,
                );
            }
            ui.painter().text(
                rect.min + vec2(duration_x, 19.0),
                egui::Align2::LEFT_CENTER,
                duration_text(track.duration),
                FontId::proportional(12.0),
                MUTED,
            );
            let heart_rect = Rect::from_center_size(
                pos2(rect.right() - 18.0, rect.center().y),
                vec2(26.0, 26.0),
            );
            let heart = ui.interact(
                heart_rect,
                ui.scope_id().with(("favorite", index, &track.id)),
                Sense::click(),
            );
            icons::draw(
                ui.painter(),
                Icon::Heart,
                heart_rect.shrink(6.0),
                if track.favorite { BLUE } else { MUTED },
            );
            if heart.clicked() {
                actions.push(Action::Favorite {
                    track_id: track.id.clone(),
                    favorite: !track.favorite,
                });
            }
            if response.double_clicked() {
                actions.push(Action::Play {
                    tracks: tracks.iter().map(|track| (**track).clone()).collect(),
                    start: index,
                });
            }
            response
                .on_hover_text("Double-click to play; right-click for queue actions")
                .context_menu(|ui| {
                    if ui.button("Play").clicked() {
                        actions.push(Action::Play {
                            tracks: tracks.iter().map(|track| (**track).clone()).collect(),
                            start: index,
                        });
                        ui.close();
                    }
                    if ui.button("Play next").clicked() {
                        actions.push(Action::QueueNext(vec![(**track).clone()]));
                        ui.close();
                    }
                    if ui.button("Add to queue").clicked() {
                        actions.push(Action::QueueLast(vec![(**track).clone()]));
                        ui.close();
                    }
                    if self.connected {
                        ui.menu_button("Add to playlist", |ui| {
                            for playlist in &self.library.playlists {
                                if ui.button(&playlist.name).clicked() {
                                    actions.push(Action::AddToPlaylist {
                                        playlist_id: playlist.id.clone(),
                                        track_ids: vec![track.id.clone()],
                                    });
                                    ui.close();
                                }
                            }
                        });
                        if self.page == Page::Playlist
                            && let Some(playlist) = &self.selected_playlist
                            && ui.button("Remove from playlist").clicked()
                        {
                            actions.push(Action::RemovePlaylistTrack {
                                playlist_id: playlist.id.clone(),
                                index,
                            });
                            ui.close();
                        }
                    }
                    if !track.album_id.is_empty() && ui.button("Go to album").clicked() {
                        actions.push(Action::OpenAlbum(track.album_id.clone()));
                        ui.close();
                    }
                });
        }
        ui.add_space(tracks.len().saturating_sub(last) as f32 * stride);
    }
    fn search_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, &format!("Search: {}", self.search), 28.0);
        ui.add_space(18.0);
        let albums = self.library.albums.clone();
        if !albums.is_empty() {
            heading(ui, "Albums", 22.0);
            self.album_grid(ui, &albums, actions);
        }
        let tracks = self.library.tracks.iter().collect::<Vec<_>>();
        if !tracks.is_empty() {
            ui.add_space(16.0);
            heading(ui, "Tracks", 22.0);
            self.track_table(ui, &tracks, actions, true);
        }
        if albums.is_empty() && tracks.is_empty() && !self.loading {
            empty(
                ui,
                "No results",
                "Try a different artist, album, or track title.",
            );
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Settings", 28.0);
        ui.add_space(22.0);
        heading(ui, "Music server", 20.0);
        ui.add_space(10.0);
        self.settings_form(ui, actions);
        ui.add_space(28.0);
        heading(ui, "Playback", 20.0);
        ui.add_space(10.0);
        ui.label("MPV executable");
        ui.add_sized(
            [ui.available_width().min(540.0), 32.0],
            egui::TextEdit::singleline(&mut self.mpv_path).hint_text("mpv"),
        );
        ui.label(
            RichText::new(
                "MPV is the only playback engine. Install it separately or set its full path.",
            )
            .small()
            .color(MUTED),
        );
        if ui.button("Save playback settings").clicked() {
            actions.push(Action::SaveSettings {
                mpv_path: self.mpv_path.clone(),
            });
        }
        ui.add_space(28.0);
        heading(ui, "Keyboard shortcuts", 20.0);
        ui.label("Space    Play / pause");
        ui.label("Ctrl + Right arrow    Next track");
        ui.label("Ctrl + Left arrow    Previous track");
        ui.label("Escape    Close now playing / queue");
    }
    fn settings_form(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.set_max_width(ui.available_width());
        egui::Grid::new("connection-form")
            .num_columns(2)
            .spacing(vec2(18.0, 12.0))
            .show(ui, |ui| {
                ui.label("Server type");
                egui::ComboBox::from_id_salt("server-type")
                    .selected_text(self.profile.kind.label())
                    .show_ui(ui, |ui| {
                        for kind in ServerKind::ALL {
                            ui.selectable_value(&mut self.profile.kind, kind, kind.label());
                        }
                    });
                ui.end_row();
                ui.label("Name");
                ui.add_sized(
                    [330.0, 30.0],
                    egui::TextEdit::singleline(&mut self.profile.name)
                        .hint_text("My music library"),
                );
                ui.end_row();
                ui.label("Server URL");
                ui.add_sized(
                    [330.0, 30.0],
                    egui::TextEdit::singleline(&mut self.profile.url)
                        .hint_text("https://music.example.com"),
                );
                ui.end_row();
                ui.label("Username");
                ui.add_sized(
                    [330.0, 30.0],
                    egui::TextEdit::singleline(&mut self.profile.username),
                );
                ui.end_row();
                ui.label("Password");
                ui.add_sized(
                    [330.0, 30.0],
                    egui::TextEdit::singleline(&mut self.password).password(true),
                );
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.label(
            RichText::new("Password is kept in memory for this session and is never saved.")
                .small()
                .color(MUTED),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let enabled = !self.loading
                && !self.profile.url.trim().is_empty()
                && !self.profile.username.trim().is_empty();
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(
                        RichText::new(if self.connected {
                            "Reconnect"
                        } else {
                            "Connect"
                        })
                        .color(Color32::BLACK),
                    )
                    .fill(TEXT)
                    .corner_radius(16),
                )
                .clicked()
            {
                actions.push(Action::Connect {
                    profile: self.profile.clone(),
                    password: std::mem::take(&mut self.password),
                });
            }
            if self.connected && ui.button("Disconnect").clicked() {
                actions.push(Action::Disconnect);
            }
        });
    }
    fn playback_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let rect = ui.available_rect_before_wrap();
        let width = rect.width();
        let track = self.queue.current_track().cloned();
        let middle_width = (width * 0.46).clamp(350.0, 600.0);
        let left_width = ((width - middle_width - 20.0) / 2.0).max(130.0);
        let right_width = left_width;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(left_width, 65.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_min_size(vec2(left_width, 65.0));
                    ui.set_max_width(left_width);
                    if let Some(track) = &track {
                        let (r, response) =
                            ui.allocate_exact_size(vec2(58.0, 58.0), Sense::click());
                        self.paint_cover(ui, r, &track.cover_id, &track.title);
                        if response.clicked() {
                            self.fullscreen = !self.fullscreen;
                        }
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&track.title).size(12.0)).truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&track.artist).size(12.0).color(MUTED),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&track.album).size(11.0).color(MUTED),
                                )
                                .truncate(),
                            );
                        });
                    } else {
                        ui.vertical(|ui| {
                            ui.label("Nothing playing");
                            ui.label(
                                RichText::new("Choose something from your library")
                                    .small()
                                    .color(MUTED),
                            );
                        });
                    }
                },
            );
            ui.allocate_ui_with_layout(
                vec2(middle_width, 65.0),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.set_min_size(vec2(middle_width, 65.0));
                    ui.set_max_width(middle_width);
                    ui.horizontal(|ui| {
                        let controls_width = 7.0 * 30.0 + 6.0 * 7.0;
                        ui.add_space(((middle_width - controls_width) / 2.0).max(0.0));
                        ui.spacing_mut().item_spacing.x = 7.0;
                        if icon_button(ui, Icon::Stop, "Stop", false, false).clicked() {
                            actions.push(Action::Stop);
                        }
                        if icon_button(ui, Icon::Shuffle, "Shuffle", self.queue.shuffle, false)
                            .clicked()
                        {
                            actions.push(Action::ToggleShuffle);
                        }
                        if icon_button(ui, Icon::Previous, "Previous", false, false).clicked() {
                            actions.push(Action::Previous);
                        }
                        if icon_button(
                            ui,
                            if self.playback.paused {
                                Icon::Play
                            } else {
                                Icon::Pause
                            },
                            "Play / pause",
                            false,
                            true,
                        )
                        .clicked()
                        {
                            actions.push(Action::TogglePause);
                        }
                        if icon_button(ui, Icon::Next, "Next", false, false).clicked() {
                            actions.push(Action::Next);
                        }
                        if icon_button(
                            ui,
                            Icon::Repeat,
                            match self.queue.repeat {
                                Repeat::Off => "Repeat: off",
                                Repeat::All => "Repeat: all",
                                Repeat::One => "Repeat: one",
                            },
                            self.queue.repeat != Repeat::Off,
                            false,
                        )
                        .clicked()
                        {
                            actions.push(Action::CycleRepeat);
                        }
                        if icon_button(ui, Icon::Queue, "Queue", self.queue_open, false).clicked() {
                            self.queue_open = !self.queue_open;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(duration_text(
                                self.seek_preview.unwrap_or(self.playback.position),
                            ))
                            .size(10.0)
                            .color(MUTED),
                        );
                        let duration = self
                            .playback
                            .duration
                            .max(track.as_ref().map_or(0.0, |t| t.duration))
                            .max(0.01);
                        self.seek_slider(ui, duration, (middle_width - 90.0).max(80.0), actions);
                        ui.label(
                            RichText::new(duration_text(duration))
                                .size(10.0)
                                .color(MUTED),
                        );
                    });
                },
            );
            ui.allocate_ui_with_layout(
                vec2(right_width, 65.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.set_min_size(vec2(right_width, 65.0));
                    ui.set_max_width(right_width);
                    let mut volume = self.playback.volume.clamp(0.0, 100.0);
                    if thin_slider(ui, "volume", &mut volume, 100.0, 90.0).changed() {
                        actions.push(Action::Volume(volume));
                    }
                    if icon_button(ui, Icon::Volume, "Mute / unmute", false, false).clicked() {
                        actions.push(Action::Volume(if self.playback.volume > 0.0 {
                            0.0
                        } else {
                            75.0
                        }));
                    }
                    if let Some(track) = &track {
                        if icon_button(ui, Icon::Heart, "Favorite", track.favorite, false).clicked()
                        {
                            actions.push(Action::Favorite {
                                track_id: track.id.clone(),
                                favorite: !track.favorite,
                            });
                        }
                        if icon_button(ui, Icon::Lyrics, "Lyrics", self.lyrics_open, false)
                            .clicked()
                        {
                            self.lyrics_open = !self.lyrics_open;
                            self.fullscreen = true;
                            actions.push(Action::Lyrics(track.id.clone()));
                        }
                    }
                },
            );
        });
    }
    fn seek_slider(
        &mut self,
        ui: &mut egui::Ui,
        duration: f64,
        width: f32,
        actions: &mut Vec<Action>,
    ) -> egui::Response {
        let mut position = self
            .seek_preview
            .unwrap_or(self.playback.position)
            .clamp(0.0, duration);
        let response = thin_slider(ui, "seek", &mut position, duration, width);
        if response.is_pointer_button_down_on() || response.dragged() {
            self.seek_preview = Some(position);
        }
        if response.drag_stopped() || response.clicked() {
            actions.push(Action::Seek(position));
            self.seek_preview = None;
        }
        response
    }
    fn queue_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            heading(ui, "Queue", 20.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Close, "Close queue", false, false).clicked() {
                    self.queue_open = false;
                }
                if ui.small_button("Clear").clicked() {
                    actions.push(Action::ClearQueue);
                }
            });
        });
        ui.label(
            RichText::new(format!("{} tracks", self.queue.tracks.len()))
                .small()
                .color(MUTED),
        );
        ui.add_space(12.0);
        egui::ScrollArea::vertical()
            .id_salt("queue-scroll")
            .show(ui, |ui| {
                for (index, track) in self.queue.tracks.iter().enumerate() {
                    let (rect, row) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
                    if row.hovered() {
                        ui.painter().rect_filled(rect, 4.0, CARD);
                    }
                    let color = if Some(index) == self.queue.current {
                        BLUE
                    } else {
                        TEXT
                    };
                    let title_rect = Rect::from_min_size(
                        rect.min + vec2(8.0, 4.0),
                        vec2((rect.width() - 45.0).max(1.0), 22.0),
                    );
                    clipped_text(ui, title_rect, &track.title, 13.0, color);
                    clipped_text(
                        ui,
                        title_rect.translate(vec2(0.0, 23.0)),
                        &format!("{} · {}", track.artist, duration_text(track.duration)),
                        11.0,
                        MUTED,
                    );
                    let remove_rect = Rect::from_center_size(
                        pos2(rect.right() - 14.0, rect.top() + 18.0),
                        vec2(26.0, 26.0),
                    );
                    let remove = ui.interact(
                        remove_rect,
                        ui.scope_id().with(("remove-queued", index)),
                        Sense::click(),
                    );
                    icons::draw(
                        ui.painter(),
                        Icon::Close,
                        remove_rect.shrink(7.0),
                        if remove.hovered() { TEXT } else { MUTED },
                    );
                    if remove.clicked() {
                        actions.push(Action::RemoveQueue(index));
                    } else if row.clicked() {
                        actions.push(Action::SelectQueue(index));
                    }
                    remove.on_hover_text("Remove from queue");
                }
            });
    }
    fn now_playing(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let tint = self
            .queue
            .current_track()
            .and_then(|track| self.artwork_tints.get(&track.cover_id))
            .copied()
            .unwrap_or(Color32::from_rgb(70, 84, 112));
        gradient(
            ui,
            ui.available_rect_before_wrap().expand(24.0),
            tint.gamma_multiply(0.70),
            BG,
        );
        if icon_button(ui, Icon::Down, "Close now playing", false, false).clicked() {
            self.fullscreen = false;
        }
        if self.demo {
            ui.label(
                RichText::new("DEMO LIBRARY · sample metadata")
                    .small()
                    .color(Color32::from_rgb(240, 190, 86)),
            );
        }
        let Some(track) = self.queue.current_track().cloned() else {
            empty(ui, "Nothing playing", "Choose a track from your library.");
            return;
        };
        ui.add_space(22.0);
        let width = ui.available_width();
        let side = (width * 0.40).min(460.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(width * 0.48, ui.available_height()),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.set_min_width(width * 0.48);
                    ui.set_max_width(width * 0.48);
                    ui.add_space(18.0);
                    let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                    self.paint_cover(ui, r, &track.cover_id, &track.title);
                    ui.add_space(20.0);
                    heading(ui, &track.title, 28.0);
                    ui.label(&track.album);
                    ui.label(RichText::new(&track.artist).color(MUTED));
                },
            );
            Frame::new()
                .fill(Color32::from_black_alpha(100))
                .corner_radius(4)
                .inner_margin(20)
                .show(ui, |ui| {
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.set_width((width * 0.48 - 40.0).max(150.0));
                        ui.set_min_height((ui.available_height() - 25.0).max(200.0));
                        ui.horizontal(|ui| {
                            if ui.selectable_label(!self.lyrics_open, "UP NEXT").clicked() {
                                self.lyrics_open = false;
                            }
                            if ui.selectable_label(self.lyrics_open, "LYRICS").clicked() {
                                self.lyrics_open = true;
                                actions.push(Action::Lyrics(track.id.clone()));
                            }
                        });
                        ui.separator();
                        if self.lyrics_open {
                            egui::ScrollArea::vertical()
                                .id_salt("lyrics-scroll")
                                .show(ui, |ui| {
                                    ui.add_space(24.0);
                                    if self.lyrics.trim().is_empty() {
                                        ui.label(
                                            RichText::new("No lyrics available for this track.")
                                                .color(MUTED),
                                        );
                                    } else {
                                        for line in self.lyrics.lines() {
                                            ui.label(RichText::new(line).size(19.0));
                                            ui.add_space(10.0);
                                        }
                                    }
                                });
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("now-queue")
                                .show(ui, |ui| {
                                    for (index, t) in self.queue.tracks.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                Some(index) == self.queue.current,
                                                format!("{}   {}", index + 1, t.title),
                                            )
                                            .clicked()
                                        {
                                            actions.push(Action::SelectQueue(index));
                                        }
                                        ui.label(RichText::new(&t.artist).small().color(MUTED));
                                        ui.add_space(8.0);
                                    }
                                });
                        }
                    });
                });
        });
    }
    fn paint_cover(&self, ui: &egui::Ui, rect: Rect, id: &str, label: &str) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        if !id.is_empty()
            && let Ok(mut visible) = self.visible_artwork.try_borrow_mut()
        {
            visible.push(id.to_owned());
        }
        if let Some(texture) = self.artwork.get(id) {
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            let seed = label
                .bytes()
                .fold(0usize, |n, b| n.wrapping_mul(31).wrapping_add(b as usize));
            let color = palette(seed);
            gradient(
                ui,
                rect,
                Color32::from_rgb(color.r() / 3 + 15, color.g() / 3 + 15, color.b() / 3 + 15),
                CARD,
            );
            icons::draw(
                ui.painter(),
                Icon::Disc,
                Rect::from_center_size(
                    rect.center(),
                    vec2(rect.width() * 0.36, rect.height() * 0.36),
                ),
                Color32::from_white_alpha(70),
            );
        }
    }
}

/// A slim native timeline with a filled handle and pointer-driven interaction.
fn thin_slider(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut f64,
    maximum: f64,
    width: f32,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
    let mut response = ui.interact(rect, ui.scope_id().with(id), Sense::click_and_drag());
    let rail = Rect::from_center_size(rect.center(), vec2((width - 8.0).max(1.0), 4.0));
    if (response.is_pointer_button_down_on() || response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let next = (((pointer.x - rail.left()) / rail.width()).clamp(0.0, 1.0) as f64) * maximum;
        if next != *value {
            *value = next;
            response.mark_changed();
        }
    }
    let fraction = (*value / maximum.max(0.001)).clamp(0.0, 1.0) as f32;
    ui.painter()
        .rect_filled(rail, 2.0, Color32::from_rgb(64, 64, 64));
    let end = rail.left() + fraction * rail.width();
    ui.painter().rect_filled(
        Rect::from_min_max(rail.min, pos2(end, rail.bottom())),
        2.0,
        TEXT,
    );
    ui.painter().circle_filled(
        pos2(end, rail.center().y),
        if response.hovered() || response.dragged() {
            4.5
        } else {
            2.8
        },
        TEXT,
    );
    response
}

fn heading(ui: &mut egui::Ui, text: &str, size: f32) {
    ui.label(RichText::new(text).font(FontId::new(size, FontFamily::Name("bold".into()))));
}
fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::BLACK))
            .fill(TEXT)
            .corner_radius(16)
            .min_size(vec2(88.0, 34.0)),
    )
}
fn primary_icon(ui: &mut egui::Ui, icon: Icon, text: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(92.0, 34.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    ui.painter().rect_filled(
        rect,
        16.0,
        if response.hovered() {
            Color32::WHITE
        } else {
            TEXT
        },
    );
    icons::draw(
        ui.painter(),
        icon,
        Rect::from_center_size(pos2(rect.left() + 21.0, rect.center().y), vec2(12.0, 12.0)),
        Color32::BLACK,
    );
    ui.painter().text(
        pos2(rect.left() + 35.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        Color32::BLACK,
    );
    response
}

fn empty(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.add_space(24.0);
    heading(ui, title, 22.0);
    ui.label(RichText::new(description).color(MUTED));
    ui.add_space(24.0);
}
fn clipped_text(ui: &egui::Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            FontId::proportional(size),
            color,
        );
}
fn centered_text(ui: &egui::Ui, rect: Rect, text: &str, size: f32, color: Color32, bold: bool) {
    let font = if bold {
        FontId::new(size, FontFamily::Name("bold".into()))
    } else {
        FontId::proportional(size)
    };
    let galley = ui.painter().layout(text.into(), font, color, rect.width());
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(
            pos2(rect.center().x - galley.size().x / 2.0, rect.top()),
            galley,
            color,
        );
}
fn palette(i: usize) -> Color32 {
    const COLORS: [Color32; 8] = [
        Color32::from_rgb(112, 111, 113),
        Color32::from_rgb(153, 79, 42),
        Color32::from_rgb(158, 107, 39),
        Color32::from_rgb(146, 25, 173),
        Color32::from_rgb(88, 87, 121),
        Color32::from_rgb(33, 121, 114),
        Color32::from_rgb(62, 90, 142),
        Color32::from_rgb(145, 64, 89),
    ];
    COLORS.get(i % COLORS.len()).copied().unwrap_or(BLUE)
}
fn gradient(ui: &egui::Ui, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_up_next_places_tracks_in_a_vertical_column() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut view = View {
            demo: true,
            fullscreen: true,
            ..Default::default()
        };
        view.queue.replace(
            ["First", "Second", "Third"]
                .into_iter()
                .map(|title| Track {
                    title: title.into(),
                    artist: "Artist".into(),
                    ..Default::default()
                })
                .collect(),
            0,
        );
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                view.ui(ui);
            },
        );
        // This headless test has no renderer to consume texture uploads. Clear
        // them before assertions so a failing layout does not panic on Drop.
        output.textures_delta.clear();
        let positions: Vec<_> = ["1   First", "2   Second", "3   Third"]
            .iter()
            .map(|label| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == *label => Some(text.pos),
                        _ => None,
                    })
                    .expect("queue title must be painted")
            })
            .collect();
        for pair in positions.windows(2) {
            assert!((pair[0].x - pair[1].x).abs() < 2.0);
            assert!(pair[1].y > pair[0].y + 20.0);
        }
    }

    fn frame(view: &mut View, input: egui::RawInput) -> Vec<Action> {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut actions = vec![];
        let mut output = ctx.run_ui(input, |ui| actions.extend(view.ui(ui)));
        assert!(!output.shapes.is_empty());
        // Headless layout test deliberately has no renderer to apply texture uploads.
        output.textures_delta.clear();
        actions
    }

    #[test]
    fn all_empty_views_render_without_server_or_player() {
        for page in [
            Page::Home,
            Page::Tracks,
            Page::Albums,
            Page::Artists,
            Page::Settings,
            Page::Album,
            Page::Artist,
            Page::Playlist,
            Page::Search,
        ] {
            let mut view = View {
                page,
                ..Default::default()
            };
            let actions = frame(
                &mut view,
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                    ..Default::default()
                },
            );
            assert!(actions.is_empty());
        }
    }

    #[test]
    fn populated_detail_and_now_playing_views_render_at_supported_sizes() {
        for width in [900.0, 1440.0, 1920.0] {
            for page in [
                Page::Home,
                Page::Album,
                Page::Artist,
                Page::Playlist,
                Page::Tracks,
            ] {
                let track = Track {
                    id: "track".into(),
                    title: "Test track".into(),
                    artist: "Test artist".into(),
                    duration: 183.0,
                    ..Default::default()
                };
                let album = Album {
                    id: "album".into(),
                    title: "Test album".into(),
                    artist: "Test artist".into(),
                    song_count: 1,
                    duration: 183.0,
                    ..Default::default()
                };
                let mut view = View {
                    page,
                    connected: true,
                    selected_album: Some(album.clone()),
                    selected_artist: Some(Artist {
                        name: "Test artist".into(),
                        ..Default::default()
                    }),
                    selected_playlist: Some(Playlist {
                        name: "Test playlist".into(),
                        ..Default::default()
                    }),
                    detail_tracks: vec![track.clone()],
                    artist_albums: vec![album.clone()],
                    ..Default::default()
                };
                view.library.albums.push(album);
                view.library.tracks.push(track.clone());
                view.queue.replace(vec![track], 0);
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 800.0))),
                    ..Default::default()
                };
                assert!(frame(&mut view, input.clone()).is_empty());
                view.fullscreen = true;
                assert!(frame(&mut view, input.clone()).is_empty());
                view.lyrics_open = true;
                view.lyrics = "First line\nSecond line".into();
                assert!(frame(&mut view, input).is_empty());
            }
        }
    }

    #[test]
    fn navigation_resets_local_search_without_altering_library() {
        let mut view = View {
            page: Page::Album,
            filter: "old query".into(),
            genre: Some("Jazz".into()),
            ..Default::default()
        };
        view.library.albums.push(Album::default());
        let from = view.route();
        view.navigate(Page::Albums);
        view.finish_navigation(from, &[]);
        assert!(view.filter.is_empty());
        assert!(view.genre.is_none());
        assert_eq!(view.library.albums.len(), 1);
        assert_eq!(
            view.history.pop().map(|route| route.page),
            Some(Page::Album)
        );
    }

    #[test]
    fn back_restores_entity_identity_across_album_artist_album_routes() {
        let mut view = View {
            page: Page::Albums,
            ..Default::default()
        };
        let a = Album {
            id: "album-a".into(),
            ..Default::default()
        };
        let b = Album {
            id: "album-b".into(),
            ..Default::default()
        };
        let artist = Artist {
            id: "artist-x".into(),
            ..Default::default()
        };
        view.library.albums = vec![a.clone(), b.clone()];
        view.library.artists = vec![artist.clone()];
        let from = view.route();
        view.selected_album = Some(a);
        view.navigate(Page::Album);
        view.finish_navigation(from, &[Action::OpenAlbum("album-a".into())]);
        let from = view.route();
        view.selected_artist = Some(artist);
        view.navigate(Page::Artist);
        view.finish_navigation(from, &[Action::OpenArtist("artist-x".into())]);
        let from = view.route();
        view.selected_album = Some(b);
        view.navigate(Page::Album);
        view.finish_navigation(from, &[Action::OpenAlbum("album-b".into())]);
        let mut actions = vec![];
        view.go_back(&mut actions);
        assert_eq!(view.page, Page::Artist);
        assert!(matches!(actions.last(), Some(Action::OpenArtist(id)) if id == "artist-x"));
        actions.clear();
        view.go_back(&mut actions);
        assert_eq!(view.page, Page::Album);
        assert!(matches!(actions.last(), Some(Action::OpenAlbum(id)) if id == "album-a"));
        assert_eq!(
            view.selected_album.as_ref().map(|a| a.id.as_str()),
            Some("album-a")
        );
        assert!(view.detail_tracks.is_empty());
    }

    #[test]
    fn same_page_album_navigation_records_identity_and_history_is_bounded() {
        let mut view = View {
            page: Page::Album,
            selected_album: Some(Album {
                id: "a".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let from = view.route();
        view.selected_album = Some(Album {
            id: "b".into(),
            ..Default::default()
        });
        view.navigate(Page::Album);
        view.finish_navigation(from, &[Action::OpenAlbum("b".into())]);
        let mut actions = vec![];
        view.go_back(&mut actions);
        assert!(matches!(actions.last(), Some(Action::OpenAlbum(id)) if id == "a"));
        view.back_navigation = false;
        for i in 0..150 {
            let from = view.route();
            let id = i.to_string();
            view.selected_album = Some(Album {
                id: id.clone(),
                ..Default::default()
            });
            view.finish_navigation(from, &[Action::OpenAlbum(id)]);
        }
        assert_eq!(view.history.len(), 100);
    }

    #[test]
    fn preview_is_demo_only_and_selects_existing_metadata() {
        let mut view = View::default();
        assert!(!view.set_preview("album"));
        view.demo = true;
        view.library.albums.push(Album {
            id: "a".into(),
            ..Default::default()
        });
        view.library.tracks = vec![
            Track {
                id: "selected".into(),
                album_id: "a".into(),
                ..Default::default()
            },
            Track {
                id: "other".into(),
                album_id: "b".into(),
                ..Default::default()
            },
        ];
        assert!(view.set_preview("album"));
        assert_eq!(view.detail_tracks.len(), 1);
        assert_eq!(view.detail_tracks[0].id, "selected");
        assert!(!view.set_preview("unknown"));
        assert_eq!(view.page, Page::Album);
        assert!(view.set_preview("lyrics"));
        assert!(view.fullscreen && view.lyrics_open);
        assert!(!view.lyrics.is_empty());
    }

    #[test]
    fn artwork_prefetch_is_bounded_and_prioritizes_current_detail_and_visible() {
        let mut view = View {
            page: Page::Album,
            selected_album: Some(Album {
                cover_id: "detail".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        view.queue.replace(
            vec![Track {
                cover_id: "current".into(),
                ..Default::default()
            }],
            0,
        );
        view.visible_artwork
            .borrow_mut()
            .push("scrolled-into-view".into());
        view.library.albums = (0..1000)
            .map(|i| Album {
                cover_id: format!("cover-{i}"),
                ..Default::default()
            })
            .collect();
        let ids = view.needed_artwork();
        assert_eq!(&ids[..3], &["current", "detail", "scrolled-into-view"]);
        assert_eq!(ids.len(), 512);
        assert_eq!(ids, view.needed_artwork());
    }

    #[test]
    fn seeking_retains_drag_target_until_release_instead_of_resetting_to_mpv_position() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut view = View::default();
        let mut render = |events: Vec<egui::Event>| {
            let mut actions = vec![];
            let mut rect = Rect::NOTHING;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 100.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    rect = view.seek_slider(ui, 100.0, 300.0, &mut actions).rect;
                },
            );
            output.textures_delta.clear();
            (rect, actions)
        };
        let (rect, _) = render(vec![]);
        let point = pos2(
            rect.left() + 4.0 + (rect.width() - 8.0) * 0.75,
            rect.center().y,
        );
        render(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        render(vec![egui::Event::PointerMoved(point)]);
        let (_, actions) = render(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Seek(value) if (value - 75.0).abs() < 0.1))
        );
        assert!(view.seek_preview.is_none());
        assert_eq!(view.playback.position, 0.0);
    }

    #[test]
    fn space_routes_to_playback_without_needing_a_server() {
        let mut view = View::default();
        let actions = frame(
            &mut view,
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                events: vec![egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
        );
        assert!(actions.iter().any(|a| matches!(a, Action::TogglePause)));
    }
}
