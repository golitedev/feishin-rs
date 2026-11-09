//! Native settings and server-capability views.
use super::{Action, Page, Sort, View, empty, heading, primary, shortcuts, theme};
use crate::icons::{self, Icon, icon_button};
use egui::{Align, Color32, Frame, Layout, Rect, RichText, Sense, Stroke, pos2, vec2};
use feishin_core::features::{RadioStation, SmartPlaylist, StreamFormat};
use feishin_core::playback::{AudioChannels, EQ_FREQUENCIES, ReplayGain};
use feishin_core::preferences::{Preferences, SavedFilter};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SettingsTab {
    #[default]
    General,
    Playback,
    Hotkeys,
    Window,
    Advanced,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FavoriteCategory {
    Tracks,
    Albums,
    Artists,
}

impl View {
    pub(super) fn main_menu(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if !self.profile.name.is_empty() {
            ui.label(RichText::new(&self.profile.name).strong());
            ui.label(
                RichText::new(&self.profile.username)
                    .small()
                    .color(self.colors.muted),
            );
            ui.separator();
        }
        if ui.button("Quick search                 Ctrl+K").clicked() {
            self.command_palette_open = true;
            self.command_filter.clear();
            ui.close();
        }
        if ui.button("Manage servers").clicked() {
            self.settings_tab = SettingsTab::Advanced;
            self.navigate(Page::Settings);
            ui.close();
        }
        let settings = ui
            .horizontal(|ui| {
                icons::paint_icon(ui, Icon::Settings, 14.0, self.colors.text);
                ui.button("Settings                     Ctrl+,")
            })
            .inner;
        if settings.clicked() {
            self.navigate(Page::Settings);
            ui.close();
        }
        if ui.button("Refresh library").clicked() {
            actions.push(Action::Refresh);
            ui.close();
        }
        if !self.profiles.is_empty() {
            ui.menu_button("Switch server", |ui| {
                for (index, profile) in self.profiles.iter().enumerate() {
                    if ui
                        .button(if profile.name.is_empty() {
                            profile.kind.label()
                        } else {
                            &profile.name
                        })
                        .clicked()
                    {
                        actions.push(Action::SelectServer(index));
                        ui.close();
                    }
                }
            });
        }
        if self.connected && ui.button("Disconnect").clicked() {
            actions.push(Action::Disconnect);
            ui.close();
        }
        ui.separator();
        if ui.button("Collapse sidebar").clicked() {
            self.preferences.appearance.sidebar_collapsed = true;
            ui.close();
        }
        if ui.button("Now playing                  F11").clicked() {
            self.fullscreen = true;
            ui.close();
        }
        if ui.button("Quit").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    pub(super) fn command_palette(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if !self.command_palette_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Quick search")
            .id(egui::Id::unique("command-palette"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 80.0))
            .default_width(600.0)
            .show(ctx, |ui| {
                let input = ui.add_sized(
                    [ui.available_width(), 36.0],
                    egui::TextEdit::singleline(&mut self.command_filter)
                        .hint_text("Search your library"),
                );
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.command_palette_open = false;
                }
                if input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.search = self.command_filter.clone();
                    self.navigate(Page::Search);
                    actions.push(Action::Search(self.search.clone()));
                    self.command_palette_open = false;
                }
                let query = self.command_filter.to_lowercase();
                for track in self
                    .library
                    .tracks
                    .iter()
                    .filter(|t| {
                        t.title.to_lowercase().contains(&query)
                            || t.artist.to_lowercase().contains(&query)
                    })
                    .take(12)
                {
                    if ui
                        .button(format!("{}   ·   {}", track.title, track.artist))
                        .clicked()
                    {
                        actions.push(Action::Play {
                            tracks: vec![track.clone()],
                            start: 0,
                        });
                        self.command_palette_open = false;
                    }
                }
            });
        self.command_palette_open &= open;
    }
    pub fn apply_custom_theme_json(&mut self, text: &str) -> Result<(), String> {
        let (base, colors) = theme::parse_custom(text)?;
        self.preferences.appearance.theme = base;
        self.preferences.appearance.custom_colors = colors;
        Ok(())
    }
    pub(super) fn settings_modal(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let screen = ui.ctx().viewport_rect();
        let size = vec2(
            (screen.width() - 24.0).min(960.0),
            (screen.height() - 24.0).min(810.0),
        );
        let ctx = ui.ctx().clone();
        let modal = egui::Modal::new(egui::Id::unique("settings-modal"))
            .area(egui::Modal::default_area(egui::Id::unique("settings-modal")).fade_in(false))
            .frame(
                Frame::new()
                    .fill(self.colors.bg)
                    .stroke(Stroke::new(1.0, self.colors.line))
                    .corner_radius(4)
                    .inner_margin(24),
            )
            .show(&ctx, |ui| {
                ui.set_min_size(size - vec2(48.0, 48.0));
                ui.set_max_size(size - vec2(48.0, 48.0));
                ui.horizontal(|ui| {
                    heading(ui, "Settings", 34.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, "Close settings", false, false).clicked() {
                            self.navigate(Page::Home);
                        }
                        if ui.button("Reset to default").clicked() {
                            self.preferences = Preferences::default();
                        }
                    });
                });
                ui.add_space(28.0);
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (SettingsTab::General, "General"),
                        (SettingsTab::Playback, "Playback"),
                        (SettingsTab::Hotkeys, "Hotkeys"),
                        (SettingsTab::Window, "Window"),
                        (SettingsTab::Advanced, "Advanced"),
                    ] {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(74.0, 36.0), Sense::click());
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            super::font_id(ui, 13.0, false),
                            self.colors.text,
                        );
                        if self.settings_tab == tab {
                            ui.painter().line_segment(
                                [rect.left_bottom(), rect.right_bottom()],
                                Stroke::new(2.0, self.colors.accent),
                            );
                        }
                        if response.clicked() {
                            self.settings_tab = tab;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("settings-scroll")
                    .max_height((size.y - 178.0).max(100.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        match self.settings_tab {
                            SettingsTab::General => self.general_settings(ui),
                            SettingsTab::Playback => self.playback_settings(ui, actions),
                            SettingsTab::Hotkeys => self.hotkey_settings(ui),
                            SettingsTab::Window => self.window_settings(ui),
                            SettingsTab::Advanced => self.advanced_settings(ui, actions),
                        }
                    });
            });
        if modal.should_close() {
            self.navigate(Page::Home);
        }
    }
    fn general_settings(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Theme", 20.0);
        setting(
            ui,
            "Use system theme",
            "Follow the system-defined light or dark preference",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.follow_system_theme);
            },
        );
        setting(
            ui,
            "Theme",
            "Sets the theme to use for the application",
            |ui| {
                egui::ComboBox::from_id_salt("theme")
                    .width(230.0)
                    .selected_text(
                        theme::THEMES
                            .iter()
                            .find(|(id, _)| *id == self.preferences.appearance.theme)
                            .map_or("Custom", |(_, label)| *label),
                    )
                    .show_ui(ui, |ui| {
                        for (id, label) in theme::THEMES {
                            if ui
                                .selectable_value(
                                    &mut self.preferences.appearance.theme,
                                    (*id).into(),
                                    *label,
                                )
                                .clicked()
                            {
                                self.preferences.appearance.custom_colors.clear();
                            }
                        }
                    });
            },
        );
        setting(
            ui,
            "Accent color",
            "Sets the accent color for the application",
            |ui| {
                ui.color_edit_button_srgb(&mut self.preferences.appearance.accent_rgb);
            },
        );
        egui::CollapsingHeader::new("Custom themes").show(ui,|ui|{
            ui.label("Paste a JSON theme with mode, extends, and colors. Native palettes do not execute linked CSS.");
            ui.add(egui::TextEdit::multiline(&mut self.custom_theme_json).code_editor().desired_rows(7).desired_width(f32::INFINITY));
            if ui.button("Apply custom theme").clicked(){let text=self.custom_theme_json.clone();if let Err(error)=self.apply_custom_theme_json(&text){self.error=Some(error);}}
        });
        ui.separator();
        heading(ui, "Application", 20.0);
        setting(ui, "Font", "Font used throughout the application", |ui| {
            egui::ComboBox::from_id_salt("font")
                .selected_text(&self.preferences.appearance.font_family)
                .show_ui(ui, |ui| {
                    for name in ["Inter", "Poppins"] {
                        ui.selectable_value(
                            &mut self.preferences.appearance.font_family,
                            name.into(),
                            name,
                        );
                    }
                });
        });
        setting(
            ui,
            "Font size",
            "Interface text size in logical pixels",
            |ui| {
                ui.add(
                    egui::Slider::new(&mut self.preferences.appearance.font_size, 10.0..=24.0)
                        .suffix(" px"),
                );
            },
        );
        setting(
            ui,
            "Home featured albums",
            "Show a featured album carousel on the home page",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.show_feature);
            },
        );
        setting(
            ui,
            "Home feature style",
            "Single banner or multiple album cards",
            |ui| {
                egui::ComboBox::from_id_salt("feature-style")
                    .selected_text(&self.preferences.appearance.home_feature_style)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.preferences.appearance.home_feature_style,
                            "single".into(),
                            "Single",
                        );
                        ui.selectable_value(
                            &mut self.preferences.appearance.home_feature_style,
                            "multiple".into(),
                            "Multiple",
                        );
                    });
            },
        );
        setting(ui, "Genres", "Show genre tiles on the home page", |ui| {
            toggle(ui, &mut self.preferences.appearance.show_genres);
        });
        setting(
            ui,
            "Album background",
            "Show artwork color behind album headers",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.album_background);
            },
        );
        setting(
            ui,
            "Artist background",
            "Show artwork color behind artist headers",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.artist_background);
            },
        );
        setting(
            ui,
            "Ratings",
            "Show numeric ratings when supported by the server",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.show_ratings);
            },
        );
        setting(
            ui,
            "Album grid size",
            "Sets the size of covers in grid views",
            |ui| {
                ui.add(
                    egui::Slider::new(&mut self.preferences.appearance.grid_size, 96.0..=360.0)
                        .suffix(" px"),
                );
            },
        );
        ui.separator();
        heading(ui, "Lyrics", 20.0);
        setting(
            ui,
            "Fetch lyrics",
            "Retrieve lyrics for the current song",
            |ui| {
                toggle(ui, &mut self.preferences.lyrics.enabled);
            },
        );
        setting(
            ui,
            "Follow current line",
            "Keep the active synchronized lyric line in view",
            |ui| {
                toggle(ui, &mut self.preferences.lyrics.follow);
            },
        );
        setting(
            ui,
            "Timing offset",
            "Positive values display lyrics later",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.preferences.lyrics.delay_ms)
                        .range(-30000..=30000)
                        .suffix(" ms"),
                );
            },
        );
        setting(
            ui,
            "Line lead time",
            "Highlight upcoming lines this much earlier",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.preferences.lyrics.lead_ms)
                        .range(0..=10000)
                        .suffix(" ms"),
                );
            },
        );
        setting(
            ui,
            "Lyric text size",
            "Text size in the now-playing lyric panel",
            |ui| {
                ui.add(
                    egui::Slider::new(&mut self.preferences.lyrics.font_size, 14.0..=48.0)
                        .suffix(" px"),
                );
            },
        );
        setting(
            ui,
            "Lyric alignment",
            "Align text within the lyric panel",
            |ui| {
                egui::ComboBox::from_id_salt("lyric-align")
                    .selected_text(&self.preferences.lyrics.alignment)
                    .show_ui(ui, |ui| {
                        for name in ["left", "center", "right"] {
                            ui.selectable_value(
                                &mut self.preferences.lyrics.alignment,
                                name.into(),
                                name,
                            );
                        }
                    });
            },
        );
        ui.separator();
        heading(ui, "Auto DJ", 20.0);
        setting(
            ui,
            "Enable Auto DJ",
            "Append server-selected songs when the queue runs low",
            |ui| {
                toggle(ui, &mut self.preferences.auto_dj.enabled);
            },
        );
        setting(ui, "Number of songs", "Number of tracks to append", |ui| {
            ui.add(egui::DragValue::new(&mut self.preferences.auto_dj.item_count).range(1..=100));
        });
        setting(
            ui,
            "Allow duplicates",
            "Allow songs already in the queue to be added again",
            |ui| {
                toggle(ui, &mut self.preferences.auto_dj.allow_duplicates);
            },
        );
        setting(
            ui,
            "Only similar songs",
            "Do not fill missing results with random songs",
            |ui| {
                toggle(ui, &mut self.preferences.auto_dj.similar_only);
            },
        );
    }
    fn playback_settings(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Player", 20.0);
        setting(
            ui,
            "Playback engine",
            "All playback uses an external MPV process",
            |ui| {
                ui.label("MPV");
            },
        );
        setting(
            ui,
            "MPV executable",
            "Path to the installed MPV executable",
            |ui| {
                ui.text_edit_singleline(&mut self.mpv_path);
            },
        );
        ui.label("MPV extra parameters (one option per line)");
        ui.add(
            egui::TextEdit::multiline(&mut self.preferences.mpv_arguments)
                .code_editor()
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("--audio-buffer=0.2\n--cache=yes"),
        );
        ui.label(
            RichText::new("Audio and cache options apply when MPV restarts.")
                .small()
                .color(self.colors.muted),
        );
        if ui.button("Apply and restart MPV").clicked() {
            actions.push(Action::SaveSettings {
                mpv_path: self.mpv_path.clone(),
            });
        }
        ui.separator();
        heading(ui, "Audio output", 20.0);
        setting(ui, "Output device", "Select the device used by MPV", |ui| {
            let description = self
                .audio_devices
                .iter()
                .find(|d| d.name == self.preferences.playback.audio_device)
                .map_or(self.preferences.playback.audio_device.as_str(), |d| {
                    d.description.as_str()
                });
            egui::ComboBox::from_id_salt("audio-device")
                .width(230.0)
                .selected_text(description)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.preferences.playback.audio_device,
                        "auto".into(),
                        "Automatic",
                    );
                    for d in &self.audio_devices {
                        ui.selectable_value(
                            &mut self.preferences.playback.audio_device,
                            d.name.clone(),
                            &d.description,
                        );
                    }
                });
        });
        if ui.button("Refresh output devices").clicked() {
            actions.push(Action::RequestAudioDevices);
        }
        setting(
            ui,
            "Exclusive output",
            "Request exclusive access where the output driver supports it",
            |ui| {
                toggle(ui, &mut self.preferences.playback.exclusive);
            },
        );
        setting(
            ui,
            "Output channels",
            "Select the output channel layout",
            |ui| {
                egui::ComboBox::from_id_salt("audio-channels")
                    .selected_text(self.preferences.playback.channels.label())
                    .show_ui(ui, |ui| {
                        for value in AudioChannels::ALL {
                            ui.selectable_value(
                                &mut self.preferences.playback.channels,
                                value,
                                value.label(),
                            );
                        }
                    });
            },
        );
        setting(
            ui,
            "Output sample rate",
            "Automatic preserves the device's preferred rate",
            |ui| {
                egui::ComboBox::from_id_salt("sample-rate")
                    .selected_text(
                        self.preferences
                            .playback
                            .sample_rate
                            .map_or("Automatic".into(), |v| format!("{v} Hz")),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.preferences.playback.sample_rate,
                            None,
                            "Automatic",
                        );
                        for rate in [44100, 48000, 88200, 96000, 192000] {
                            ui.selectable_value(
                                &mut self.preferences.playback.sample_rate,
                                Some(rate),
                                format!("{rate} Hz"),
                            );
                        }
                    });
            },
        );
        setting(ui, "Maximum volume", "Maximum player volume", |ui| {
            ui.add(
                egui::Slider::new(&mut self.preferences.playback.volume_max, 100.0..=200.0)
                    .suffix("%"),
            );
        });
        setting(ui, "Playback speed", "Change the playback rate", |ui| {
            ui.add(egui::Slider::new(&mut self.preferences.playback.speed, 0.5..=2.0).suffix("×"));
        });
        setting(
            ui,
            "Preserve pitch",
            "Keep the original pitch when changing playback speed",
            |ui| {
                toggle(ui, &mut self.preferences.playback.preserve_pitch);
            },
        );
        setting(
            ui,
            "Gapless playback",
            "Weak preserves the output device for compatible consecutive tracks",
            |ui| {
                let mut choice = if !self.preferences.playback.gapless {
                    0
                } else if self.preferences.playback.gapless_weak {
                    1
                } else {
                    2
                };
                egui::ComboBox::from_id_salt("gapless-mode")
                    .selected_text(match choice {
                        0 => "No",
                        1 => "Weak",
                        _ => "Yes",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut choice, 0, "No");
                        ui.selectable_value(&mut choice, 1, "Weak");
                        ui.selectable_value(&mut choice, 2, "Yes");
                    });
                self.preferences.playback.gapless = choice != 0;
                self.preferences.playback.gapless_weak = choice == 1;
            },
        );
        setting(
            ui,
            "Audio fade on status change",
            "Fade audio when pausing and resuming playback",
            |ui| {
                toggle(ui, &mut self.preferences.playback.fade_on_pause);
            },
        );
        setting(
            ui,
            "Dynamic normalization",
            "Normalize perceived loudness through MPV",
            |ui| {
                toggle(ui, &mut self.preferences.playback.normalization);
            },
        );
        ui.separator();
        heading(ui, "ReplayGain", 20.0);
        setting(
            ui,
            "ReplayGain mode",
            "Use track or album gain metadata",
            |ui| {
                egui::ComboBox::from_id_salt("replaygain")
                    .selected_text(self.preferences.playback.replay_gain.label())
                    .show_ui(ui, |ui| {
                        for mode in ReplayGain::ALL {
                            ui.selectable_value(
                                &mut self.preferences.playback.replay_gain,
                                mode,
                                mode.label(),
                            );
                        }
                    });
            },
        );
        setting(
            ui,
            "ReplayGain preamp",
            "Gain applied before ReplayGain",
            |ui| {
                ui.add(
                    egui::Slider::new(
                        &mut self.preferences.playback.replay_gain_preamp,
                        -15.0..=15.0,
                    )
                    .suffix(" dB"),
                );
            },
        );
        setting(
            ui,
            "Fallback gain",
            "Gain used when ReplayGain tags are absent",
            |ui| {
                ui.add(
                    egui::Slider::new(
                        &mut self.preferences.playback.replay_gain_fallback,
                        -15.0..=15.0,
                    )
                    .suffix(" dB"),
                );
            },
        );
        setting(
            ui,
            "Allow clipping",
            "Disable automatic gain reduction for clipping protection",
            |ui| {
                toggle(ui, &mut self.preferences.playback.replay_gain_clip);
            },
        );
        ui.separator();
        heading(ui, "Equalizer", 20.0);
        ui.checkbox(
            &mut self.preferences.playback.equalizer.enabled,
            "Enable 10-band equalizer",
        );
        ui.horizontal_wrapped(|ui| {
            for (index, freq) in EQ_FREQUENCIES.iter().enumerate() {
                ui.vertical(|ui| {
                    if let Some(gain) = self.preferences.playback.equalizer.bands_db.get_mut(index)
                    {
                        ui.add(
                            egui::Slider::new(gain, -12.0..=12.0)
                                .vertical()
                                .suffix(" dB"),
                        );
                    }
                    ui.label(if *freq >= 1000 {
                        format!("{}k", *freq / 1000)
                    } else {
                        freq.to_string()
                    });
                });
            }
        });
        setting(ui, "Preamp", "Gain applied before equalization", |ui| {
            ui.add(
                egui::Slider::new(&mut self.preferences.playback.preamp_db, -24.0..=12.0)
                    .suffix(" dB"),
            );
        });
        if ui.button("Reset equalizer").clicked() {
            self.preferences.playback.equalizer.bands_db = [0.0; 10];
            self.preferences.playback.preamp_db = 0.0;
        }
        if primary(ui, "Apply audio settings").clicked() {
            actions.push(Action::ApplyPlaybackSettings(
                self.preferences.playback.clone(),
            ));
        }
        if self.capabilities.transcoding {
            ui.separator();
            heading(ui, "Server transcoding", 20.0);
            setting(ui, "Format", "Original leaves decoding to MPV", |ui| {
                egui::ComboBox::from_id_salt("stream-format")
                    .selected_text(self.preferences.stream.format.label())
                    .show_ui(ui, |ui| {
                        for format in StreamFormat::ALL {
                            ui.selectable_value(
                                &mut self.preferences.stream.format,
                                format,
                                format.label(),
                            );
                        }
                    });
            });
            setting(
                ui,
                "Maximum bitrate",
                "Used by the server when transcoding",
                |ui| {
                    egui::ComboBox::from_id_salt("stream-bitrate")
                        .selected_text(
                            self.preferences
                                .stream
                                .max_bitrate_kbps
                                .map_or("Unlimited".into(), |v| format!("{v} kbps")),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.preferences.stream.max_bitrate_kbps,
                                None,
                                "Unlimited",
                            );
                            for rate in [64, 96, 128, 192, 256, 320] {
                                ui.selectable_value(
                                    &mut self.preferences.stream.max_bitrate_kbps,
                                    Some(rate),
                                    format!("{rate} kbps"),
                                );
                            }
                        });
                },
            );
        }
        ui.separator();
        heading(ui, "Listening history", 20.0);
        setting(
            ui,
            "Scrobble playback",
            "Submit listening history to the music server",
            |ui| {
                toggle(ui, &mut self.preferences.scrobble_enabled);
            },
        );
        setting(
            ui,
            "Private mode",
            "Suppress listening-history submissions",
            |ui| {
                toggle(ui, &mut self.preferences.private_mode);
            },
        );
    }
    fn hotkey_settings(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Hotkeys", 20.0);
        ui.label("Use combinations such as Ctrl+ArrowRight, Alt+ArrowLeft, Space, or F11.");
        let keys = self
            .preferences
            .shortcuts
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            setting(ui, &key.replace('_', " "), "", |ui| {
                if let Some(value) = self.preferences.shortcuts.get_mut(&key) {
                    ui.text_edit_singleline(value);
                    if !value.trim().is_empty() && shortcuts::parse(value).is_none() {
                        ui.colored_label(Color32::from_rgb(230, 110, 110), "Invalid shortcut");
                    }
                }
            });
        }
        if ui.button("Restore default hotkeys").clicked() {
            self.preferences.shortcuts = Preferences::default().shortcuts;
        }
    }
    fn window_settings(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Window and layout", 20.0);
        setting(
            ui,
            "Sidebar width",
            "Drag the sidebar separator or set its size here",
            |ui| {
                ui.add(
                    egui::Slider::new(
                        &mut self.preferences.appearance.sidebar_width,
                        200.0..=600.0,
                    )
                    .suffix(" px"),
                );
            },
        );
        setting(
            ui,
            "Collapse sidebar",
            "Show navigation icons without labels",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.sidebar_collapsed);
            },
        );
        setting(
            ui,
            "Expand cover in sidebar",
            "Show the current album cover above the player bar",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.expand_sidebar_art);
            },
        );
        setting(
            ui,
            "Compact playlists",
            "Use smaller rows in the sidebar",
            |ui| {
                toggle(ui, &mut self.preferences.appearance.playlist_compact);
            },
        );
        setting(
            ui,
            "Restore queue",
            "Restore the play queue after restarting",
            |ui| {
                toggle(ui, &mut self.preferences.resume_queue);
            },
        );
        setting(
            ui,
            "Restore position",
            "Remember the last playback position",
            |ui| {
                toggle(ui, &mut self.preferences.resume_position);
            },
        );
        setting(
            ui,
            "Desktop notifications",
            "Show track-change notifications",
            |ui| {
                toggle(ui, &mut self.preferences.notifications);
            },
        );
    }
    fn advanced_settings(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Music servers", 20.0);
        for (index, profile) in self.profiles.clone().iter().enumerate() {
            Frame::new()
                .fill(self.colors.surface)
                .inner_margin(12)
                .corner_radius(4)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(if profile.name.is_empty() {
                                    profile.kind.label()
                                } else {
                                    &profile.name
                                })
                                .strong(),
                            );
                            ui.label(RichText::new(&profile.url).small().color(self.colors.muted));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Remove").clicked() {
                                actions.push(Action::RemoveServer(index));
                            }
                            if ui.button("Connect").clicked() {
                                actions.push(Action::SelectServer(index));
                            }
                        });
                    });
                });
        }
        ui.add_space(14.0);
        egui::CollapsingHeader::new("Add or edit a server").show(ui, |ui| {
            self.settings_form(ui, actions);
            if ui.button("Save server profile").clicked() {
                actions.push(Action::SaveServer(self.profile.clone()));
            }
        });
        if ui
            .button("Forget saved password for current server")
            .clicked()
        {
            actions.push(Action::ForgetPassword);
        }
        ui.separator();
        heading(ui, "Downloads", 20.0);
        ui.label("Default download folder");
        ui.text_edit_singleline(&mut self.preferences.download_directory);
        ui.label(RichText::new("Downloads are original server files; authentication stays inside the server adapter.").small().color(self.colors.muted));
        ui.separator();
        heading(ui, "Audio information", 20.0);
        ui.label(format!("Codec: {}", self.audio_stats.codec));
        ui.label(format!(
            "Source: {} Hz · {} · {} kbps",
            self.audio_stats.sample_rate,
            self.audio_stats.channels,
            self.audio_stats.bitrate / 1000
        ));
        ui.label(format!(
            "Output: {} Hz · {}",
            self.audio_stats.output_sample_rate, self.audio_stats.output_channels
        ));
        ui.separator();
        heading(ui, "About", 20.0);
        ui.label(concat!("feishin-rs ", env!("CARGO_PKG_VERSION")));
        ui.label("Native Rust interface · MPV playback · MIT original application code");
    }
    pub(super) fn favorites_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Favorites", 28.0);
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            for (category, label) in [
                (FavoriteCategory::Tracks, "Tracks"),
                (FavoriteCategory::Albums, "Albums"),
                (FavoriteCategory::Artists, "Artists"),
            ] {
                ui.selectable_value(&mut self.favorite_category, category, label);
            }
        });
        ui.add_space(18.0);
        match self.favorite_category {
            FavoriteCategory::Albums => {
                let albums = self
                    .library
                    .albums
                    .iter()
                    .filter(|a| a.favorite)
                    .cloned()
                    .collect::<Vec<_>>();
                if albums.is_empty() {
                    empty(
                        ui,
                        "No favorite albums",
                        "Add albums to your favorites from their menu.",
                    );
                }
                self.album_grid(ui, &albums, actions);
            }
            FavoriteCategory::Artists => {
                let artists = self
                    .library
                    .artists
                    .iter()
                    .filter(|a| a.favorite)
                    .cloned()
                    .collect::<Vec<_>>();
                self.artist_grid(ui, &artists, actions);
            }
            FavoriteCategory::Tracks => {
                let tracks = self
                    .library
                    .tracks
                    .iter()
                    .filter(|t| t.favorite)
                    .collect::<Vec<_>>();
                self.track_table(ui, &tracks, actions, true);
            }
        }
    }
    pub(super) fn genres_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Genres", 28.0);
        ui.add_space(18.0);
        for genre in self.library.genres.clone() {
            if ui.button(&genre).clicked() {
                self.navigate(Page::Albums);
                self.genre = Some(genre);
            }
        }
        let _ = actions;
    }
    pub(super) fn radio_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Radio Stations", 28.0);
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                actions.push(Action::LoadRadios);
            }
            if ui.button("Add station").clicked() {
                self.radio_draft = Some(RadioStation::default());
            }
        });
        ui.add_space(18.0);
        for station in self.radio_stations.clone() {
            Frame::new()
                .fill(self.colors.surface)
                .inner_margin(16)
                .corner_radius(4)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if icon_button(ui, Icon::Play, "Play station", false, true).clicked() {
                            actions.push(Action::PlayRadio(station.clone()));
                        }
                        ui.label(&station.name);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Edit").clicked() {
                                self.radio_draft = Some(station.clone());
                            }
                            if ui.button("Delete").clicked() {
                                self.radio_delete_pending = Some(station.id.clone());
                            }
                        });
                    });
                });
        }
    }
    pub(super) fn smart_playlists_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Smart Playlists", 28.0);
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                actions.push(Action::LoadSmartPlaylists);
            }
            if ui.button("Create smart playlist").clicked() {
                self.smart_draft = Some(SmartPlaylist {
                    rules_json: "{\n  \"all\": [{\"gt\": {\"playCount\": -1}}],\n  \"sort\": \"random\",\n  \"limit\": 50\n}".into(),
                    ..Default::default()
                });
            }
        });
        ui.add_space(18.0);
        for playlist in self.smart_playlists.clone() {
            ui.horizontal(|ui| {
                ui.label(&playlist.name);
                if ui.button("Edit rules").clicked() {
                    self.smart_draft = Some(playlist.clone());
                }
                if ui.button("Delete").clicked() {
                    self.smart_delete_pending = Some(playlist.id.clone());
                }
            });
        }
    }
    pub(super) fn extra_dialogs(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if (self.connected || self.demo) && self.page == Page::Settings {
            self.settings_modal(ui, actions);
        }
        let ctx = ui.ctx().clone();
        if self.track_info_open {
            let mut open = true;
            egui::Window::new("Track information")
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(&ctx, |ui| {
                    if let Some(detail) = &self.track_details {
                        heading(ui, &detail.track.title, 22.0);
                        ui.label(&detail.track.artist);
                        ui.label(&detail.track.album);
                        ui.separator();
                        for (label, value) in [
                            ("Codec", detail.codec.clone()),
                            ("Bitrate", format!("{} kbps", detail.bitrate_kbps)),
                            ("Sample rate", format!("{} Hz", detail.sample_rate)),
                            ("Bit depth", format!("{} bit", detail.bit_depth)),
                            ("Channels", detail.channels.to_string()),
                            (
                                "Size",
                                format!("{:.2} MiB", detail.size_bytes as f64 / 1048576.0),
                            ),
                            ("Play count", detail.play_count.to_string()),
                            ("Comment", detail.comment.clone()),
                            ("MusicBrainz ID", detail.musicbrainz_id.clone()),
                        ] {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(label);
                                ui.label(value);
                            });
                        }
                    } else {
                        ui.spinner();
                        ui.label("Loading track information…");
                    }
                });
            self.track_info_open = open;
        }
        if self.editing_playlist {
            let mut open = true;
            egui::Window::new("Edit playlist")
                .open(&mut open)
                .default_width(460.0)
                .show(&ctx, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.playlist_metadata_draft.name);
                    ui.label("Description");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.playlist_metadata_draft.description)
                            .desired_rows(4),
                    );
                    ui.checkbox(&mut self.playlist_metadata_draft.public, "Public playlist");
                    if ui.button("Save changes").clicked()
                        && !self.playlist_metadata_draft.name.trim().is_empty()
                    {
                        if let Some(p) = &self.selected_playlist {
                            actions.push(Action::UpdatePlaylist {
                                id: p.id.clone(),
                                metadata: self.playlist_metadata_draft.clone(),
                            });
                        }
                        self.editing_playlist = false;
                    }
                });
            self.editing_playlist &= open;
        }
        if let Some(mut station) = self.radio_draft.take() {
            let mut open = true;
            let mut saved = false;
            egui::Window::new("Radio station")
                .open(&mut open)
                .default_width(460.0)
                .show(&ctx, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut station.name);
                    ui.label("Stream URL");
                    ui.text_edit_singleline(&mut station.stream_url);
                    ui.label("Homepage URL");
                    ui.text_edit_singleline(&mut station.homepage_url);
                    if ui
                        .add_enabled(
                            !station.name.trim().is_empty()
                                && !station.stream_url.trim().is_empty(),
                            egui::Button::new("Save station"),
                        )
                        .clicked()
                    {
                        actions.push(Action::SaveRadio(station.clone()));
                        saved = true;
                    }
                });
            if open && !saved {
                self.radio_draft = Some(station);
            }
        }
        if let Some(mut playlist) = self.smart_draft.take() {
            let mut open = true;
            let mut saved = false;
            egui::Window::new("Smart playlist")
                .open(&mut open)
                .default_width(650.0)
                .show(&ctx, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut playlist.name);
                    ui.label("Description");
                    ui.text_edit_singleline(&mut playlist.description);
                    ui.checkbox(&mut playlist.public, "Public playlist");
                    ui.label("Navidrome criteria JSON");
                    ui.add(
                        egui::TextEdit::multiline(&mut playlist.rules_json)
                            .code_editor()
                            .desired_rows(16)
                            .desired_width(f32::INFINITY),
                    );
                    let valid = serde_json::from_str::<serde_json::Value>(&playlist.rules_json)
                        .is_ok_and(|v| v.is_object());
                    if !valid {
                        ui.colored_label(
                            Color32::from_rgb(240, 120, 120),
                            "Rules must be a valid JSON object.",
                        );
                    }
                    if ui
                        .add_enabled(
                            valid && !playlist.name.trim().is_empty(),
                            egui::Button::new("Save smart playlist"),
                        )
                        .clicked()
                    {
                        actions.push(Action::SaveSmartPlaylist(playlist.clone()));
                        saved = true;
                    }
                });
            if open && !saved {
                self.smart_draft = Some(playlist);
            }
        }
        if let Some(id) = self.radio_delete_pending.clone() {
            egui::Window::new("Delete radio station?")
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label("Remove this station from the music server?");
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            actions.push(Action::DeleteRadio(id));
                            self.radio_delete_pending = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.radio_delete_pending = None;
                        }
                    });
                });
        }
        if let Some(id) = self.smart_delete_pending.clone() {
            egui::Window::new("Delete smart playlist?")
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label("Remove this smart playlist from the music server?");
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            actions.push(Action::DeleteSmartPlaylist(id));
                            self.smart_delete_pending = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.smart_delete_pending = None;
                        }
                    });
                });
        }
        if let Some(track) = self.download_draft.clone() {
            let mut open = true;
            egui::Window::new("Download track")
                .open(&mut open)
                .default_width(540.0)
                .show(&ctx, |ui| {
                    ui.label(&track.title);
                    ui.label("Destination file");
                    ui.text_edit_singleline(&mut self.download_path);
                    if ui
                        .add_enabled(
                            !self.download_path.trim().is_empty(),
                            egui::Button::new("Download original file"),
                        )
                        .clicked()
                    {
                        actions.push(Action::DownloadTrack {
                            track: track.clone(),
                            destination: self.download_path.clone(),
                        });
                        self.download_draft = None;
                    }
                });
            if !open {
                self.download_draft = None;
            }
        }
    }
    pub(super) fn saved_filter_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("saved-filter")
                .selected_text("Saved filters")
                .show_ui(ui, |ui| {
                    for filter in self.preferences.saved_filters.clone() {
                        ui.horizontal(|ui| {
                            if ui.button(&filter.name).clicked() {
                                self.filter = filter.query.clone();
                                self.genre =
                                    (!filter.genre.is_empty()).then_some(filter.genre.clone());
                                self.favorites_only = filter.favorites_only;
                                self.min_rating = filter.min_rating;
                                self.sort = match filter.sort.as_str() {
                                    "artist" => Sort::Artist,
                                    "year" => Sort::Year,
                                    _ => Sort::Title,
                                };
                                self.descending = filter.descending;
                            }
                            if ui
                                .small_button("×")
                                .on_hover_text("Delete saved filter")
                                .clicked()
                            {
                                self.preferences
                                    .saved_filters
                                    .retain(|saved| saved.name != filter.name);
                            }
                        });
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.saved_filter_name)
                    .hint_text("Filter name")
                    .desired_width(130.0),
            );
            if ui
                .add_enabled(
                    !self.saved_filter_name.trim().is_empty(),
                    egui::Button::new("Save filter"),
                )
                .clicked()
            {
                let name = self.saved_filter_name.trim().to_owned();
                self.preferences.saved_filters.retain(|f| f.name != name);
                self.preferences.saved_filters.push(SavedFilter {
                    name,
                    query: self.filter.clone(),
                    genre: self.genre.clone().unwrap_or_default(),
                    favorites_only: self.favorites_only,
                    min_rating: self.min_rating,
                    sort: match self.sort {
                        Sort::Artist => "artist",
                        Sort::Year => "year",
                        _ => "title",
                    }
                    .into(),
                    descending: self.descending,
                });
                self.saved_filter_name.clear();
            }
        });
    }
    pub(super) fn lyrics_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let lyrics = self.structured_lyrics.clone();
        if lyrics.lines.is_empty() {
            if self.lyrics.trim().is_empty() {
                ui.label(
                    RichText::new("No lyrics available for this track.").color(self.colors.muted),
                );
            } else {
                for line in self.lyrics.lines() {
                    ui.label(RichText::new(line).size(self.preferences.lyrics.font_size));
                    ui.add_space(10.0);
                }
            }
            return;
        }
        let position = ((self.playback.position * 1000.0).max(0.0) as i128)
            - i128::from(self.preferences.lyrics.delay_ms)
            + i128::from(self.preferences.lyrics.lead_ms);
        let active = lyrics.active_line(position.clamp(0, i128::from(u64::MAX)) as u64);
        let active_changed = active != self.last_lyric_line;
        self.last_lyric_line = active;
        for (index, line) in lyrics.lines.iter().enumerate() {
            let highlighted = active == Some(index);
            let size = self.preferences.lyrics.font_size;
            let color = if highlighted || !lyrics.synced {
                self.colors.text
            } else {
                self.colors.muted.gamma_multiply(0.8)
            };
            let alignment = match self.preferences.lyrics.alignment.as_str() {
                "left" => Align::LEFT,
                "right" => Align::RIGHT,
                _ => Align::Center,
            };
            ui.with_layout(Layout::top_down(alignment), |ui| {
                let response = ui.add(
                    egui::Label::new(RichText::new(&line.text).size(size).color(color))
                        .wrap()
                        .sense(Sense::click()),
                );
                if response.clicked()
                    && let Some(start) = line.start_ms
                {
                    actions.push(Action::Seek(start as f64 / 1000.0));
                }
                if highlighted && active_changed && self.preferences.lyrics.follow {
                    response.scroll_to_me(Some(Align::Center));
                }
            });
            ui.add_space(16.0);
        }
    }
    pub(super) fn audio_levels_panel(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Live MPV audio levels").strong());
        ui.label(
            RichText::new("Stereo RMS and peak levels in dBFS")
                .small()
                .color(self.colors.muted),
        );
        ui.add_space(25.0);
        for (channel, rms, peak) in [
            ("L", self.audio_levels[0], self.audio_levels[2]),
            ("R", self.audio_levels[1], self.audio_levels[3]),
        ] {
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            ui.painter().text(
                rect.left_center(),
                egui::Align2::LEFT_CENTER,
                channel,
                super::font_id(ui, 16.0, false),
                self.colors.text,
            );
            let rail = Rect::from_min_max(rect.min + vec2(30.0, 10.0), rect.max - vec2(55.0, 10.0));
            ui.painter().rect_filled(rail, 3.0, self.colors.surface);
            let fraction = ((rms.max(-60.0) + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
            let fill = Rect::from_min_size(rail.min, vec2(rail.width() * fraction, rail.height()));
            ui.painter().rect_filled(
                fill,
                3.0,
                if rms > -3.0 {
                    Color32::from_rgb(240, 90, 90)
                } else {
                    self.colors.accent
                },
            );
            let x = rail.left()
                + rail.width() * (((peak.max(-60.0) + 60.0) / 60.0).clamp(0.0, 1.0) as f32);
            ui.painter().line_segment(
                [pos2(x, rail.top() - 2.0), pos2(x, rail.bottom() + 2.0)],
                Stroke::new(2.0, self.colors.text),
            );
            ui.painter().text(
                rect.right_center(),
                egui::Align2::RIGHT_CENTER,
                format!("{rms:.1}"),
                super::font_id(ui, 11.0, false),
                self.colors.muted,
            );
        }
    }
}
fn toggle(ui: &mut egui::Ui, value: &mut bool) {
    let (rect, mut response) = ui.allocate_exact_size(vec2(38.0, 20.0), Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *value, "Enabled")
    });
    let color = if *value {
        ui.visuals().selection.bg_fill
    } else {
        ui.visuals().extreme_bg_color
    };
    ui.painter().rect_filled(rect, 10.0, color);
    ui.painter().rect_stroke(
        rect,
        10.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    ui.painter().circle_filled(
        pos2(
            if *value {
                rect.right() - 10.0
            } else {
                rect.left() + 10.0
            },
            rect.center().y,
        ),
        7.0,
        Color32::from_gray(224),
    );
}

fn setting(ui: &mut egui::Ui, label: &str, description: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        let width = (ui.available_width() - 294.0).max(150.0);
        ui.allocate_ui_with_layout(vec2(width, 45.0), Layout::top_down(Align::LEFT), |ui| {
            ui.set_min_width(width);
            ui.label(label);
            if !description.is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(description)
                            .size(12.0)
                            .color(ui.visuals().weak_text_color()),
                    )
                    .wrap(),
                );
            }
        });
        ui.allocate_ui_with_layout(
            vec2(240.0, 40.0),
            Layout::right_to_left(Align::Center),
            control,
        );
    });
    ui.add_space(7.0);
}

impl View {
    pub(super) fn titlebar(&self, ui: &mut egui::Ui) {
        egui::Panel::top("window-titlebar")
            .exact_size(30.0)
            .frame(
                Frame::new()
                    .fill(self.colors.side)
                    .stroke(Stroke::new(1.0, self.colors.line))
                    .inner_margin(0),
            )
            .show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                let title = self.queue.current_track().map_or_else(
                    || "Feishin".to_owned(),
                    |t| {
                        format!(
                            "{}{} - {} - Feishin",
                            if self.playback.paused {
                                "(Paused) "
                            } else {
                                ""
                            },
                            t.title,
                            t.artist
                        )
                    },
                );
                ui.painter().text(
                    pos2(rect.left() + 39.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    title,
                    super::font_id(ui, 12.0, false),
                    self.colors.text,
                );
                icons::draw(
                    ui.painter(),
                    Icon::Disc,
                    Rect::from_center_size(
                        pos2(rect.left() + 22.0, rect.center().y),
                        vec2(14.0, 14.0),
                    ),
                    self.colors.text,
                );
                let drag = ui.interact(
                    Rect::from_min_max(rect.min, rect.right_bottom() - vec2(132.0, 0.0)),
                    ui.scope_id().with("drag-title"),
                    Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for kind in [0, 1, 2] {
                        let (r, response) =
                            ui.allocate_exact_size(vec2(44.0, 29.0), Sense::click());
                        if response.hovered() {
                            ui.painter().rect_filled(
                                r,
                                0.0,
                                if kind == 0 {
                                    Color32::from_rgb(175, 40, 50)
                                } else {
                                    self.colors.surface
                                },
                            );
                        }
                        let center = r.center();
                        match kind {
                            0 => icons::draw(
                                ui.painter(),
                                Icon::Close,
                                Rect::from_center_size(center, vec2(12.0, 12.0)),
                                self.colors.text,
                            ),
                            1 => {
                                ui.painter().rect_stroke(
                                    Rect::from_center_size(center, vec2(9.0, 9.0)),
                                    0.0,
                                    Stroke::new(1.0, self.colors.text),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            _ => {
                                ui.painter().line_segment(
                                    [center - vec2(5.0, 0.0), center + vec2(5.0, 0.0)],
                                    Stroke::new(1.0, self.colors.text),
                                );
                            }
                        }
                        if response.clicked() {
                            match kind {
                                0 => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                                1 => {
                                    let max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                                    ui.ctx()
                                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                                }
                                _ => ui
                                    .ctx()
                                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                            }
                        }
                    }
                });
            });
    }
    pub(super) fn server_required(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        self.banners(ui);
        let width = ui.available_width().min(690.0);
        let offset = ((ui.available_width() - width) / 2.0).max(0.0);
        ui.horizontal(|ui|{ui.add_space(offset);ui.allocate_ui_with_layout(vec2(width,700.0),Layout::top_down(Align::LEFT),|ui|{
            ui.set_min_width(width);ui.add_space(30.0);ui.label(RichText::new("SERVER REQUIRED").size(18.0));ui.add_space(26.0);
            Frame::new().fill(self.colors.surface).inner_margin(12).corner_radius(3).show(ui,|ui|{ui.horizontal(|ui|{ui.spacing_mut().item_spacing.x=0.0;for kind in[feishin_core::ServerKind::Jellyfin,feishin_core::ServerKind::Navidrome,feishin_core::ServerKind::OpenSubsonic]{let(r,response)=ui.allocate_exact_size(vec2((width-24.0)/3.0,86.0),Sense::click());if self.profile.kind==kind{ui.painter().rect_filled(r,0.0,self.colors.line);}icons::draw(ui.painter(),Icon::Disc,Rect::from_center_size(pos2(r.center().x,r.top()+28.0),vec2(40.0,40.0)),self.colors.accent);ui.painter().text(pos2(r.center().x,r.bottom()-12.0),egui::Align2::CENTER_CENTER,kind.label(),super::font_id(ui, 13.0, false),self.colors.text);if response.clicked(){self.profile.kind=kind;}}});});
            ui.add_space(8.0);ui.horizontal(|ui|{ui.vertical(|ui|{ui.set_width((width-12.0)/2.0);ui.label(RichText::new("Server Name *").strong());ui.add_sized([ui.available_width(),36.0],egui::TextEdit::singleline(&mut self.profile.name).hint_text("My Server"));});ui.vertical(|ui|{ui.set_width((width-12.0)/2.0);ui.label(RichText::new("URL *").strong());ui.add_sized([ui.available_width(),36.0],egui::TextEdit::singleline(&mut self.profile.url).hint_text("http://"));});});
            ui.add_space(20.0);ui.label(RichText::new("Username *").strong());ui.add_sized([width,36.0],egui::TextEdit::singleline(&mut self.profile.username));ui.add_space(12.0);ui.label(RichText::new("Password").strong());ui.label(RichText::new("Username/password authentication is supported. Credentials stay in memory unless saved in the OS credential store.").small().color(self.colors.muted));ui.add_sized([width,36.0],egui::TextEdit::singleline(&mut self.password).password(true));ui.checkbox(&mut self.remember_password,"Save Password");
            if ui.add_enabled(!self.loading&&!self.profile.url.trim().is_empty()&&!self.profile.username.trim().is_empty(),egui::Button::new(RichText::new("ADD").color(Color32::WHITE)).fill(self.colors.accent).min_size(vec2(width,36.0))).clicked(){actions.push(Action::Connect{profile:self.profile.clone(),password:std::mem::take(&mut self.password)});}
            if !self.profiles.is_empty(){ui.add_space(16.0);ui.label("Saved servers");for(index,profile)in self.profiles.iter().enumerate(){if ui.button(if profile.name.is_empty(){profile.kind.label()}else{&profile.name}).clicked(){actions.push(Action::SelectServer(index));}}}
        });});
    }
    pub(super) fn folders_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        heading(ui, "Folders", 28.0);
        ui.horizontal(|ui| {
            if ui.button("Music folders").clicked() {
                self.directory = None;
                actions.push(Action::LoadFolders);
            }
            if let Some(directory) = &self.directory
                && let Some(parent) = &directory.parent_id
                && ui.button("Parent folder").clicked()
            {
                actions.push(Action::OpenFolder {
                    id: parent.clone(),
                    root: false,
                });
            }
        });
        ui.add_space(16.0);
        if let Some(directory) = self.directory.clone() {
            heading(ui, &directory.name, 20.0);
            for entry in directory.entries {
                ui.horizontal(|ui| {
                    if entry.is_directory {
                        icons::paint_icon(ui, Icon::Folder, 16.0, self.colors.muted);
                        if ui.button(&entry.name).clicked() {
                            actions.push(Action::OpenFolder {
                                id: entry.id,
                                root: false,
                            });
                        }
                    } else if let Some(track) = entry.track
                        && ui.button(&track.title).double_clicked()
                    {
                        actions.push(Action::Play {
                            tracks: vec![track],
                            start: 0,
                        });
                    }
                });
            }
        } else {
            for folder in &self.music_folders {
                if ui.button(&folder.name).clicked() {
                    actions.push(Action::OpenFolder {
                        id: folder.id.clone(),
                        root: true,
                    });
                }
            }
        }
    }
    pub(super) fn album_artists_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        self.toolbar(ui, "Album Artists", self.album_artists.len(), actions);
        let artists = self.filtered_artists(&self.album_artists);
        self.artist_grid(ui, &artists, actions);
    }
}
