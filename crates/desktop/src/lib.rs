//! Linux MPRIS integration. D-Bus operations never run on the UI thread.
use feishin_core::{Repeat, Track};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopState {
    pub track: Option<Track>,
    pub paused: bool,
    pub position: f64,
    /// Application volume is a percentage; MPRIS uses 0.0 through 1.0.
    pub volume: f64,
    pub can_next: bool,
    pub can_previous: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            track: None,
            paused: true,
            position: 0.0,
            volume: 100.0,
            can_next: false,
            can_previous: false,
            shuffle: false,
            repeat: Repeat::Off,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DesktopCommand {
    Play,
    Pause,
    TogglePause,
    Stop,
    Next,
    Previous,
    Seek(f64),
    Volume(f64),
    Shuffle(bool),
    Repeat(Repeat),
    Raise,
    Quit,
    /// An asynchronous startup/connection diagnostic; never includes bus data.
    Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not start the desktop media-control worker.")]
    Worker,
    #[error("The desktop media-control state is unavailable.")]
    State,
}

pub struct Desktop {
    commands: Receiver<DesktopCommand>,
    mailbox: Arc<Mutex<Option<DesktopState>>>,
    seeked: Arc<Mutex<Option<f64>>>,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Desktop {
    /// Returns immediately; missing D-Bus/session errors arrive in try_recv().
    pub fn start() -> Result<Self, Error> {
        let (tx, commands) = mpsc::sync_channel(128);
        let mailbox = Arc::new(Mutex::new(None));
        let seeked = Arc::new(Mutex::new(None));
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_mailbox = Arc::clone(&mailbox);
        let worker_seeked = Arc::clone(&seeked);
        let worker_stopping = Arc::clone(&stopping);
        let worker = thread::Builder::new()
            .name("feishin-mpris".into())
            .spawn(move || {
                if let Err(message) =
                    run(tx.clone(), worker_mailbox, worker_seeked, &worker_stopping)
                {
                    let _ = tx.try_send(DesktopCommand::Error(message.into()));
                }
            })
            .map_err(|_| Error::Worker)?;
        Ok(Self {
            commands,
            mailbox,
            seeked,
            stopping,
            worker: Some(worker),
        })
    }

    /// Coalesces snapshots instead of building an unbounded update queue.
    pub fn update(&self, state: DesktopState) -> Result<(), Error> {
        *self.mailbox.lock().map_err(|_| Error::State)? = Some(state);
        Ok(())
    }

    pub fn try_recv(&self) -> Result<DesktopCommand, TryRecvError> {
        self.commands.try_recv()
    }

    /// Call after a confirmed seek from the application to emit MPRIS Seeked.
    /// Ordinary position progress must not emit this signal.
    pub fn seeked(&self, position: f64) -> Result<(), Error> {
        if position.is_finite() && position >= 0.0 {
            *self.seeked.lock().map_err(|_| Error::State)? = Some(position);
        }
        Ok(())
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        // A session-bus handshake can be delayed by the environment. Never
        // block application shutdown waiting for that optional integration.
        if let Some(worker) = self.worker.take().filter(JoinHandle::is_finished) {
            let _ = worker.join();
        }
    }
}

#[cfg(target_os = "linux")]
fn run(
    commands: SyncSender<DesktopCommand>,
    mailbox: Arc<Mutex<Option<DesktopState>>>,
    seeked: Arc<Mutex<Option<f64>>>,
    stopping: &AtomicBool,
) -> Result<(), &'static str> {
    linux::run(commands, mailbox, seeked, stopping)
}

#[cfg(not(target_os = "linux"))]
fn run(
    _commands: SyncSender<DesktopCommand>,
    _mailbox: Arc<Mutex<Option<DesktopState>>>,
    _seeked: Arc<Mutex<Option<f64>>>,
    _stopping: &AtomicBool,
) -> Result<(), &'static str> {
    Err("Desktop media controls are currently implemented on Linux only.")
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        collections::HashMap,
        hash::{DefaultHasher, Hash, Hasher},
    };
    use zbus::{
        blocking::{Connection, connection::Builder},
        fdo,
        zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Str, Value},
    };

    const PATH: &str = "/org/mpris/MediaPlayer2";
    const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
    const NAME: &str = "org.mpris.MediaPlayer2.feishin_rs";
    type Snapshot = Arc<Mutex<DesktopState>>;
    type Metadata = HashMap<String, OwnedValue>;

    #[derive(Clone)]
    struct Shared {
        state: Snapshot,
        commands: SyncSender<DesktopCommand>,
    }
    impl Shared {
        fn read(&self) -> fdo::Result<DesktopState> {
            self.state
                .lock()
                .map(|state| state.clone())
                .map_err(|_| fdo::Error::Failed("Media state is unavailable.".into()))
        }
        fn send(&self, command: DesktopCommand) -> fdo::Result<()> {
            self.commands
                .try_send(command)
                .map_err(|error| match error {
                    mpsc::TrySendError::Full(_) => {
                        fdo::Error::LimitsExceeded("The application is busy.".into())
                    }
                    mpsc::TrySendError::Disconnected(_) => {
                        fdo::Error::Failed("The application has disconnected.".into())
                    }
                })
        }
        fn change(&self, change: impl FnOnce(&mut DesktopState)) -> fdo::Result<()> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| fdo::Error::Failed("Media state is unavailable.".into()))?;
            change(&mut state);
            Ok(())
        }
    }

    struct Root(Shared);
    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl Root {
        fn raise(&self) -> fdo::Result<()> {
            self.0.send(DesktopCommand::Raise)
        }
        fn quit(&self) -> fdo::Result<()> {
            self.0.send(DesktopCommand::Quit)
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn can_quit(&self) -> bool {
            true
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn can_raise(&self) -> bool {
            true
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn has_track_list(&self) -> bool {
            false
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn identity(&self) -> &str {
            "Feishin RS"
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn desktop_entry(&self) -> &str {
            "feishin-rs"
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn supported_uri_schemes(&self) -> Vec<&str> {
            Vec::new()
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn supported_mime_types(&self) -> Vec<&str> {
            Vec::new()
        }
    }

    struct Player(Shared);
    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Player {
        #[zbus(signal)]
        async fn seeked(
            emitter: &zbus::object_server::SignalEmitter<'_>,
            position: i64,
        ) -> zbus::Result<()>;

        fn next(&self) -> fdo::Result<()> {
            if self.0.read()?.can_next {
                self.0.send(DesktopCommand::Next)?;
            }
            Ok(())
        }
        fn previous(&self) -> fdo::Result<()> {
            if self.0.read()?.can_previous {
                self.0.send(DesktopCommand::Previous)?;
            }
            Ok(())
        }
        fn pause(&self) -> fdo::Result<()> {
            self.0.send(DesktopCommand::Pause)
        }
        fn play_pause(&self) -> fdo::Result<()> {
            self.0.send(DesktopCommand::TogglePause)
        }
        fn stop(&self) -> fdo::Result<()> {
            self.0.send(DesktopCommand::Stop)
        }
        fn play(&self) -> fdo::Result<()> {
            self.0.send(DesktopCommand::Play)
        }
        fn seek(&self, offset: i64) -> fdo::Result<()> {
            let state = self.0.read()?;
            if let Some(track) = state.track {
                let target = (state.position + offset as f64 / 1_000_000.0).max(0.0);
                if track.duration > 0.0 && target > track.duration {
                    return self.0.send(DesktopCommand::Next);
                }
                self.0.send(DesktopCommand::Seek(target))?;
            }
            Ok(())
        }
        fn set_position(&self, track_id: ObjectPath<'_>, position: i64) -> fdo::Result<()> {
            let state = self.0.read()?;
            if let Some(track) = state.track {
                let seconds = position as f64 / 1_000_000.0;
                if track_id.as_str() == track_path(&track).as_str()
                    && position >= 0
                    && (track.duration <= 0.0 || seconds <= track.duration)
                {
                    self.0.send(DesktopCommand::Seek(seconds))?;
                }
            }
            Ok(())
        }
        fn open_uri(&self, _uri: &str) -> fdo::Result<()> {
            Err(fdo::Error::NotSupported(
                "Open tracks from the application library.".into(),
            ))
        }
        #[zbus(property)]
        fn playback_status(&self) -> fdo::Result<&'static str> {
            Ok(status(&self.0.read()?))
        }
        #[zbus(property)]
        fn loop_status(&self) -> fdo::Result<&'static str> {
            Ok(loop_status(self.0.read()?.repeat))
        }
        #[zbus(property)]
        fn set_loop_status(&mut self, value: &str) -> fdo::Result<()> {
            let repeat = match value {
                "None" => Repeat::Off,
                "Track" => Repeat::One,
                "Playlist" => Repeat::All,
                _ => {
                    return Err(fdo::Error::InvalidArgs(
                        "LoopStatus must be None, Track, or Playlist.".into(),
                    ));
                }
            };
            self.0.send(DesktopCommand::Repeat(repeat))?;
            self.0.change(|state| state.repeat = repeat)
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn rate(&self) -> f64 {
            1.0
        }
        #[zbus(property)]
        fn set_rate(&mut self, value: f64) -> fdo::Result<()> {
            if value == 1.0 {
                Ok(())
            } else {
                Err(fdo::Error::NotSupported(
                    "Only normal playback speed is supported.".into(),
                ))
            }
        }
        #[zbus(property)]
        fn shuffle(&self) -> fdo::Result<bool> {
            Ok(self.0.read()?.shuffle)
        }
        #[zbus(property)]
        fn set_shuffle(&mut self, value: bool) -> fdo::Result<()> {
            self.0.send(DesktopCommand::Shuffle(value))?;
            self.0.change(|state| state.shuffle = value)
        }
        #[zbus(property)]
        fn metadata(&self) -> fdo::Result<Metadata> {
            metadata(self.0.read()?.track.as_ref())
        }
        #[zbus(property)]
        fn volume(&self) -> fdo::Result<f64> {
            Ok(safe_volume(self.0.read()?.volume) / 100.0)
        }
        #[zbus(property)]
        fn set_volume(&mut self, value: f64) -> fdo::Result<()> {
            if !value.is_finite() {
                return Err(fdo::Error::InvalidArgs("Volume must be finite.".into()));
            }
            let volume = (value * 100.0).clamp(0.0, 200.0);
            self.0.send(DesktopCommand::Volume(volume))?;
            self.0.change(|state| state.volume = volume)
        }
        #[zbus(property(emits_changed_signal = "false"))]
        fn position(&self) -> fdo::Result<i64> {
            Ok(microseconds(self.0.read()?.position))
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn minimum_rate(&self) -> f64 {
            1.0
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn maximum_rate(&self) -> f64 {
            1.0
        }
        #[zbus(property)]
        fn can_go_next(&self) -> fdo::Result<bool> {
            Ok(self.0.read()?.can_next)
        }
        #[zbus(property)]
        fn can_go_previous(&self) -> fdo::Result<bool> {
            Ok(self.0.read()?.can_previous)
        }
        #[zbus(property)]
        fn can_play(&self) -> fdo::Result<bool> {
            Ok(self.0.read()?.track.is_some())
        }
        #[zbus(property)]
        fn can_pause(&self) -> fdo::Result<bool> {
            Ok(self.0.read()?.track.is_some())
        }
        #[zbus(property)]
        fn can_seek(&self) -> fdo::Result<bool> {
            Ok(self
                .0
                .read()?
                .track
                .is_some_and(|track| track.duration > 0.0))
        }
        #[zbus(property(emits_changed_signal = "const"))]
        fn can_control(&self) -> bool {
            true
        }
    }

    fn status(state: &DesktopState) -> &'static str {
        if state.track.is_none() {
            "Stopped"
        } else if state.paused {
            "Paused"
        } else {
            "Playing"
        }
    }
    fn loop_status(repeat: Repeat) -> &'static str {
        match repeat {
            Repeat::Off => "None",
            Repeat::One => "Track",
            Repeat::All => "Playlist",
        }
    }
    fn microseconds(seconds: f64) -> i64 {
        if seconds.is_finite() && seconds >= 0.0 {
            (seconds.min(i64::MAX as f64 / 1_000_000.0) * 1_000_000.0) as i64
        } else {
            0
        }
    }
    fn safe_volume(volume: f64) -> f64 {
        if volume.is_finite() {
            volume.clamp(0.0, 200.0)
        } else {
            0.0
        }
    }
    fn track_path(track: &Track) -> String {
        let mut hash = DefaultHasher::new();
        (&track.id, &track.album_id).hash(&mut hash);
        format!("/io/feishin_rs/track/t{:016x}", hash.finish())
    }
    fn metadata(track: Option<&Track>) -> fdo::Result<Metadata> {
        let mut data = Metadata::new();
        if let Some(track) = track {
            let path = OwnedObjectPath::try_from(track_path(track))
                .map_err(|_| fdo::Error::Failed("Track identity is unavailable.".into()))?;
            data.insert("mpris:trackid".into(), ObjectPath::from(path).into());
            data.insert("mpris:length".into(), microseconds(track.duration).into());
            data.insert("xesam:title".into(), Str::from(track.title.clone()).into());
            data.insert("xesam:album".into(), Str::from(track.album.clone()).into());
            data.insert(
                "xesam:trackNumber".into(),
                i32::try_from(track.number).unwrap_or(i32::MAX).into(),
            );
            let artists = Value::from(vec![track.artist.as_str()])
                .try_to_owned()
                .map_err(|_| fdo::Error::Failed("Artist metadata is unavailable.".into()))?;
            data.insert("xesam:artist".into(), artists);
        }
        Ok(data)
    }

    pub(super) fn run(
        commands: SyncSender<DesktopCommand>,
        mailbox: Arc<Mutex<Option<DesktopState>>>,
        seeked: Arc<Mutex<Option<f64>>>,
        stopping: &AtomicBool,
    ) -> Result<(), &'static str> {
        let state = Arc::new(Mutex::new(DesktopState::default()));
        let shared = Shared {
            state: Arc::clone(&state),
            commands,
        };
        let connection = Builder::session().map_err(|_| "Linux desktop controls are unavailable: no session D-Bus connection.")?
            .method_timeout(Duration::from_secs(3))
            .serve_at(PATH, Root(shared.clone())).map_err(|_| "Could not register the MPRIS application interface.")?
            .serve_at(PATH, Player(shared)).map_err(|_| "Could not register the MPRIS player interface.")?
            .name(NAME).map_err(|_| "Could not configure the MPRIS bus name.")?
            .build().map_err(|_| "Linux desktop controls could not connect to the session bus or acquire the MPRIS name.")?;
        while !stopping.load(Ordering::Relaxed) {
            if connection.is_closed() {
                return Err("The Linux desktop media-control session bus disconnected.");
            }
            let next = mailbox
                .lock()
                .map_err(|_| "Desktop state became unavailable.")?
                .take();
            if let Some(mut next) = next {
                next.volume = safe_volume(next.volume);
                if !next.position.is_finite() || next.position < 0.0 {
                    next.position = 0.0;
                }
                let old = {
                    let mut current = state
                        .lock()
                        .map_err(|_| "Desktop state became unavailable.")?;
                    std::mem::replace(&mut *current, next.clone())
                };
                publish(&connection, &old, &next)
                    .map_err(|_| "The Linux desktop media-control connection failed.")?;
            }
            if let Some(position) = seeked
                .lock()
                .map_err(|_| "Desktop seek state became unavailable.")?
                .take()
            {
                connection
                    .emit_signal(
                        None::<&str>,
                        PATH,
                        PLAYER,
                        "Seeked",
                        &(microseconds(position),),
                    )
                    .map_err(|_| "Could not publish the desktop seek position.")?;
            }
            thread::sleep(Duration::from_millis(40));
        }
        Ok(())
    }

    fn publish(
        connection: &Connection,
        old: &DesktopState,
        next: &DesktopState,
    ) -> zbus::Result<()> {
        let mut changed: HashMap<&str, OwnedValue> = HashMap::new();
        if status(old) != status(next) {
            changed.insert("PlaybackStatus", Str::from(status(next)).into());
        }
        if old.repeat != next.repeat {
            changed.insert("LoopStatus", Str::from(loop_status(next.repeat)).into());
        }
        if old.shuffle != next.shuffle {
            changed.insert("Shuffle", next.shuffle.into());
        }
        if old.volume != next.volume {
            changed.insert("Volume", (next.volume / 100.0).into());
        }
        if old.can_next != next.can_next {
            changed.insert("CanGoNext", next.can_next.into());
        }
        if old.can_previous != next.can_previous {
            changed.insert("CanGoPrevious", next.can_previous.into());
        }
        if old.track != next.track {
            let data = metadata(next.track.as_ref()).map_err(zbus::Error::from)?;
            let value = Value::from(data)
                .try_to_owned()
                .map_err(zbus::Error::from)?;
            changed.insert("Metadata", value);
            changed.insert("CanPlay", next.track.is_some().into());
            changed.insert("CanPause", next.track.is_some().into());
            changed.insert(
                "CanSeek",
                next.track
                    .as_ref()
                    .is_some_and(|track| track.duration > 0.0)
                    .into(),
            );
        }
        if !changed.is_empty() {
            connection.emit_signal(
                None::<&str>,
                PATH,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(PLAYER, changed, Vec::<String>::new()),
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn invalid_numbers_cannot_escape_to_mpris() {
            assert_eq!(microseconds(f64::NAN), 0);
            assert_eq!(microseconds(-1.0), 0);
            assert_eq!(microseconds(1.5), 1_500_000);
            assert_eq!(safe_volume(f64::INFINITY), 0.0);
            assert_eq!(safe_volume(120.0), 120.0);
        }
        #[test]
        fn track_paths_are_valid_and_do_not_expose_server_ids() {
            let track = Track {
                id: "server/path?token=private".into(),
                title: "Track".into(),
                ..Track::default()
            };
            let path = track_path(&track);
            assert!(!path.contains("private"));
            assert!(OwnedObjectPath::try_from(path.clone()).is_ok());
            assert_eq!(track_path(&track), path);
            assert_eq!(metadata(Some(&track)).unwrap().len(), 6);
        }
    }
}
