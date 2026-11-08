//! An application-owned, external MPV process controlled through JSON IPC.
//!
//! Process startup, IPC and shutdown run on a worker thread. This crate never
//! renders audio and never falls back to another playback engine.
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            executable: "mpv".into(),
            extra_args: Vec::new(),
            startup_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(5),
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
    Buffering(bool),
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

#[cfg(unix)]
fn run(
    config: Config,
    commands: Receiver<Command>,
    events: &mpsc::Sender<Event>,
    stopping: &AtomicBool,
) -> Result<(), &'static str> {
    unix::run(config, commands, events, stopping)
}

#[cfg(not(unix))]
fn run(
    _config: Config,
    _commands: Receiver<Command>,
    _events: &mpsc::Sender<Event>,
    _stopping: &AtomicBool,
) -> Result<(), &'static str> {
    Err(
        "MPV IPC is currently supported on Linux and macOS only. Windows named-pipe support is not implemented.",
    )
}

#[cfg(unix)]
mod unix {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        collections::HashMap,
        io::{Read, Write},
        os::unix::{fs::PermissionsExt, net::UnixStream},
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

    pub(super) fn run(
        config: Config,
        commands: Receiver<Command>,
        events: &mpsc::Sender<Event>,
        stopping: &AtomicBool,
    ) -> Result<(), &'static str> {
        let directory = tempfile::Builder::new()
            .prefix("feishin-mpv-")
            .tempdir()
            .map_err(|_| "Could not create a private MPV IPC directory.")?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not secure the MPV IPC directory.")?;
        let socket = directory.path().join("ipc");
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
            .arg(format!("--input-ipc-server={}", socket.display()))
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
            if let Ok(stream) = UnixStream::connect(&socket) {
                break stream;
            }
            if started.elapsed() >= config.startup_timeout {
                return Err("MPV did not create its IPC socket before the startup timeout.");
            }
            thread::sleep(POLL);
        };
        stream
            .set_read_timeout(Some(POLL))
            .map_err(|_| "Could not configure MPV IPC.")?;
        stream
            .set_write_timeout(Some(Duration::from_millis(250)))
            .map_err(|_| "Could not configure MPV IPC.")?;
        let mut protocol = Protocol::new(config.command_timeout);
        for (id, name) in [
            "pause",
            "time-pos",
            "duration",
            "volume",
            "paused-for-cache",
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
                    Ok(command) => {
                        let generation = match &command {
                            Command::Load { generation, .. } => Some(*generation),
                            _ => None,
                        };
                        match encode_command(command) {
                            Ok((value, label)) => {
                                let id = protocol.send(&mut stream, value, label)?;
                                if let Some(generation) = generation {
                                    protocol.loading.insert(id, generation);
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
            match stream.read(&mut bytes) {
                Ok(0) => {
                    return Err("MPV closed its IPC connection. Reconnect the player to continue.");
                }
                Ok(count) => {
                    for message in decoder.push(bytes.get(..count).unwrap_or_default())? {
                        for event in protocol.receive(message)? {
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
            } => {
                if url.is_empty() || url.len() > 65536 || url.chars().any(char::is_control) {
                    return Err("The stream source is invalid.");
                }
                // Named arguments avoid loadfile's positional options/index
                // incompatibility introduced in MPV 0.38.
                (
                    json!({"name":"loadfile", "url":url, "flags":"replace", "options":{
                        "http-header-fields":header_options(headers)?, "pause":if paused {"yes"} else {"no"}
                    }}),
                    "load the selected track",
                )
            }
            Command::Pause(value) => (
                json!(["set_property", "pause", value]),
                "change pause state",
            ),
            Command::Seek(value) if value.is_finite() && value >= 0.0 => {
                (json!(["seek", value, "absolute"]), "seek")
            }
            Command::Volume(value) if value.is_finite() => (
                json!(["set_property", "volume", value.clamp(0.0, 100.0)]),
                "change volume",
            ),
            Command::Stop => (json!(["stop"]), "stop playback"),
            Command::Shutdown => (json!(["quit"]), "quit"),
            _ => return Err("The playback command contained an invalid number."),
        })
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
        entries: HashMap<i64, u64>,
        active_entry: Option<i64>,
        deferred: Vec<(i64, TrackEvent)>,
    }
    impl Protocol {
        fn new(timeout: Duration) -> Self {
            Self {
                next_id: 1,
                timeout,
                pending: HashMap::new(),
                loading: HashMap::new(),
                entries: HashMap::new(),
                active_entry: None,
                deferred: Vec::new(),
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
                TrackEvent::Loaded => Event::FileLoaded(generation),
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
                        let mut deferred = std::mem::take(&mut self.deferred);
                        for (event_entry, kind) in deferred.drain(..) {
                            if let Some(event) = self.track_event(event_entry, kind)? {
                                events.push(event);
                            }
                        }
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
                        Some("paused-for-cache") => data.as_bool().map(Event::Buffering),
                        _ => None,
                    };
                    events.extend(event);
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

    fn finite_nonnegative(value: &Value) -> Option<f64> {
        value.as_f64().filter(|v| v.is_finite() && *v >= 0.0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
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
