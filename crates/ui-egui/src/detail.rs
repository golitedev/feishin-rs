use super::*;

impl View {
    pub(super) fn album_detail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
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
                    "{}{} tracks · {}",
                    if album.year > 0 {
                        format!("{} · ", album.year)
                    } else {
                        String::new()
                    },
                    album.song_count,
                    duration_text(album.duration)
                ),
            },
            actions,
        );
        ui.add_space(22.0);
        if !album.genre.is_empty() {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.spacing_mut().interact_size.y = 24.0;
                ui.spacing_mut().button_padding.y = 3.0;
                ui.label(
                    RichText::new("GENRE")
                        .size(12.0)
                        .color(self.colors.muted)
                        .strong(),
                );
                if ui
                    .add(
                        egui::Button::new(RichText::new(&album.genre).color(self.colors.muted))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, self.colors.line))
                            .min_size(vec2(80.0, 24.0)),
                    )
                    .clicked()
                {
                    self.navigate(Page::Albums);
                    self.genre = Some(album.genre.clone());
                }
            });
        }
        ui.add_space(12.0);
        self.detail_search(ui);
        ui.add_space(12.0);
        let tracks = self
            .detail_tracks
            .iter()
            .filter(|t| t.title.to_lowercase().contains(&self.filter.to_lowercase()))
            .collect::<Vec<_>>();
        self.track_table(ui, &tracks, actions, false);
        let related = self
            .library
            .albums
            .iter()
            .filter(|a| {
                a.id != album.id
                    && if album.artist_id.is_empty() {
                        a.artist == album.artist
                    } else {
                        a.artist_id == album.artist_id
                    }
            })
            .cloned()
            .collect::<Vec<_>>();
        if !related.is_empty() {
            ui.add_space(74.0);
            heading(ui, "More from this Artist", 24.0);
            ui.add_space(10.0);
            self.album_grid(ui, &related, actions);
        }
    }

    pub(super) fn artist_detail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
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
                info: format!(
                    "{} albums · {} tracks",
                    self.artist_albums.len(),
                    self.detail_tracks.len()
                ),
            },
            actions,
        );
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.artist_tracks_open, false, "VIEW DISCOGRAPHY");
            ui.selectable_value(&mut self.artist_tracks_open, true, "VIEW ALL TRACKS");
        });
        ui.add_space(28.0);
        self.detail_search(ui);
        ui.add_space(16.0);
        if self.artist_tracks_open {
            let tracks = self.filtered_tracks(&self.detail_tracks);
            self.track_table(ui, &tracks, actions, true);
        } else {
            ui.horizontal(|ui| {
                heading(ui, "Album", 24.0);
                ui.label(
                    RichText::new(self.artist_albums.len().to_string()).color(self.colors.muted),
                );
            });
            ui.add_space(16.0);
            let query = self.filter.to_lowercase();
            let albums = self
                .artist_albums
                .iter()
                .filter(|a| a.title.to_lowercase().contains(&query))
                .cloned()
                .collect::<Vec<_>>();
            self.album_grid(ui, &albums, actions);
        }
    }

    pub(super) fn playlist_detail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(playlist) = self.selected_playlist.clone() else {
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
                title: &playlist.name,
                artist: "",
                cover: &playlist.cover_id,
                info: format!(
                    "{} tracks · {}",
                    playlist.song_count,
                    duration_text(playlist.duration)
                ),
            },
            actions,
        );
        if self.delete_playlist_confirm {
            ui.horizontal_wrapped(|ui| {
                ui.label("Delete this playlist from the server?");
                if ui.button("Delete").clicked() {
                    actions.push(Action::DeletePlaylist {
                        id: playlist.id.clone(),
                    });
                    self.delete_playlist_confirm = false;
                }
                if ui.button("Cancel").clicked() {
                    self.delete_playlist_confirm = false;
                }
            });
        }
        ui.add_space(22.0);
        self.detail_search(ui);
        ui.add_space(12.0);
        // Playlist indices identify occurrences, including duplicate songs. Keep the
        // original ordering when emitting edits and removals from the context menu.
        let query = self.filter.to_lowercase();
        let tracks = self
            .detail_tracks
            .iter()
            .filter(|track| track.title.to_lowercase().contains(&query))
            .collect::<Vec<_>>();
        self.track_table(ui, &tracks, actions, true);
    }

    fn detail_search(&mut self, ui: &mut egui::Ui) {
        let response = ui.add_sized(
            [ui.available_width(), 36.0],
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Search")
                .frame(Frame::NONE.corner_radius(18).inner_margin(egui::Margin {
                    left: 34,
                    right: 12,
                    top: 9,
                    bottom: 9,
                })),
        );
        ui.painter().rect_stroke(
            response.rect,
            18,
            Stroke::new(1.0, self.colors.line),
            egui::StrokeKind::Inside,
        );
        icons::draw(
            ui.painter(),
            Icon::Search,
            Rect::from_center_size(
                response.rect.left_center() + vec2(18.0, 0.0),
                vec2(14.0, 14.0),
            ),
            self.colors.muted,
        );
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
        let compact = width < 620.0;
        let cover_size = if compact { 140.0 } else { 225.0 };
        let height = if compact { 260.0 } else { 300.0 };
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        if (kind == "ALBUM" && self.preferences.appearance.album_background)
            || (kind == "ALBUM ARTIST" && self.preferences.appearance.artist_background)
        {
            let background =
                Rect::from_min_max(rect.min - vec2(32.0, 32.0), rect.max + vec2(32.0, 80.0));
            let mut background_ui = ui.new_child(egui::UiBuilder::new().max_rect(background));
            background_ui.set_clip_rect(Rect::from_min_max(
                pos2(background.left(), 30.0),
                pos2(background.right(), ui.clip_rect().bottom()),
            ));
            gradient(
                &background_ui,
                background,
                self.artwork_tints
                    .get(cover)
                    .copied()
                    .unwrap_or(Color32::from_gray(85))
                    .gamma_multiply(0.4),
                self.colors.bg,
            );
        }
        let cover_rect =
            Rect::from_min_size(rect.min + vec2(0.0, 51.0), vec2(cover_size, cover_size));
        if kind == "ALBUM ARTIST" {
            self.paint_artist_cover(ui, cover_rect, cover, title);
        } else {
            self.paint_cover(ui, cover_rect, cover, title);
        }
        let text_left = cover_rect.right() + 12.0;
        let text_rect = Rect::from_min_max(
            pos2(
                text_left,
                rect.top()
                    + if compact {
                        56.0
                    } else if kind == "ALBUM ARTIST" {
                        110.0
                    } else {
                        80.0
                    },
            ),
            rect.max,
        );
        let mut text_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        text_ui.spacing_mut().item_spacing.y = 8.0;
        text_ui.label(
            RichText::new(kind)
                .size(14.0)
                .strong()
                .color(self.colors.muted),
        );
        text_ui.add(
            egui::Label::new(RichText::new(title).font(font_id(
                ui,
                if compact { 28.0 } else { 44.0 },
                true,
            )))
            .truncate(),
        );
        text_ui.label(RichText::new(info).size(14.0).color(self.colors.muted));
        if !artist.is_empty() {
            let response = text_ui.add(
                egui::Label::new(RichText::new(artist).size(14.0).color(self.colors.muted))
                    .sense(Sense::click()),
            );
            if response.clicked()
                && let Some(album) = &self.selected_album
                && !album.artist_id.is_empty()
            {
                actions.push(Action::OpenArtist(album.artist_id.clone()));
            }
            response.on_hover_text("Go to album artist");
        }
        let controls = Rect::from_min_max(
            pos2(
                if compact { rect.left() } else { text_left },
                rect.top() + if compact { 210.0 } else { 233.0 },
            ),
            rect.max,
        );
        let mut controls_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(controls)
                .layout(Layout::left_to_right(Align::Min)),
        );
        controls_ui.spacing_mut().item_spacing.x = 12.0;
        for (icon, label, mode) in [
            (Icon::Play, "Play", 0),
            (Icon::Queue, "Add to queue", 1),
            (Icon::Next, "Play next", 2),
        ] {
            if detail_button(&mut controls_ui, icon, label).clicked()
                && !self.detail_tracks.is_empty()
            {
                actions.push(match mode {
                    0 => Action::Play {
                        tracks: self.detail_tracks.clone(),
                        start: 0,
                    },
                    1 => Action::QueueLast(self.detail_tracks.clone()),
                    _ => Action::QueueNext(self.detail_tracks.clone()),
                });
            }
        }
        controls_ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            ui.add_space(12.0);
            let more = icon_button(ui, Icon::More, "More actions", false, false);
            egui::Popup::menu(&more).show(|ui| {
                if ui.button("Add to queue").clicked() {
                    actions.push(Action::QueueLast(self.detail_tracks.clone()));
                    ui.close();
                }
                if ui.button("Play next").clicked() {
                    actions.push(Action::QueueNext(self.detail_tracks.clone()));
                    ui.close();
                }
                if kind == "PLAYLIST" && self.connected {
                    if self.capabilities.playlist_metadata && ui.button("Edit playlist").clicked() {
                        self.playlist_metadata_draft = self
                            .playlist_details
                            .as_ref()
                            .map(|p| p.metadata.clone())
                            .unwrap_or_else(|| PlaylistMetadata {
                                name: title.into(),
                                ..Default::default()
                            });
                        self.editing_playlist = true;
                        ui.close();
                    }
                    if ui.button("Delete playlist…").clicked() {
                        self.delete_playlist_confirm = true;
                        ui.close();
                    }
                }
            });
            let entity = match kind {
                "ALBUM" => self
                    .selected_album
                    .as_ref()
                    .map(|a| (a.id.clone(), a.favorite)),
                "ALBUM ARTIST" => self
                    .selected_artist
                    .as_ref()
                    .map(|a| (a.id.clone(), a.favorite)),
                _ => None,
            };
            if let Some((id, favorite)) = entity {
                ui.add_space(16.0);
                if self.connected
                    && icon_button(
                        ui,
                        Icon::Heart,
                        if favorite {
                            "Remove from favorites"
                        } else {
                            "Add to favorites"
                        },
                        favorite,
                        false,
                    )
                    .clicked()
                {
                    actions.push(if kind == "ALBUM" {
                        Action::FavoriteAlbum {
                            id: id.clone(),
                            favorite: !favorite,
                        }
                    } else {
                        Action::FavoriteArtist {
                            id: id.clone(),
                            favorite: !favorite,
                        }
                    });
                }
                if self.connected
                    && self.capabilities.numeric_ratings
                    && self.preferences.appearance.show_ratings
                    && !compact
                {
                    ui.add_space(22.0);
                    let current = self.ratings.get(&id).copied().unwrap_or(0);
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for rating in (1..=5).rev() {
                        let (star, response) =
                            ui.allocate_exact_size(vec2(14.0, 28.0), Sense::click());
                        icons::draw(
                            ui.painter(),
                            Icon::Star,
                            Rect::from_center_size(star.center(), vec2(14.0, 14.0)),
                            if current >= rating {
                                self.colors.accent
                            } else {
                                self.colors.muted
                            },
                        );
                        if response.clicked() {
                            let rating = if current == rating { 0 } else { rating };
                            actions.push(if kind == "ALBUM" {
                                Action::RateAlbum {
                                    id: id.clone(),
                                    rating,
                                }
                            } else {
                                Action::RateArtist {
                                    id: id.clone(),
                                    rating,
                                }
                            });
                        }
                        response.on_hover_text(format!(
                            "Rate {rating}/5; click current rating to clear"
                        ));
                        ui.spacing_mut().item_spacing.x = 2.0;
                    }
                }
            }
        });
    }
}

impl View {
    pub(super) fn paint_artist_cover(&self, ui: &egui::Ui, rect: Rect, id: &str, label: &str) {
        self.paint_cover(ui, rect, id, label);
        if !self.artwork.contains_key(id) {
            ui.painter().rect_filled(rect, 5, Color32::from_gray(210));
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(rect.center(), Color32::from_gray(240));
            for i in 0..10 {
                let angle = i as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                let radius = rect.width() * if i % 2 == 0 { 0.34 } else { 0.15 };
                mesh.colored_vertex(
                    rect.center() + vec2(angle.cos(), angle.sin()) * radius,
                    Color32::from_gray(240),
                );
            }
            for i in 0..10 {
                mesh.add_triangle(0, i + 1, (i + 1) % 10 + 1);
            }
            ui.painter().add(egui::Shape::mesh(mesh));
        }
    }
}

fn detail_button(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(66.0, 36.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    ui.painter().rect_filled(
        rect,
        16,
        if response.hovered() {
            Color32::WHITE
        } else {
            Color32::from_gray(214)
        },
    );
    icons::draw(
        ui.painter(),
        icon,
        Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
        Color32::BLACK,
    );
    response.on_hover_text(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(
        ctx: &egui::Context,
        view: &mut View,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<Action>) {
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| actions = view.ui(ui),
        );
        output.textures_delta.clear();
        (output, actions)
    }

    fn positions(output: &egui::FullOutput, label: &str) -> Vec<egui::Pos2> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .collect()
    }

    fn click(
        ctx: &egui::Context,
        view: &mut View,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) -> (egui::FullOutput, Vec<Action>) {
        render(
            ctx,
            view,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        render(
            ctx,
            view,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
    }

    fn album() -> View {
        View {
            connected: true,
            page: Page::Album,
            selected_album: Some(Album {
                id: "album".into(),
                title: "Record".into(),
                artist: "Performer".into(),
                artist_id: "artist".into(),
                genre: "Jazz".into(),
                ..Default::default()
            }),
            detail_tracks: vec![Track {
                id: "song".into(),
                title: "Song".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn album_artist_and_genre_links_navigate_without_starting_playback() {
        for label in ["Performer", "Jazz"] {
            let ctx = egui::Context::default();
            install(&ctx);
            let mut view = album();
            render(&ctx, &mut view, vec![]);
            let (output, _) = render(&ctx, &mut view, vec![]);
            let pos = positions(&output, label)[0];
            let (_, actions) = click(&ctx, &mut view, pos, egui::PointerButton::Primary);
            assert!(!actions.iter().any(|a| matches!(a, Action::Play { .. })));
            if label == "Performer" {
                assert!(
                    actions
                        .iter()
                        .any(|a| matches!(a, Action::OpenArtist(id) if id == "artist"))
                );
            } else {
                assert_eq!(view.page, Page::Albums);
                assert_eq!(view.genre.as_deref(), Some("Jazz"));
            }
        }
    }

    #[test]
    fn artist_tracks_tab_displays_loaded_tracks_and_navigation_resets_it() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut view = album();
        view.page = Page::Artist;
        view.selected_artist = Some(Artist {
            name: "Performer".into(),
            ..Default::default()
        });
        render(&ctx, &mut view, vec![]);
        let (output, _) = render(&ctx, &mut view, vec![]);
        let pos = positions(&output, "VIEW ALL TRACKS")[0];
        click(&ctx, &mut view, pos, egui::PointerButton::Primary);
        let (output, _) = render(&ctx, &mut view, vec![]);
        assert!(view.artist_tracks_open);
        assert!(!positions(&output, "Song").is_empty());
        view.navigate(Page::Home);
        assert!(!view.artist_tracks_open);
    }

    #[test]
    fn filtered_playlist_removal_targets_the_original_duplicate_occurrence() {
        let ctx = egui::Context::default();
        install(&ctx);
        let duplicate = Track {
            id: "duplicate".into(),
            title: "Match".into(),
            ..Default::default()
        };
        let mut view = View {
            connected: true,
            page: Page::Playlist,
            selected_playlist: Some(Playlist {
                id: "playlist".into(),
                name: "Collection".into(),
                ..Default::default()
            }),
            filter: "Match".into(),
            detail_tracks: vec![
                Track {
                    id: "hidden".into(),
                    title: "Hidden".into(),
                    ..Default::default()
                },
                duplicate.clone(),
                duplicate,
            ],
            ..Default::default()
        };
        render(&ctx, &mut view, vec![]);
        let (output, _) = render(&ctx, &mut view, vec![]);
        let rows = positions(&output, "Match");
        // The search field also contains Match; use the lowest visible track row.
        let pos = *rows
            .iter()
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .expect("filtered track visible");
        click(&ctx, &mut view, pos, egui::PointerButton::Secondary);
        let (output, _) = render(&ctx, &mut view, vec![]);
        let pos = positions(&output, "Remove from playlist")[0];
        let (_, actions) = click(&ctx, &mut view, pos, egui::PointerButton::Primary);
        assert!(actions.iter().any(|a| matches!(a, Action::RemovePlaylistTrack { playlist_id, index: 2 } if playlist_id == "playlist")));
    }
}
