//! Queue-aware preparation for MPV's same-process gapless playlist transitions.
use super::*;
use rand::seq::SliceRandom;

pub(super) struct PreparedPlayback {
    pub ticket: u64,
    pub source_generation: u64,
    pub generation: u64,
    pub queue: Queue,
    pub ready: bool,
    pub transcoded: bool,
}

impl Application {
    pub(super) fn allocate_generation(&mut self) -> u64 {
        self.generation_counter = self.generation_counter.max(self.stream).wrapping_add(1);
        self.generation_counter
    }
    pub(super) fn invalidate_preparation(&mut self) {
        self.preparation_ticket = self.preparation_ticket.wrapping_add(1);
        if let Some(prepared) = self.preparation.take() {
            self.retired_preparations
                .push_back((prepared.generation, prepared.source_generation));
            while self.retired_preparations.len() > 64 {
                self.retired_preparations.pop_front();
            }
        }
        self.send(Command::ClearNext);
    }
    pub(super) fn recover_cancelled_preparation(&mut self, ctx: &egui::Context, generation: u64) {
        let Some(index) = self
            .retired_preparations
            .iter()
            .position(|(g, _)| *g == generation)
        else {
            return;
        };
        let Some((_, source_generation)) = self.retired_preparations.remove(index) else {
            return;
        };
        // A newer explicit load/stop already has authority over the player.
        if source_generation != self.stream {
            return;
        }
        // MPV can begin an appended file just before receiving ClearNext. Reload
        // the still-selected occurrence at its observed position; never let the
        // retired snapshot replace queue edits or keep playing untracked audio.
        if self.client.is_some() && self.view.queue.current_track().is_some() {
            let position = self.view.playback.position;
            let id = self
                .view
                .queue
                .current_track()
                .map(|track| track.id.clone());
            let paused = self.desired_paused;
            self.play_current(ctx);
            self.restored_position = id.map(|id| (id, position));
            self.desired_paused = paused;
        } else {
            self.stop();
        }
    }
    pub(super) fn prepare_next(&mut self, ctx: &egui::Context) {
        if self.radio || self.active_track.is_none() || self.preparation.is_some() {
            return;
        }
        let mut queue = self.view.queue.clone();
        let Some(track) = queue.advance(true).cloned() else {
            self.request_auto_dj(ctx);
            return;
        };
        if !self.view.preferences.playback.gapless {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let current = self.stream;
        let generation = self.allocate_generation();
        let ticket = self.preparation_ticket;
        let session = self.session;
        let options = self.view.preferences.stream.clone();
        self.preparation = Some(PreparedPlayback {
            ticket,
            source_generation: current,
            generation,
            queue,
            ready: false,
            transcoded: false,
        });
        self.background(ctx, move || {
            Message::PreparedSource(
                session,
                current,
                ticket,
                generation,
                client
                    .stream_with_options(&track, &options)
                    .map_err(|e| e.to_string()),
            )
        });
    }
    pub(super) fn prepared_source(
        &mut self,
        current: u64,
        ticket: u64,
        generation: u64,
        result: Result<StreamSource, String>,
    ) {
        if current != self.stream
            || !self.preparation.as_ref().is_some_and(|p| {
                p.ticket == ticket && p.generation == generation && p.source_generation == current
            })
        {
            return;
        }
        match result {
            Ok(source) => {
                if let Some(prepared) = &mut self.preparation {
                    prepared.transcoded = source.transcoded;
                }
                self.send(Command::PrepareNext {
                    current_generation: current,
                    generation,
                    url: source.url,
                    headers: source.headers,
                });
            }
            Err(error) => {
                self.preparation = None;
                self.view.message = Some(error);
            }
        }
    }
    pub(super) fn adopt_prepared(&mut self, generation: u64) -> bool {
        if !self
            .preparation
            .as_ref()
            .is_some_and(|p| p.generation == generation && p.source_generation == self.stream)
        {
            return false;
        }
        let Some(prepared) = self.preparation.take() else {
            return false;
        };
        self.finish_active(true);
        self.view.queue = prepared.queue;
        self.stream_transcoded = prepared.transcoded;
        self.stream = generation;
        self.view.playback.position = 0.0;
        self.view.playback.duration = self.view.queue.current_track().map_or(0.0, |t| t.duration);
        self.view.playback.buffering = true;
        self.dirty = true;
        true
    }
    pub(super) fn request_auto_dj(&mut self, ctx: &egui::Context) {
        if !self.view.preferences.auto_dj.enabled || self.auto_dj_pending || self.radio {
            return;
        }
        let (Some(client), Some(track)) = (
            self.client.clone(),
            self.view.queue.current_track().cloned(),
        ) else {
            return;
        };
        self.auto_dj_pending = true;
        let settings = self.view.preferences.auto_dj.clone();
        let existing: HashSet<_> = self
            .view
            .queue
            .tracks
            .iter()
            .map(|t| t.id.clone())
            .collect();
        let fallback = self.normal_library.tracks.clone();
        let (session, generation) = (self.session, self.stream);
        self.background(ctx, move || {
            let mut candidates = client
                .similar_tracks(&track, settings.item_count.min(100) as u32)
                .unwrap_or_default();
            if candidates.is_empty() && !settings.similar_only {
                candidates = fallback;
            }
            if !settings.allow_duplicates {
                candidates.retain(|t| !existing.contains(&t.id));
                let mut seen = HashSet::new();
                candidates.retain(|t| seen.insert(t.id.clone()));
            }
            candidates.shuffle(&mut rand::rng());
            candidates.truncate(settings.item_count);
            Message::AutoDj(session, generation, candidates)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> (Application, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = Application::new(&ctx, true, None);
        app.persistence_enabled = false;
        app.stream = 9;
        app.active_track = app.view.queue.current_track().cloned();
        (app, ctx)
    }

    fn prepare(app: &mut Application) {
        app.generation_counter = 10;
        let mut queue = app.view.queue.clone();
        queue.advance(true).unwrap();
        app.preparation = Some(PreparedPlayback {
            ticket: 1,
            source_generation: 9,
            generation: 10,
            queue,
            ready: true,
            transcoded: false,
        });
    }

    #[test]
    fn auto_dj_rechecks_current_queue_and_retires_prepared_snapshot() {
        let (mut app, ctx) = app();
        prepare(&mut app);
        app.view.preferences.auto_dj.enabled = true;
        let count = app.view.queue.tracks.len();
        let duplicate = app.view.queue.tracks[0].clone();
        let added = Track {
            id: "new-track".into(),
            ..Default::default()
        };
        app.tx
            .send(Message::AutoDj(0, 9, vec![duplicate, added.clone(), added]))
            .unwrap();
        app.poll(&ctx);
        assert_eq!(app.view.queue.tracks.len(), count + 1);
        assert_eq!(app.view.queue.tracks.last().unwrap().id, "new-track");
        assert!(!app.adopt_prepared(10));
        app.view.preferences.auto_dj.enabled = false;
        app.tx
            .send(Message::AutoDj(0, 9, vec![Track::default()]))
            .unwrap();
        app.poll(&ctx);
        assert_eq!(app.view.queue.tracks.len(), count + 1);
    }

    #[test]
    fn prepared_file_loaded_before_retired_eof_advances_only_once() {
        let (mut app, ctx) = app();
        let current = app.view.queue.current.unwrap();
        prepare(&mut app);
        app.player_event(&ctx, Event::FileLoaded(10));
        assert_eq!(app.stream, 10);
        assert_eq!(app.view.queue.current, Some(current + 1));
        assert_eq!(
            app.active_track.as_ref().unwrap(),
            app.view.queue.current_track().unwrap()
        );
        app.player_event(
            &ctx,
            Event::EndFile {
                generation: 9,
                reason: EndReason::Eof,
            },
        );
        assert_eq!(app.view.queue.current, Some(current + 1));
        assert!(!app.adopt_prepared(10));
    }

    #[test]
    fn favorite_updates_survive_adopting_prepared_queue() {
        let (mut app, ctx) = app();
        prepare(&mut app);
        let id = app
            .preparation
            .as_ref()
            .unwrap()
            .queue
            .current_track()
            .unwrap()
            .id
            .clone();
        app.tx
            .send(Message::Favorite(
                0,
                ItemKind::Track,
                id.clone(),
                true,
                Ok(()),
            ))
            .unwrap();
        app.poll(&ctx);
        assert!(app.adopt_prepared(10));
        let track = app.view.queue.current_track().unwrap();
        assert_eq!(track.id, id);
        assert!(track.favorite);
    }

    #[test]
    fn cancelled_prepared_audio_is_stopped_but_cannot_stop_a_new_selection() {
        let (mut app, ctx) = app();
        prepare(&mut app);
        app.invalidate_preparation();
        assert_eq!(app.stream, 9);
        app.player_event(&ctx, Event::FileLoaded(10));
        assert!(app.view.playback.paused);
        assert!(app.active_track.is_none());
        assert!(app.stream > 9);

        app.stream = 9;
        prepare(&mut app);
        app.invalidate_preparation();
        app.stream = 11;
        app.active_track = app.view.queue.current_track().cloned();
        app.view.playback.paused = false;
        app.player_event(&ctx, Event::FileLoaded(10));
        assert_eq!(app.stream, 11);
        assert!(!app.view.playback.paused);
        assert!(app.active_track.is_some());
    }

    #[test]
    fn retired_preparation_cannot_replace_a_newly_selected_queue() {
        let ctx = egui::Context::default();
        let mut app = Application::new(&ctx, true, None);
        app.persistence_enabled = false;
        app.stream = 9;
        app.view.queue.select(2);
        let queue = app.view.queue.clone();
        app.preparation = Some(PreparedPlayback {
            ticket: 1,
            source_generation: 8,
            generation: 10,
            queue,
            ready: true,
            transcoded: false,
        });
        assert!(!app.adopt_prepared(10));
        assert_eq!(app.view.queue.current, Some(2));
        app.invalidate_preparation();
        assert!(app.preparation.is_none());
    }
}
