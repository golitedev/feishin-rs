//! An application-owned, external MPV process controlled through JSON IPC.
//!
//! Process startup, IPC and shutdown run on a worker thread. This crate never
//! renders audio and never falls back to another playback engine.
use feishin_core::playback::{AudioDevice, AudioStats, EQ_FREQUENCIES, PlaybackSettings};
#[cfg(any(unix, windows))]
mod transport;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub executable: String,
    /// Trusted application configuration, not data received from a music server.
    pub extra_args: Vec<String>,
    pub startup_timeout: Duration,
    pub command_timeout: Duration,
    pub settings: PlaybackSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            executable: "mpv".into(),
            extra_args: Vec::new(),
            startup_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(5),
            settings: PlaybackSettings::default(),
        }
    }
}

/// No Debug implementation: Load can contain authenticated URLs and headers.
pub enum Command {
    Load {
        generation: u64,
        paused: bool,
        url: String,
        headers: Vec<(String, String)>,
    },
    /// Replaces any previously prepared successor, leaving the current source
    /// playing. MPV owns the gapless output-device transition.
    PrepareNext {
        current_generation: u64,
        generation: u64,
        url: String,
        headers: Vec<(String, String)>,
    },
    ClearNext,
    ApplySettings(PlaybackSettings),
    ListAudioDevices,
    EnableAnalysis(bool),
    Pause(bool),
    Seek(f64),
    Volume(f64),
    Stop,
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    Eof,
    Stop,
    Quit,
    Error,
    Redirect,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Ready,
    Pause(bool),
    Position(f64),
    Duration(f64),
    Volume(f64),
    /// Actual MPV gain, including optional pause fades; never the user's volume.
    AudioGain(f64),
    Buffering(bool),
    AudioDevices(Vec<AudioDevice>),
    AudioStats(AudioStats),
    Speed(f64),
    PreparedNext(u64),
    AudioLevels {
        left_rms_db: f64,
        right_rms_db: f64,
        left_peak_db: f64,
        right_peak_db: f64,
    },
    FileLoaded(u64),
    EndFile {
        generation: u64,
        reason: EndReason,
    },
    /// Messages contain only fixed application text, never MPV diagnostics.
    Error(String),
    Disconnected,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not start the MPV control worker.")]
    Worker,
    #[error("MPV is not connected. Check the executable path and reconnect.")]
    Disconnected,
    #[error("MPV is busy. Please try the command again.")]
    Busy,
}

pub struct Player {
    commands: SyncSender<Command>,
    events: Receiver<Event>,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Player {
    /// Returns immediately; readiness and startup errors arrive as events.
    pub fn spawn(config: Config) -> Result<Self, Error> {
        let (commands, command_rx) = mpsc::sync_channel(128);
        let (event_tx, events) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stopping);
        let worker = thread::Builder::new()
            .name("feishin-mpv".into())
            .spawn(move || {
                if let Err(message) = run(config, command_rx, &event_tx, &stopped) {
                    let _ = event_tx.send(Event::Error(message.into()));
                }
                let _ = event_tx.send(Event::Disconnected);
            })
            .map_err(|_| Error::Worker)?;
        Ok(Self {
            commands,
            events,
            stopping,
            worker: Some(worker),
        })
    }

    /// Never waits for socket I/O or a command reply.
    pub fn send(&self, command: Command) -> Result<(), Error> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => Error::Busy,
                mpsc::TrySendError::Disconnected(_) => Error::Disconnected,
            })
    }

    pub fn try_recv(&self) -> Result<Event, TryRecvError> {
        self.events.try_recv()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(any(unix, windows))]
fn run(
    config: Config,
    commands: Receiver<Command>,
    events: &mpsc::Sender<Event>,
    stopping: &AtomicBool,
) -> Result<(), &'static str> {
    backend::run(config, commands, events, stopping)
}

#[cfg(not(any(unix, windows)))]
fn run(
    _config: Config,
    _commands: Receiver<Command>,
    _events: &mpsc::Sender<Event>,
    _stopping: &AtomicBool,
) -> Result<(), &'static str> {
    Err("MPV IPC is not implemented for this operating system.")
}

#[cfg(any(unix, windows))]
mod backend {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        collections::{HashMap, HashSet},
        io::{Read, Write},
        process::{Child, Command as ProcessCommand, Stdio},
        time::Instant,
    };

    const MAX_MESSAGE: usize = 1024 * 1024;
    const POLL: Duration = Duration::from_millis(20);

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    const PAUSE_FADE_DURATION: Duration = Duration::from_millis(150);

    struct FadeTransition {
        from: f64,
        target: f64,
        began: Instant,
        pause_after: bool,
    }

    struct PauseFade {
        level: f64,
        transition: Option<FadeTransition>,
    }

    impl Default for PauseFade {
        fn default() -> Self {
            Self {
                level: 1.0,
                transition: None,
            }
        }
    }

    impl PauseFade {
        fn start(&mut self, pause: bool, observed_paused: bool, now: Instant) {
            if !pause && observed_paused && self.transition.is_none() {
                self.level = 0.0;
            }
            self.transition = Some(FadeTransition {
                from: self.level,
                target: if pause { 0.0 } else { 1.0 },
                began: now,
                pause_after: pause,
            });
        }

        fn gain_db(&self, preamp_db: f64) -> f64 {
            preamp_db + 20.0 * self.level.max(0.001).log10()
        }

        /// Return whether the completed transition must pause MPV.
        fn advance(&mut self, now: Instant) -> Option<bool> {
            let transition = self.transition.as_ref()?;
            let progress = (now
                .saturating_duration_since(transition.began)
                .as_secs_f64()
                / PAUSE_FADE_DURATION.as_secs_f64())
            .clamp(0.0, 1.0);
            self.level = transition.from + (transition.target - transition.from) * progress;
            if progress >= 1.0 {
                let pause_after = transition.pause_after;
                self.transition = None;
                Some(pause_after)
            } else {
                Some(false)
            }
        }

        fn reset(&mut self) -> bool {
            let changed = self.level != 1.0 || self.transition.is_some();
            *self = Self::default();
            changed
        }
    }

    pub(super) fn run(
        config: Config,
        commands: Receiver<Command>,
        events: &mpsc::Sender<Event>,
        stopping: &AtomicBool,
    ) -> Result<(), &'static str> {
        let endpoint = transport::Endpoint::new()?;
        // Set application-owned options last so even a user configuration cannot
        // replace the IPC endpoint or make this an interactive video player.
        let child = ProcessCommand::new(&config.executable)
            .args(&config.extra_args)
            .args([
                "--no-config",
                "--idle=yes",
                "--no-video",
                "--no-terminal",
                "--input-terminal=no",
                "--input-default-bindings=no",
                "--no-osc",
                "--load-scripts=no",
                "--ytdl=no",
                "--audio-display=no",
                "--loop-file=no",
                "--loop-playlist=no",
                "--network-timeout=15",
                "--tls-verify=yes",
            ])
            .arg(format!("--input-ipc-server={}", endpoint.name()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(
                |_| "Could not launch MPV. Install MPV or choose its executable in Settings.",
            )?;
        let mut child = OwnedChild(child);
        let started = Instant::now();
        let mut stream = loop {
            if stopping.load(Ordering::Relaxed) {
                return Ok(());
            }
            if child
                .0
                .try_wait()
                .map_err(|_| "Could not check the MPV process.")?
                .is_some()
            {
                return Err(
                    "MPV exited before connecting. Check the executable and supported options.",
                );
            }
            if let Ok(stream) = transport::connect(&endpoint) {
                break stream;
            }
            if started.elapsed() >= config.startup_timeout {
                return Err("MPV did not create its IPC socket before the startup timeout.");
            }
            thread::sleep(POLL);
        };
        let mut protocol = Protocol::new(config.command_timeout);
        let mut settings = config.settings.normalized();
        let mut analysis = false;
        let mut fade = PauseFade::default();
        let mut observed_paused = true;
        let mut last_analysis = Instant::now();
        apply_settings(&mut protocol, &mut stream, None, &settings, analysis)?;
        for (id, name) in [
            "pause",
            "time-pos",
            "duration",
            "volume",
            "volume-gain",
            "paused-for-cache",
            "speed",
            "audio-device-list",
            "audio-params",
            "audio-out-params",
            "current-tracks/audio/codec",
            "audio-bitrate",
        ]
        .iter()
        .enumerate()
        {
            protocol.send(
                &mut stream,
                json!(["observe_property", id + 1, name]),
                "subscribe to playback state",
            )?;
        }
        let _ = events.send(Event::Ready);
        let mut decoder = Decoder::default();
        let mut bytes = [0_u8; 16384];
        loop {
            if stopping.load(Ordering::Relaxed) {
                break;
            }
            for _ in 0..32 {
                match commands.try_recv() {
                    Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => return Ok(()),
                    Ok(Command::ApplySettings(next)) => {
                        let next = next.normalized();
                        if !next.fade_on_pause {
                            if fade
                                .transition
                                .as_ref()
                                .is_some_and(|fade| fade.pause_after)
                            {
                                protocol.send(
                                    &mut stream,
                                    json!(["set_property", "pause", true]),
                                    "change pause state",
                                )?;
                            }
                            if fade.reset() {
                                protocol.send(
                                    &mut stream,
                                    json!(["set_property", "volume-gain", next.preamp_db]),
                                    "restore playback gain",
                                )?;
                            }
                        }
                        apply_settings(
                            &mut protocol,
                            &mut stream,
                            Some(&settings),
                            &next,
                            analysis,
                        )?;
                        if settings.gapless && !next.gapless {
                            protocol.cancel_prepared();
                            protocol.send(
                                &mut stream,
                                json!(["playlist-clear"]),
                                "clear the prepared track",
                            )?;
                        }
                        settings = next;
                    }
                    Ok(Command::Pause(paused))
                        if settings.fade_on_pause && protocol.active_entry.is_some() =>
                    {
                        fade.start(paused, observed_paused, Instant::now());
                        if !paused {
                            // Set the initial quiet gain before resuming audio.
                            protocol.send(
                                &mut stream,
                                json!([
                                    "set_property",
                                    "volume-gain",
                                    fade.gain_db(settings.preamp_db)
                                ]),
                                "fade playback gain",
                            )?;
                            protocol.send(
                                &mut stream,
                                json!(["set_property", "pause", false]),
                                "change pause state",
                            )?;
                        }
                    }
                    Ok(Command::EnableAnalysis(enabled)) => {
                        if enabled != analysis {
                            analysis = enabled;
                            protocol.analysis_enabled = analysis;
                            protocol.send(
                                &mut stream,
                                json!(["set_property", "af", audio_filters(&settings, analysis)]),
                                "configure audio analysis",
                            )?;
                        }
                    }
                    Ok(Command::ListAudioDevices) => {
                        let id = protocol.send(
                            &mut stream,
                            json!(["get_property", "audio-device-list"]),
                            "discover audio devices",
                        )?;
                        protocol.device_requests.insert(id);
                    }
                    Ok(Command::PrepareNext {
                        current_generation,
                        generation,
                        url,
                        headers,
                    }) => {
                        // A delayed server response must never append a track to
                        // the playlist of a newer user selection.
                        if settings.gapless && protocol.can_prepare(current_generation) {
                            match encode_load(url, headers, false, "append-play") {
                                Ok(value) => {
                                    protocol.send(
                                        &mut stream,
                                        json!(["playlist-clear"]),
                                        "clear the prepared track",
                                    )?;
                                    let id = protocol.send(
                                        &mut stream,
                                        value,
                                        "prepare the next track",
                                    )?;
                                    protocol.loading.insert(id, generation);
                                    protocol.preparing.insert(id);
                                    protocol.prepared_transition =
                                        Some((current_generation, generation));
                                }
                                Err(message) => {
                                    let _ = events.send(Event::Error(message.into()));
                                }
                            }
                        }
                    }
                    Ok(command) => {
                        if matches!(command, Command::Load { .. } | Command::Stop) && fade.reset() {
                            protocol.send(
                                &mut stream,
                                json!(["set_property", "volume-gain", settings.preamp_db]),
                                "restore playback gain",
                            )?;
                        }
                        if matches!(command, Command::ClearNext) {
                            protocol.cancel_prepared();
                        }
                        if matches!(command, Command::Stop) {
                            protocol.prepared_transition = None;
                            protocol.desired_generation = None;
                        }
                        let command = match command {
                            Command::Volume(value) if value.is_finite() => {
                                Command::Volume(value.clamp(0.0, settings.volume_max))
                            }
                            other => other,
                        };
                        let generation = match &command {
                            Command::Load { generation, .. } => Some(*generation),
                            _ => None,
                        };
                        match encode_command(command) {
                            Ok((value, label)) => {
                                let id = protocol.send(&mut stream, value, label)?;
                                if let Some(generation) = generation {
                                    protocol.loading.insert(id, generation);
                                    protocol.replacing.insert(id);
                                    protocol.desired_generation = Some(generation);
                                    protocol.prepared_transition = None;
                                }
                            }
                            Err(message) => {
                                let _ = events.send(Event::Error(message.into()));
                            }
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            if let Some(pause_after) = fade.advance(Instant::now()) {
                protocol.send(
                    &mut stream,
                    json!([
                        "set_property",
                        "volume-gain",
                        fade.gain_db(settings.preamp_db)
                    ]),
                    "fade playback gain",
                )?;
                if pause_after {
                    protocol.send(
                        &mut stream,
                        json!(["set_property", "pause", true]),
                        "change pause state",
                    )?;
                }
            }
            if analysis
                && protocol.active_entry.is_some()
                && last_analysis.elapsed() >= Duration::from_millis(50)
            {
                let id = protocol.send(
                    &mut stream,
                    json!(["get_property", "af-metadata/feishin_levels"]),
                    "read audio levels",
                )?;
                protocol.analysis_requests.insert(id);
                last_analysis = Instant::now();
            }
            match stream.read(&mut bytes) {
                Ok(0) => {
                    return Err("MPV closed its IPC connection. Reconnect the player to continue.");
                }
                Ok(count) => {
                    for message in decoder.push(bytes.get(..count).unwrap_or_default())? {
                        for event in protocol.receive(message)? {
                            if let Event::Pause(paused) = &event {
                                observed_paused = *paused;
                            }
                            let _ = events.send(event);
                        }
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => {
                    return Err("The MPV IPC connection failed. Reconnect the player to continue.");
                }
            }
            if protocol.expired() {
                return Err(
                    "MPV did not acknowledge a command before the timeout. Reconnect the player to continue.",
                );
            }
            if child
                .0
                .try_wait()
                .map_err(|_| "Could not check the MPV process.")?
                .is_some()
            {
                return Err("The MPV process exited. Reconnect the player to continue.");
            }
        }
        Ok(())
    }

    /// MPV string lists escape separator commas with a backslash. Other
    /// backslashes remain literal; doubling them changes the HTTP header.
    /// The loadfile options map removes one outer parsing layer, but the
    /// http-header-fields string-list parser still runs. Never log this string.
    fn header_options(headers: Vec<(String, String)>) -> Result<String, &'static str> {
        if headers.len() > 64 {
            return Err("Too many authentication headers for MPV.");
        }
        let mut encoded = Vec::new();
        for (name, value) in headers {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                || value.bytes().any(|b| b < 0x20 || b == 0x7f)
                || value.len() > 16384
            {
                return Err("The stream supplied an invalid HTTP header.");
            }
            let field = format!("{name}: {value}");
            encoded.push(field.replace(',', "\\,"));
        }
        Ok(encoded.join(","))
    }

    fn encode_command(command: Command) -> Result<(Value, &'static str), &'static str> {
        Ok(match command {
            Command::Load {
                url,
                headers,
                paused,
                ..
            } => (
                encode_load(url, headers, paused, "replace")?,
                "load the selected track",
            ),
            Command::Pause(value) => (
                json!(["set_property", "pause", value]),
                "change pause state",
            ),
            Command::Seek(value) if value.is_finite() && value >= 0.0 => {
                (json!(["seek", value, "absolute"]), "seek")
            }
            Command::Volume(value) if value.is_finite() => (
                json!(["set_property", "volume", value.clamp(0.0, 200.0)]),
                "change volume",
            ),
            Command::Stop => (json!(["stop"]), "stop playback"),
            Command::ClearNext => (json!(["playlist-clear"]), "clear the prepared track"),
            Command::Shutdown => (json!(["quit"]), "quit"),
            _ => return Err("The playback command contained an invalid number."),
        })
    }

    fn encode_load(
        url: String,
        headers: Vec<(String, String)>,
        paused: bool,
        flags: &str,
    ) -> Result<Value, &'static str> {
        if url.is_empty() || url.len() > 65536 || url.chars().any(char::is_control) {
            return Err("The stream source is invalid.");
        }
        // Named arguments avoid the MPV 0.38 positional index/options break.
        Ok(
            json!({"name":"loadfile", "url":url, "flags":flags, "options":{
                "http-header-fields":header_options(headers)?, "pause":if paused {"yes"} else {"no"}
            }}),
        )
    }

    fn audio_filters(settings: &PlaybackSettings, analysis: bool) -> String {
        let mut graph = Vec::new();
        if settings.equalizer.enabled {
            for (frequency, gain) in EQ_FREQUENCIES.iter().zip(settings.equalizer.bands_db) {
                graph.push(format!("equalizer=f={frequency}:t=o:w=1:g={gain}"));
            }
        }
        if settings.normalization {
            graph.push("dynaudnorm=f=250:g=31:p=0.95:m=10".into());
        }
        let mut filters = if graph.is_empty() {
            String::new()
        } else {
            format!("lavfi=[{}]", graph.join(","))
        };
        if analysis {
            if !filters.is_empty() {
                filters.push(',');
            }
            // Native lavfi-bridge labels expose astats frame metadata through
            // af-metadata. Keep this separate from the EQ/normalization graph.
            filters.push_str("@feishin_levels:astats=metadata=1:reset=1:measure_overall=none:measure_perchannel=RMS_level+Peak_level");
        }
        filters
    }

    fn settings_properties(
        settings: &PlaybackSettings,
        analysis: bool,
    ) -> Vec<(&'static str, Value)> {
        vec![
            ("volume-max", json!(settings.volume_max)),
            ("speed", json!(settings.speed)),
            ("audio-pitch-correction", json!(settings.preserve_pitch)),
            ("audio-device", json!(settings.audio_device)),
            ("audio-exclusive", json!(settings.exclusive)),
            ("audio-samplerate", json!(settings.sample_rate.unwrap_or(0))),
            ("audio-channels", json!(settings.channels.mpv_value())),
            ("replaygain", json!(settings.replay_gain.mpv_value())),
            ("replaygain-preamp", json!(settings.replay_gain_preamp)),
            ("replaygain-fallback", json!(settings.replay_gain_fallback)),
            ("replaygain-clip", json!(settings.replay_gain_clip)),
            ("volume-gain", json!(settings.preamp_db)),
            (
                "gapless-audio",
                json!(if !settings.gapless {
                    "no"
                } else if settings.gapless_weak {
                    "weak"
                } else {
                    "yes"
                }),
            ),
            // MPV's playlist prefetch opens the successor with the current
            // file's HTTP options, before its loadfile options are applied.
            // That can send current authentication to an unrelated source.
            // Preparing an entry still avoids an application-level reload;
            // leave URL prefetch disabled to preserve per-source isolation.
            ("prefetch-playlist", json!(false)),
            ("af", json!(audio_filters(settings, analysis))),
        ]
    }

    fn apply_settings(
        protocol: &mut Protocol,
        stream: &mut impl Write,
        previous: Option<&PlaybackSettings>,
        next: &PlaybackSettings,
        analysis: bool,
    ) -> Result<(), &'static str> {
        let previous: HashMap<_, _> = previous
            .map(|settings| settings_properties(settings, analysis))
            .unwrap_or_default()
            .into_iter()
            .collect();
        for (name, value) in settings_properties(next, analysis) {
            if previous.get(name) != Some(&value) {
                protocol.send(
                    stream,
                    json!(["set_property", name, value]),
                    "apply audio settings",
                )?;
            }
        }
        Ok(())
    }

    #[derive(Default)]
    struct Decoder {
        partial: Vec<u8>,
    }
    impl Decoder {
        fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>, &'static str> {
            let mut messages = Vec::new();
            for byte in bytes {
                if *byte == b'\n' {
                    if !self.partial.is_empty() {
                        let text = String::from_utf8_lossy(&self.partial);
                        let value = serde_json::from_str(&text)
                            .map_err(|_| "MPV sent an invalid IPC message.")?;
                        messages.push(value);
                        self.partial.clear();
                    }
                } else {
                    if self.partial.len() >= MAX_MESSAGE {
                        return Err("MPV sent an IPC message larger than the allowed limit.");
                    }
                    self.partial.push(*byte);
                }
            }
            Ok(messages)
        }
    }

    #[derive(Clone, Copy)]
    enum TrackEvent {
        Loaded,
        End(EndReason),
    }

    struct Protocol {
        next_id: i64,
        timeout: Duration,
        pending: HashMap<i64, (Instant, &'static str)>,
        // request ID -> caller generation, then MPV entry ID -> generation.
        loading: HashMap<i64, u64>,
        preparing: HashSet<i64>,
        replacing: HashSet<i64>,
        device_requests: HashSet<i64>,
        analysis_requests: HashSet<i64>,
        entries: HashMap<i64, u64>,
        active_entry: Option<i64>,
        deferred: Vec<(i64, TrackEvent)>,
        audio_stats: AudioStats,
        analysis_enabled: bool,
        desired_generation: Option<u64>,
        prepared_transition: Option<(u64, u64)>,
    }
    impl Protocol {
        fn new(timeout: Duration) -> Self {
            Self {
                next_id: 1,
                timeout,
                pending: HashMap::new(),
                loading: HashMap::new(),
                preparing: HashSet::new(),
                replacing: HashSet::new(),
                device_requests: HashSet::new(),
                analysis_requests: HashSet::new(),
                entries: HashMap::new(),
                active_entry: None,
                deferred: Vec::new(),
                audio_stats: AudioStats::default(),
                analysis_enabled: false,
                desired_generation: None,
                prepared_transition: None,
            }
        }

        fn send(
            &mut self,
            stream: &mut impl Write,
            command: Value,
            label: &'static str,
        ) -> Result<i64, &'static str> {
            if self.pending.len() >= 256 {
                return Err("MPV has too many unanswered commands. Reconnect the player.");
            }
            let id = self.next_id;
            self.next_id = self
                .next_id
                .checked_add(1)
                .ok_or("The MPV request counter was exhausted. Reconnect the player.")?;
            let bytes = serde_json::to_vec(&json!({"command":command,"request_id":id}))
                .map_err(|_| "Could not encode the MPV command.")?;
            stream
                .write_all(&bytes)
                .and_then(|_| stream.write_all(b"\n"))
                .map_err(|_| "Could not send a command to MPV.")?;
            self.pending.insert(id, (Instant::now(), label));
            Ok(id)
        }

        fn expired(&self) -> bool {
            self.pending
                .values()
                .any(|(sent, _)| sent.elapsed() >= self.timeout)
        }

        fn active_generation(&self) -> Option<u64> {
            self.active_entry
                .and_then(|entry| self.entries.get(&entry).copied())
        }

        fn can_prepare(&self, current: u64) -> bool {
            self.replacing.is_empty()
                && self.active_generation() == Some(current)
                && self.desired_generation == Some(current)
        }

        fn cancel_prepared(&mut self) {
            // playlist-clear cannot rewind a successor which MPV has already
            // started. Keep any late FileLoaded tagged for the caller, but do
            // not let a cancelled transition become the desired generation.
            self.prepared_transition = None;
        }

        fn track_event(
            &mut self,
            entry: i64,
            kind: TrackEvent,
        ) -> Result<Option<Event>, &'static str> {
            let Some(generation) = self.entries.get(&entry).copied() else {
                // A new file may start before its loadfile acknowledgement is
                // read. Never attribute it to the most recently selected track.
                if !self.loading.is_empty() {
                    if self.deferred.len() >= 256 {
                        return Err("MPV produced too many uncorrelated playback events.");
                    }
                    self.deferred.push((entry, kind));
                }
                return Ok(None);
            };
            Ok(Some(match kind {
                TrackEvent::Loaded => {
                    if let Some((source, target)) = self.prepared_transition
                        && generation == target
                        && self.desired_generation == Some(source)
                    {
                        self.desired_generation = Some(target);
                        self.prepared_transition = None;
                    }
                    Event::FileLoaded(generation)
                }
                TrackEvent::End(reason) => {
                    self.entries.remove(&entry);
                    Event::EndFile { generation, reason }
                }
            }))
        }

        fn receive(&mut self, message: Value) -> Result<Vec<Event>, &'static str> {
            let mut events = Vec::new();
            if let Some(id) = message.get("request_id").and_then(Value::as_i64) {
                if let Some((_, label)) = self.pending.remove(&id) {
                    let generation = self.loading.remove(&id);
                    let preparing = self.preparing.remove(&id);
                    self.replacing.remove(&id);
                    let devices = self.device_requests.remove(&id);
                    let analysis = self.analysis_requests.remove(&id);
                    if analysis {
                        // Metadata is temporarily unavailable during load,
                        // pause/seek and teardown. Never invent replacement data.
                        if self.analysis_enabled
                            && message.get("error").and_then(Value::as_str) == Some("success")
                        {
                            events
                                .extend(audio_levels(message.get("data").unwrap_or(&Value::Null)));
                        }
                        return Ok(events);
                    }
                    if message.get("error").and_then(Value::as_str) != Some("success") {
                        events.push(Event::Error(format!("MPV could not {label}.")));
                    } else if let Some(generation) = generation {
                        let entry = message.get("data").and_then(|data| data.get("playlist_entry_id")).and_then(Value::as_i64)
                            .ok_or("This MPV build does not return playback entry IDs. Use MPV 0.41 or newer.")?;
                        // Rapid replacements may remove accepted playlist entries
                        // before they ever start, so they have no end-file event.
                        // Bound this bookkeeping without interrupting playback.
                        // Preserve the active entry; obsolete unstarted entries
                        // cannot advance the caller's newer generation anyway.
                        if self.entries.len() >= 64 {
                            self.entries
                                .retain(|entry, _| Some(*entry) == self.active_entry);
                        }
                        self.entries.insert(entry, generation);
                        if preparing {
                            events.push(Event::PreparedNext(generation));
                        }
                        let mut deferred = std::mem::take(&mut self.deferred);
                        for (event_entry, kind) in deferred.drain(..) {
                            if let Some(event) = self.track_event(event_entry, kind)? {
                                events.push(event);
                            }
                        }
                    } else if devices {
                        events.push(Event::AudioDevices(audio_devices(
                            message.get("data").unwrap_or(&Value::Null),
                        )));
                    }
                }
                return Ok(events);
            }
            match message.get("event").and_then(Value::as_str) {
                Some("property-change") => {
                    let data = message.get("data").unwrap_or(&Value::Null);
                    let event = match message.get("name").and_then(Value::as_str) {
                        Some("pause") => data.as_bool().map(Event::Pause),
                        Some("time-pos") => finite_nonnegative(data).map(Event::Position),
                        Some("duration") => finite_nonnegative(data).map(Event::Duration),
                        Some("volume") => finite_nonnegative(data).map(Event::Volume),
                        Some("volume-gain") => data
                            .as_f64()
                            .filter(|value| value.is_finite())
                            .map(Event::AudioGain),
                        Some("paused-for-cache") => data.as_bool().map(Event::Buffering),
                        Some("speed") => finite_nonnegative(data).map(Event::Speed),
                        Some("audio-device-list") => Some(Event::AudioDevices(audio_devices(data))),
                        _ => None,
                    };
                    events.extend(event);
                    if let Some(name) = message.get("name").and_then(Value::as_str)
                        && update_audio_stats(&mut self.audio_stats, name, data)
                    {
                        events.push(Event::AudioStats(self.audio_stats.clone()));
                    }
                }
                Some("start-file") => {
                    self.active_entry = message.get("playlist_entry_id").and_then(Value::as_i64);
                }
                Some("file-loaded") => {
                    if let Some(entry) = self.active_entry {
                        events.extend(self.track_event(entry, TrackEvent::Loaded)?);
                    }
                }
                Some("end-file") => {
                    let reason = match message.get("reason").and_then(Value::as_str) {
                        Some("eof") => EndReason::Eof,
                        Some("stop") => EndReason::Stop,
                        Some("quit") => EndReason::Quit,
                        Some("error") => EndReason::Error,
                        Some("redirect") => EndReason::Redirect,
                        _ => EndReason::Unknown,
                    };
                    if let Some(entry) = message.get("playlist_entry_id").and_then(Value::as_i64) {
                        events.extend(self.track_event(entry, TrackEvent::End(reason))?);
                        if self.active_entry == Some(entry) {
                            self.active_entry = None;
                        }
                    }
                }
                Some("shutdown") => events.push(Event::Disconnected),
                _ => {}
            }
            Ok(events)
        }
    }

    fn audio_levels(data: &Value) -> Option<Event> {
        fn db(data: &Value, key: &str) -> Option<f64> {
            let value = data.get(key)?;
            let value = value
                .as_f64()
                .or_else(|| value.as_str()?.parse::<f64>().ok())?;
            if value.is_nan() {
                None
            } else {
                Some(value.clamp(-120.0, 24.0))
            }
        }
        let left_rms_db = db(data, "lavfi.astats.1.RMS_level")?;
        let left_peak_db = db(data, "lavfi.astats.1.Peak_level")?;
        // Mono sources have no second channel; display their measured level on
        // both meters instead of fabricating a nonexistent stereo channel.
        let right_rms_db = db(data, "lavfi.astats.2.RMS_level").unwrap_or(left_rms_db);
        let right_peak_db = db(data, "lavfi.astats.2.Peak_level").unwrap_or(left_peak_db);
        Some(Event::AudioLevels {
            left_rms_db,
            right_rms_db,
            left_peak_db,
            right_peak_db,
        })
    }

    fn audio_devices(data: &Value) -> Vec<AudioDevice> {
        data.as_array()
            .into_iter()
            .flatten()
            .take(512)
            .filter_map(|device| {
                let name = device.get("name")?.as_str()?;
                if name.is_empty() || name.len() > 4096 || name.chars().any(char::is_control) {
                    return None;
                }
                let description = device
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(name);
                Some(AudioDevice {
                    name: name.into(),
                    description: description
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(4096)
                        .collect(),
                })
            })
            .collect()
    }

    fn stat_string(data: &Value) -> String {
        data.as_str()
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .take(256)
            .collect()
    }

    fn stat_u32(data: &Value) -> u32 {
        data.as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0)
    }

    fn update_audio_stats(stats: &mut AudioStats, name: &str, data: &Value) -> bool {
        let old = stats.clone();
        match name {
            "audio-params" => {
                stats.sample_format = stat_string(&data["format"]);
                stats.sample_rate = stat_u32(&data["samplerate"]);
                stats.channels = stat_string(&data["channels"]);
                stats.channel_count = stat_u32(&data["channel-count"]);
            }
            "audio-out-params" => {
                stats.output_sample_rate = stat_u32(&data["samplerate"]);
                stats.output_channels = stat_string(&data["channels"]);
            }
            "current-tracks/audio/codec" => stats.codec = stat_string(data),
            "audio-bitrate" => {
                stats.bitrate = finite_nonnegative(data)
                    .map(|value| value as u64)
                    .unwrap_or(0)
            }
            _ => {}
        }
        *stats != old
    }

    fn finite_nonnegative(value: &Value) -> Option<f64> {
        value.as_f64().filter(|v| v.is_finite() && *v >= 0.0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn pause_fades_reverse_smoothly_and_preserve_preamp() {
            let began = Instant::now();
            let mut fade = PauseFade::default();
            fade.start(true, false, began);
            assert_eq!(fade.advance(began + Duration::from_millis(75)), Some(false));
            assert!((fade.gain_db(-3.0) - (-9.0205999)).abs() < 0.001);
            // A resume arriving during fade-out starts at the observed level.
            fade.start(false, false, began + Duration::from_millis(75));
            fade.advance(began + Duration::from_millis(150));
            assert!((fade.level - 0.75).abs() < 0.001);
            fade.advance(began + Duration::from_millis(225));
            assert_eq!(fade.level, 1.0);
            assert_eq!(fade.gain_db(-3.0), -3.0);
            fade.start(true, false, began + Duration::from_millis(225));
            assert_eq!(fade.advance(began + Duration::from_millis(375)), Some(true));
            assert_eq!(fade.gain_db(-3.0), -63.0);
            fade.start(false, true, began + Duration::from_millis(375));
            assert_eq!(fade.gain_db(-3.0), -63.0);
            assert!(fade.reset());
            assert_eq!(fade.gain_db(-3.0), -3.0);
        }

        #[test]
        fn pitch_and_gapless_preferences_map_to_mpv_modes() {
            let mut settings = PlaybackSettings::default();
            for (enabled, weak, expected) in [
                (true, true, "weak"),
                (true, false, "yes"),
                (false, true, "no"),
            ] {
                settings.gapless = enabled;
                settings.gapless_weak = weak;
                let properties: HashMap<_, _> =
                    settings_properties(&settings, false).into_iter().collect();
                assert_eq!(properties["gapless-audio"], json!(expected));
                assert_eq!(properties["audio-pitch-correction"], json!(true));
            }
            settings.preserve_pitch = false;
            let properties: HashMap<_, _> =
                settings_properties(&settings, false).into_iter().collect();
            assert_eq!(properties["audio-pitch-correction"], json!(false));
        }

        #[test]
        fn settings_changes_do_not_reinitialize_unchanged_output_and_filters() {
            let mut protocol = Protocol::new(Duration::from_secs(1));
            let previous = PlaybackSettings::default();
            let next = PlaybackSettings {
                speed: 1.5,
                ..previous.clone()
            };
            let mut output = Vec::new();
            apply_settings(&mut protocol, &mut output, Some(&previous), &next, true).unwrap();
            let messages: Vec<Value> = output
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect();
            assert_eq!(messages.len(), 1);
            assert_eq!(
                messages[0]["command"],
                json!(["set_property", "speed", 1.5])
            );
            let mut next = next;
            next.equalizer.enabled = true;
            assert!(audio_filters(&next, true).contains("equalizer=f=31"));
            assert!(audio_filters(&next, true).contains("@feishin_levels:astats="));
            assert!(!audio_filters(&next, false).contains("feishin_levels"));
        }
        #[test]
        fn prepared_reply_and_file_events_keep_target_generation() {
            let mut p = Protocol::new(Duration::from_secs(1));
            let id = p
                .send(&mut Vec::new(), json!({"name":"loadfile"}), "prepare")
                .unwrap();
            p.loading.insert(id, 55);
            p.preparing.insert(id);
            p.receive(json!({"event":"start-file","playlist_entry_id":42}))
                .unwrap();
            p.receive(json!({"event":"file-loaded"})).unwrap();
            assert_eq!(
                p.receive(
                    json!({"request_id":id,"error":"success","data":{"playlist_entry_id":42}})
                )
                .unwrap(),
                vec![Event::PreparedNext(55), Event::FileLoaded(55)]
            );
            assert_eq!(p.active_generation(), Some(55));
        }
        #[test]
        fn preparation_cannot_follow_a_superseded_current_generation() {
            let mut p = Protocol::new(Duration::from_secs(1));
            p.entries.insert(1, 10);
            p.active_entry = Some(1);
            p.desired_generation = Some(10);
            assert!(p.can_prepare(10));
            // A newer explicit load can be acknowledged before start-file.
            p.desired_generation = Some(20);
            assert!(!p.can_prepare(10));
            p.entries.insert(2, 30);
            p.active_entry = Some(2);
            p.desired_generation = Some(20);
            p.prepared_transition = Some((20, 30));
            p.track_event(2, TrackEvent::Loaded).unwrap();
            assert!(p.can_prepare(30));
        }
        #[test]
        fn cancelling_preparation_does_not_promote_a_late_started_successor() {
            let mut p = Protocol::new(Duration::from_secs(1));
            p.entries.insert(1, 10);
            p.entries.insert(2, 20);
            p.active_entry = Some(1);
            p.desired_generation = Some(10);
            p.prepared_transition = Some((10, 20));
            p.cancel_prepared();
            p.receive(json!({"event":"start-file","playlist_entry_id":2}))
                .unwrap();
            assert_eq!(
                p.receive(json!({"event":"file-loaded"})).unwrap(),
                vec![Event::FileLoaded(20)]
            );
            assert_eq!(p.desired_generation, Some(10));
            assert!(!p.can_prepare(20));
            assert!(!p.can_prepare(10));
        }
        #[test]
        fn audio_level_metadata_handles_silence_mono_and_invalid_numbers() {
            assert!(audio_levels(&json!({})).is_none());
            assert!(
                audio_levels(
                    &json!({"lavfi.astats.1.RMS_level":"nan","lavfi.astats.1.Peak_level":"-10"})
                )
                .is_none()
            );
            assert_eq!(
                audio_levels(
                    &json!({"lavfi.astats.1.RMS_level":"-inf","lavfi.astats.1.Peak_level":"-inf"})
                ),
                Some(Event::AudioLevels {
                    left_rms_db: -120.0,
                    right_rms_db: -120.0,
                    left_peak_db: -120.0,
                    right_peak_db: -120.0
                })
            );
        }
        #[test]
        fn malformed_device_and_audio_parameters_are_bounded() {
            assert!(audio_devices(&json!([{"name":"bad\nname"}])).is_empty());
            let devices =
                audio_devices(&json!([{"name":"auto","description":"Automatic\noutput"}]));
            assert_eq!(devices[0].description, "Automaticoutput");
            let mut stats = AudioStats::default();
            assert!(update_audio_stats(
                &mut stats,
                "audio-params",
                &json!({"samplerate":44100,"channels":"stereo","channel-count":2,"format":"floatp"})
            ));
            assert_eq!(stats.sample_rate, 44100);
            assert_eq!(stats.channel_count, 2);
            update_audio_stats(&mut stats, "audio-bitrate", &json!(128000));
            assert_eq!(stats.bitrate, 128000);
            update_audio_stats(
                &mut stats,
                "audio-params",
                &json!({"samplerate":-100,"channel-count":18446744073709551615_u64}),
            );
            assert_eq!(stats.sample_rate, 0);
            assert_eq!(stats.channel_count, 0);
        }
        #[test]
        fn split_and_coalesced_messages_remain_intact() {
            let mut decoder = Decoder::default();
            assert!(decoder.push(b"{\"event\":\"file-").unwrap().is_empty());
            let messages = decoder
                .push(b"loaded\"}\n{\"event\":\"end-file\",\"reason\":\"stop\"}\n")
                .unwrap();
            assert_eq!(messages.len(), 2);
            assert_eq!(messages[0]["event"], "file-loaded");
            assert_eq!(messages[1]["reason"], "stop");
        }
        #[test]
        fn malformed_and_oversized_messages_do_not_echo_private_data() {
            let mut decoder = Decoder::default();
            assert_eq!(
                decoder.push(b"private-token\n").unwrap_err(),
                "MPV sent an invalid IPC message."
            );
            let mut decoder = Decoder::default();
            assert!(decoder.push(&vec![b' '; MAX_MESSAGE + 1]).is_err());
        }
        #[test]
        fn request_ids_correlate_out_of_order_and_do_not_echo_mpv_errors() {
            let mut protocol = Protocol::new(Duration::from_secs(1));
            let mut output = Vec::new();
            protocol
                .send(&mut output, json!(["stop"]), "stop playback")
                .unwrap();
            protocol
                .send(&mut output, json!(["seek", 2]), "seek")
                .unwrap();
            assert!(
                protocol
                    .receive(json!({"request_id":2,"error":"success"}))
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(protocol.pending.len(), 1);
            assert_eq!(
                protocol
                    .receive(json!({"request_id":1,"error":"private-token"}))
                    .unwrap(),
                vec![Event::Error("MPV could not stop playback.".into())]
            );
        }
        #[test]
        fn unanswered_requests_expire_and_invalid_numbers_are_rejected() {
            let mut protocol = Protocol::new(Duration::ZERO);
            protocol
                .send(&mut Vec::new(), json!(["stop"]), "stop playback")
                .unwrap();
            assert!(protocol.expired());
            assert!(encode_command(Command::Seek(f64::NAN)).is_err());
            assert!(encode_command(Command::Volume(f64::INFINITY)).is_err());
        }
        #[test]
        fn load_options_are_file_local_and_header_values_cannot_inject_fields() {
            let (message, _) = encode_command(Command::Load {
                generation: 1,
                paused: false,
                url: "https://example.invalid/stream?token=private".into(),
                headers: vec![("Authorization".into(), "Bearer a,b\\c".into())],
            })
            .unwrap();
            assert_eq!(message["name"], "loadfile");
            assert_eq!(message["options"]["pause"], "no");
            assert_eq!(
                message["options"]["http-header-fields"],
                "Authorization: Bearer a\\,b\\c"
            );
            assert!(
                header_options(vec![("Authorization".into(), "a\r\nInjected: yes".into())])
                    .is_err()
            );
            let (message, _) = encode_command(Command::Load {
                generation: 2,
                paused: false,
                url: "file.wav".into(),
                headers: vec![],
            })
            .unwrap();
            assert_eq!(message["options"]["http-header-fields"], "");
        }
        #[test]
        fn stale_eof_and_early_events_keep_their_own_generation() {
            let mut p = Protocol::new(Duration::from_secs(1));
            p.entries.insert(12, 101);
            let id = p
                .send(&mut Vec::new(), json!({"name":"loadfile"}), "load")
                .unwrap();
            p.loading.insert(id, 202);
            // A newer track starts before its acknowledgement is received.
            assert!(
                p.receive(json!({"event":"start-file","playlist_entry_id":13}))
                    .unwrap()
                    .is_empty()
            );
            assert!(
                p.receive(json!({"event":"file-loaded"}))
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                p.receive(
                    json!({"request_id":id,"error":"success","data":{"playlist_entry_id":13}})
                )
                .unwrap(),
                vec![Event::FileLoaded(202)]
            );
            assert_eq!(
                p.receive(json!({"event":"end-file","reason":"eof","playlist_entry_id":12}))
                    .unwrap(),
                vec![Event::EndFile {
                    generation: 101,
                    reason: EndReason::Eof
                }]
            );
            // Stale end-file did not clear the newer active entry.
            assert_eq!(p.active_entry, Some(13));
            assert_eq!(
                p.receive(json!({"event":"end-file","reason":"stop","playlist_entry_id":13}))
                    .unwrap(),
                vec![Event::EndFile {
                    generation: 202,
                    reason: EndReason::Stop
                }]
            );
            // Duplicate events never advance twice.
            assert!(
                p.receive(json!({"event":"end-file","reason":"eof","playlist_entry_id":13}))
                    .unwrap()
                    .is_empty()
            );
        }
        #[test]
        fn skipped_unstarted_entries_do_not_exhaust_playback_bookkeeping() {
            let mut p = Protocol::new(Duration::from_secs(1));
            for generation in 0..1000_u64 {
                let id = p
                    .send(&mut Vec::new(), json!({"name":"loadfile"}), "load")
                    .unwrap();
                p.loading.insert(id, generation);
                assert!(
                    p.receive(
                        json!({"request_id":id,"error":"success","data":{"playlist_entry_id":id}})
                    )
                    .unwrap()
                    .is_empty()
                );
                assert!(p.entries.len() <= 64);
            }
        }
        #[test]
        fn missing_entry_identity_is_an_error_instead_of_guessing_generation() {
            let mut p = Protocol::new(Duration::from_secs(1));
            let id = p
                .send(&mut Vec::new(), json!({"name":"loadfile"}), "load")
                .unwrap();
            p.loading.insert(id, 1);
            assert!(
                p.receive(json!({"request_id":id,"error":"success","data":null}))
                    .is_err()
            );
        }
        #[test]
        fn only_natural_eof_is_eof() {
            let mut p = Protocol::new(Duration::from_secs(1));
            for (reason, expected) in [
                ("eof", EndReason::Eof),
                ("stop", EndReason::Stop),
                ("error", EndReason::Error),
                ("redirect", EndReason::Redirect),
            ] {
                p.entries.insert(1, 5);
                assert_eq!(
                    p.receive(json!({"event":"end-file","playlist_entry_id":1,"reason":reason}))
                        .unwrap(),
                    vec![Event::EndFile {
                        generation: 5,
                        reason: expected
                    }]
                );
            }
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn missing_executable_fails_asynchronously_without_leaking_path() {
        let config = Config {
            executable: "/missing/private-token-mpv".into(),
            ..Config::default()
        };
        let player = Player::spawn(config).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_error = false;
        while Instant::now() < deadline {
            match player.try_recv() {
                Ok(Event::Error(message)) => {
                    assert!(!message.contains("private-token"));
                    saw_error = true;
                }
                Ok(Event::Disconnected) => {
                    assert!(saw_error);
                    return;
                }
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("missing executable did not report a bounded startup failure");
    }
}
