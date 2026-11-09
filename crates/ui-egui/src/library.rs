use super::*;

impl View {
    fn library_playback_tracks(&self) -> Vec<Track> {
        if self.page == Page::Genres {
            let query = self.filter.to_lowercase();
            return self
                .library
                .tracks
                .iter()
                .filter(|t| t.genre.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }
        if self.page == Page::Favorites {
            return match self.favorite_category {
                extras::FavoriteCategory::Tracks => self
                    .filtered_tracks(&self.library.tracks)
                    .into_iter()
                    .filter(|t| t.favorite)
                    .cloned()
                    .collect(),
                extras::FavoriteCategory::Albums => {
                    let albums = self.filtered_albums();
                    self.library
                        .tracks
                        .iter()
                        .filter(|t| albums.iter().any(|a| a.favorite && a.id == t.album_id))
                        .cloned()
                        .collect()
                }
                extras::FavoriteCategory::Artists => {
                    let artists = self.filtered_artists(&self.library.artists);
                    self.library
                        .tracks
                        .iter()
                        .filter(|t| artists.iter().any(|a| a.favorite && a.id == t.artist_id))
                        .cloned()
                        .collect()
                }
            };
        }
        if self.page == Page::Albums {
            let mut result = Vec::new();
            for album in self.filtered_albums() {
                let mut tracks = self
                    .library
                    .tracks
                    .iter()
                    .filter(|t| t.album_id == album.id)
                    .cloned()
                    .collect::<Vec<_>>();
                tracks.sort_by_key(|t| (t.disc, t.number));
                result.extend(tracks);
            }
            result
        } else if matches!(self.page, Page::Artists | Page::AlbumArtists) {
            let source = if self.page == Page::Artists {
                &self.library.artists
            } else {
                &self.album_artists
            };
            let artists = self.filtered_artists(source);
            self.library
                .tracks
                .iter()
                .filter(|t| {
                    artists.iter().any(|a| {
                        if a.id.is_empty() || t.artist_id.is_empty() {
                            a.name == t.artist
                        } else {
                            a.id == t.artist_id
                        }
                    })
                })
                .cloned()
                .collect()
        } else {
            self.filtered_tracks(&self.library.tracks)
                .into_iter()
                .cloned()
                .collect()
        }
    }
    pub(super) fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        count: usize,
        actions: &mut Vec<Action>,
    ) {
        ui.horizontal(|ui| {
            let (rect, response) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
            ui.painter()
                .circle_filled(rect.center(), 24.0, self.colors.accent);
            icons::draw(ui.painter(), Icon::Play, rect.shrink(16.0), Color32::WHITE);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, count > 0, "Play this view")
            });
            if response.clicked() {
                let tracks = self.library_playback_tracks();
                if !tracks.is_empty() {
                    actions.push(Action::Play { tracks, start: 0 });
                }
            }
            if self.page == Page::Favorites {
                ui.vertical(|ui| {
                    heading(ui, title, 24.0);
                    egui::ComboBox::from_id_salt("favorite-category")
                        .selected_text(match self.favorite_category {
                            extras::FavoriteCategory::Tracks => "Tracks",
                            extras::FavoriteCategory::Albums => "Albums",
                            extras::FavoriteCategory::Artists => "Artists",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.favorite_category,
                                extras::FavoriteCategory::Tracks,
                                "Tracks",
                            );
                            ui.selectable_value(
                                &mut self.favorite_category,
                                extras::FavoriteCategory::Albums,
                                "Albums",
                            );
                            ui.selectable_value(
                                &mut self.favorite_category,
                                extras::FavoriteCategory::Artists,
                                "Artists",
                            );
                        });
                });
            } else {
                heading(ui, title, 36.0);
            }
            if self.page != Page::Favorites {
                ui.label(
                    RichText::new(count.to_string())
                        .size(12.0)
                        .color(self.colors.muted),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let opened = !self.library_search_open
                    && self.filter.is_empty()
                    && icon_button(ui, Icon::Search, "Search this view", false, false).clicked();
                if opened {
                    self.library_search_open = true;
                }
                if self.library_search_open || !self.filter.is_empty() {
                    let width = (ui.available_width() - 20.0).clamp(60.0, 240.0);
                    let response = ui.add_sized(
                        [width, 32.0],
                        egui::TextEdit::singleline(&mut self.filter).hint_text("Search"),
                    );
                    if opened {
                        response.request_focus();
                    }
                }
            });
        });
        ui.add_space(0.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("sort")
                .selected_text(match self.sort {
                    Sort::Title => "Name",
                    Sort::Artist => "Artist",
                    Sort::Year => "Release date",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sort, Sort::Title, "Name");
                    ui.selectable_value(&mut self.sort, Sort::Artist, "Artist");
                    ui.selectable_value(&mut self.sort, Sort::Year, "Release date");
                });
            if icon_button(
                ui,
                Icon::Queue,
                "Reverse sort order",
                self.descending,
                false,
            )
            .clicked()
            {
                self.descending = !self.descending;
            }
            if self.page != Page::Genres {
                let response = icon_button(
                    ui,
                    Icon::Sliders,
                    "Show filters",
                    self.library_filters_open,
                    false,
                );
                if self.page == Page::Favorites {
                    egui::Popup::menu(&response).show(|ui| {
                        ui.set_min_width(275.0);
                        self.library_filters(ui);
                    });
                } else if response.clicked() {
                    self.library_filters_open = !self.library_filters_open;
                }
            }
            if icon_button(ui, Icon::Refresh, "Refresh library", false, false).clicked() {
                actions.push(Action::Refresh);
            }
            if matches!(self.page, Page::Albums | Page::Artists | Page::AlbumArtists) {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button("View", |ui| {
                        ui.add(
                            egui::Slider::new(
                                &mut self.preferences.appearance.grid_size,
                                96.0..=360.0,
                            )
                            .text("Cover size"),
                        );
                    });
                });
            }
        });
        ui.add_space(0.0);
        ui.separator();
        ui.add_space(0.0);
    }

    pub(super) fn library_body(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        content: impl FnOnce(&mut Self, &mut egui::Ui, &mut Vec<Action>),
    ) {
        if matches!(self.page, Page::Albums | Page::Artists | Page::AlbumArtists) {
            ui.add_space(5.0);
        }
        if self.library_filters_open && ui.available_width() >= 700.0 {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    vec2(270.0, ui.available_height()),
                    Layout::top_down(Align::Min),
                    |ui| {
                        self.library_filters(ui);
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), ui.available_height()),
                    Layout::top_down(Align::Min),
                    |ui| content(self, ui, actions),
                );
            });
        } else {
            if self.library_filters_open {
                egui::CollapsingHeader::new("Filters").show(ui, |ui| self.library_filters(ui));
            }
            content(self, ui, actions);
        }
    }

    fn library_filters(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            heading(ui, "Filters", 18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Reset").clicked() {
                    self.favorites_only = false;
                    self.exclude_favorites = false;
                    self.min_rating = 0;
                    self.genre = None;
                    self.artist_filter = None;
                }
            });
        });
        ui.add_space(16.0);
        ui.label("Is favorited");
        let segment_width = ui.available_width() / 3.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (label, yes, no) in [
                ("None", false, false),
                ("Yes", true, false),
                ("No", false, true),
            ] {
                let selected = self.favorites_only == yes && self.exclude_favorites == no;
                if ui
                    .add_sized(
                        [segment_width, 24.0],
                        egui::Button::new(label)
                            .fill(if selected {
                                self.colors.line
                            } else {
                                self.colors.surface
                            })
                            .corner_radius(0),
                    )
                    .clicked()
                {
                    self.favorites_only = yes;
                    self.exclude_favorites = no;
                }
            }
        });
        if self.capabilities.numeric_ratings {
            ui.add_space(12.0);
            ui.label("Minimum rating");
            ui.add(egui::Slider::new(&mut self.min_rating, 0..=5));
        }
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(16.0);
        ui.label("Artists");
        egui::ScrollArea::vertical()
            .id_salt("filter-artists")
            .max_height(240.0)
            .show(ui, |ui| {
                if ui
                    .selectable_label(self.artist_filter.is_none(), "All artists")
                    .clicked()
                {
                    self.artist_filter = None;
                }
                for artist in &self.library.artists {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                    if response.hovered() || self.artist_filter.as_ref() == Some(&artist.name) {
                        ui.painter().rect_filled(rect, 3, self.colors.surface);
                    }
                    self.paint_artist_cover(
                        ui,
                        Rect::from_min_size(rect.min + vec2(8.0, 2.0), vec2(40.0, 40.0)),
                        &artist.cover_id,
                        &artist.name,
                    );
                    clipped_text(
                        ui,
                        Rect::from_min_max(
                            rect.min + vec2(56.0, 2.0),
                            rect.right_top() + vec2(0.0, 24.0),
                        ),
                        &artist.name,
                        12.0,
                        self.colors.text,
                    );
                    clipped_text(
                        ui,
                        Rect::from_min_max(rect.min + vec2(56.0, 24.0), rect.right_bottom()),
                        &format!("{} albums", artist.album_count),
                        11.0,
                        self.colors.muted,
                    );
                    if response.clicked() {
                        self.artist_filter = Some(artist.name.clone());
                    }
                }
            });
        ui.add_space(16.0);
        ui.label("Genres");
        egui::ComboBox::from_id_salt("filter-genre")
            .selected_text(self.genre.as_deref().unwrap_or("All genres"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.genre, None, "All genres");
                for genre in &self.library.genres {
                    ui.selectable_value(&mut self.genre, Some(genre.clone()), genre);
                }
            });
        ui.add_space(20.0);
        ui.menu_button("Save as collection", |ui| self.saved_filter_controls(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genre_search_playback_matches_genres_rather_than_track_titles() {
        let mut view = View {
            page: Page::Genres,
            filter: "Jazz".into(),
            ..Default::default()
        };
        view.library.tracks = vec![
            Track {
                id: "wanted".into(),
                title: "Blue".into(),
                genre: "Jazz".into(),
                ..Default::default()
            },
            Track {
                id: "other".into(),
                title: "Jazz".into(),
                genre: "Pop".into(),
                ..Default::default()
            },
        ];
        assert_eq!(
            view.library_playback_tracks()
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["wanted"]
        );
    }

    #[test]
    fn play_albums_uses_album_filters_and_disc_order_instead_of_track_favorites() {
        let mut view = View {
            page: Page::Albums,
            favorites_only: true,
            ..Default::default()
        };
        view.library.albums = vec![
            Album {
                id: "yes".into(),
                favorite: true,
                ..Default::default()
            },
            Album {
                id: "no".into(),
                ..Default::default()
            },
        ];
        view.library.tracks = vec![
            Track {
                id: "later-disc".into(),
                album_id: "yes".into(),
                disc: 2,
                number: 1,
                ..Default::default()
            },
            Track {
                id: "other-album".into(),
                album_id: "no".into(),
                favorite: true,
                ..Default::default()
            },
            Track {
                id: "first-disc".into(),
                album_id: "yes".into(),
                disc: 1,
                number: 2,
                ..Default::default()
            },
        ];
        assert_eq!(
            view.library_playback_tracks()
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["first-disc", "later-disc"]
        );
        view.favorites_only = false;
        view.exclude_favorites = true;
        assert_eq!(view.library_playback_tracks()[0].id, "other-album");
    }
}
