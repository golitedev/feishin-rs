//! Cross-crate listening checks using only synthetic music and a loopback server.
//! The real-MPV check is explicit: MPV_BIN=/path/to/mpv cargo test -p feishin-rs
//! --test listening -- --ignored. No real account, GUI, or audio device is needed.
use feishin_core::{Queue, ServerKind, ServerProfile, Track};
use feishin_player_mpv::{Command, Config, EndReason, Event, Player};
use feishin_servers::Client;
use md5::{Digest, Md5};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::TryRecvError,
    },
    thread,
    time::{Duration, Instant},
};

const PASSWORD: &str = "fixture-password";
const SONGS: [&str; 2] = ["first", "second"];

#[derive(Default)]
struct State {
    playlist: Vec<String>,
    playlist_exists: bool,
    streams: Vec<String>,
    scrobbles: Vec<(String, bool)>,
    authentication_failures: usize,
}

struct Fixture {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let state = Arc::new(Mutex::new(State::default()));
        let serving = state.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &serving),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("fixture listener failed: {error}"),
                }
            }
        });
        Self {
            address,
            stop,
            state,
            worker: Some(worker),
        }
    }

    fn login(&self) -> Client {
        Client::login(
            ServerProfile {
                name: "Synthetic music".into(),
                url: format!("http://{}/music", self.address),
                username: "listener".into(),
                kind: ServerKind::Navidrome,
            },
            PASSWORD,
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn song(id: &str) -> serde_json::Value {
    serde_json::json!({"id":id,"title":format!("Synthetic {id}"),"artist":"Fixture Artist","artistId":"artist","album":"Synthetic album","albumId":"album","track":if id=="first"{1}else{2},"duration":1,"contentType":"audio/wav","suffix":"wav"})
}

fn songs() -> Vec<serde_json::Value> {
    SONGS.iter().map(|id| song(id)).collect()
}

fn serve(mut stream: TcpStream, state: &Arc<Mutex<State>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") && request.len() < 32 * 1024 {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(count) => request.extend_from_slice(&buffer[..count]),
        }
    }
    let text = String::from_utf8_lossy(&request);
    let Some(target) = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return;
    };
    let Some((path, raw_query)) = target.split_once('?') else {
        return;
    };
    let query = raw_query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect::<Vec<_>>();
    let get = |key: &str| {
        query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    };
    let salt = get("s");
    let expected = format!("{:x}", Md5::digest(format!("{PASSWORD}{salt}")));
    if get("u") != "listener" || get("t") != expected || salt.len() < 32 || !get("p").is_empty() {
        state.lock().unwrap().authentication_failures += 1;
        reply(
            &mut stream,
            403,
            "text/plain",
            b"Invalid synthetic credentials",
            None,
        );
        return;
    }
    let method = path
        .strip_prefix("/music/rest/")
        .and_then(|p| p.strip_suffix(".view"))
        .unwrap_or("");
    if method == "stream" {
        let id = get("id");
        if !SONGS.contains(&id) {
            reply(
                &mut stream,
                404,
                "text/plain",
                b"Missing synthetic track",
                None,
            );
            return;
        }
        state.lock().unwrap().streams.push(id.to_owned());
        let bytes = wav();
        let range = text.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if !key.eq_ignore_ascii_case("range") {
                return None;
            }
            value
                .trim()
                .strip_prefix("bytes=")?
                .split_once('-')?
                .0
                .parse::<usize>()
                .ok()
        });
        if let Some(start) = range.filter(|start| *start < bytes.len()) {
            let content_range = format!("bytes {start}-{}/{}", bytes.len() - 1, bytes.len());
            reply(
                &mut stream,
                206,
                "audio/wav",
                &bytes[start..],
                Some(&content_range),
            );
        } else {
            reply(&mut stream, 200, "audio/wav", &bytes, None);
        }
        return;
    }
    let value = match method {
        "ping" => serde_json::json!({}),
        "getAlbumList2" => {
            serde_json::json!({"albumList2":{"album":[{"id":"album","name":"Synthetic album","artist":"Fixture Artist","artistId":"artist","songCount":2,"duration":2}]}})
        }
        "getArtists" => {
            serde_json::json!({"artists":{"index":[{"artist":[{"id":"artist","name":"Fixture Artist","albumCount":1}]}]}})
        }
        "search3" => serde_json::json!({"searchResult3":{"song":songs()}}),
        "getAlbum" => serde_json::json!({"album":{"id":"album","song":songs()}}),
        "getPlaylists" => {
            let state = state.lock().unwrap();
            let lists = if state.playlist_exists {
                vec![
                    serde_json::json!({"id":"playlist","name":"Listening fixture","songCount":state.playlist.len()}),
                ]
            } else {
                vec![]
            };
            serde_json::json!({"playlists":{"playlist":lists}})
        }
        "createPlaylist" => {
            let mut state = state.lock().unwrap();
            state.playlist_exists = true;
            state.playlist = query
                .iter()
                .filter(|(key, _)| key == "songId")
                .map(|(_, id)| id.clone())
                .collect();
            serde_json::json!({"playlist":{"id":"playlist","name":"Listening fixture","songCount":state.playlist.len()}})
        }
        "getPlaylist" => {
            let state = state.lock().unwrap();
            serde_json::json!({"playlist":{"id":"playlist","entry":state.playlist.iter().map(|id|song(id)).collect::<Vec<_>>()}})
        }
        "updatePlaylist" => {
            let mut state = state.lock().unwrap();
            for (key, id) in &query {
                if key == "songIdToAdd" {
                    state.playlist.push(id.clone());
                }
                if key == "songIndexToRemove" {
                    let index = id.parse::<usize>().unwrap();
                    state.playlist.remove(index);
                }
            }
            serde_json::json!({})
        }
        "deletePlaylist" => {
            let mut state = state.lock().unwrap();
            state.playlist_exists = false;
            state.playlist.clear();
            serde_json::json!({})
        }
        "scrobble" => {
            state
                .lock()
                .unwrap()
                .scrobbles
                .push((get("id").into(), get("submission") == "true"));
            serde_json::json!({})
        }
        _ => {
            reply(
                &mut stream,
                404,
                "text/plain",
                b"Unsupported fixture endpoint",
                None,
            );
            return;
        }
    };
    let mut response = serde_json::json!({"status":"ok","version":"1.16.1"});
    for (key, value) in value.as_object().unwrap() {
        response[key] = value.clone();
    }
    let body = serde_json::json!({"subsonic-response":response}).to_string();
    reply(&mut stream, 200, "application/json", body.as_bytes(), None);
}

fn reply(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    range: Option<&str>,
) {
    let mut headers = format!(
        "HTTP/1.1 {status} Response\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(range) = range {
        headers.push_str(&format!("Content-Range: {range}\r\n"));
    }
    headers.push_str("\r\n");
    let _ = stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(body));
}

/// One second of original, synthetic PCM silence; no external media assets.
fn wav() -> Vec<u8> {
    let rate = 8000u32;
    let data_size = rate * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.resize(bytes.len() + data_size as usize, 0);
    bytes
}

#[test]
fn server_album_queue_and_playlist_workflow_without_mpv() {
    let fixture = Fixture::start();
    let client = fixture.login();
    let library = client.library().unwrap();
    assert_eq!(library.albums.len(), 1);
    assert_eq!(library.tracks.len(), 2);
    assert_eq!(library.artists.len(), 1);
    let tracks = client.album_tracks(&library.albums[0].id).unwrap();
    let mut queue = Queue::default();
    assert!(queue.replace(tracks, 0));
    assert_eq!(queue.current_track().unwrap().id, "first");
    assert_eq!(queue.advance(true).unwrap().id, "second");
    assert!(queue.advance(true).is_none());
    let playlist = client
        .create_playlist("Listening fixture", &["first".into()])
        .unwrap();
    client
        .add_to_playlist(&playlist, &["second".into(), "first".into()])
        .unwrap();
    assert_eq!(client.playlists().unwrap()[0].song_count, 3);
    assert_eq!(
        client
            .playlist_tracks(&playlist)
            .unwrap()
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "first"]
    );
    client.remove_playlist_track(&playlist, 0).unwrap();
    assert_eq!(client.playlist_tracks(&playlist).unwrap()[0].id, "second");
    client.delete_playlist(&playlist).unwrap();
    assert!(client.playlists().unwrap().is_empty());
    assert_eq!(fixture.state.lock().unwrap().authentication_failures, 0);
}

fn wait(player: &Player, predicate: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        match player.try_recv() {
            Ok(event) => {
                assert!(
                    !matches!(event, Event::Error(_) | Event::Disconnected),
                    "unexpected MPV event: {event:?}"
                );
                if predicate(&event) {
                    return event;
                }
            }
            Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(5)),
            Err(TryRecvError::Disconnected) => panic!("MPV worker disconnected"),
        }
    }
    panic!("timed out waiting for MPV event");
}

fn load(client: &Client, player: &Player, track: &Track, generation: u64) {
    let source = client.stream(track).unwrap();
    player
        .send(Command::Load {
            generation,
            paused: false,
            url: source.url,
            headers: source.headers,
        })
        .unwrap();
}

#[test]
#[ignore = "requires installed MPV; streams synthetic HTTP WAV files into null audio output"]
fn authenticated_server_streams_play_through_mpv_and_eof_advances_queue() {
    let fixture = Fixture::start();
    let client = fixture.login();
    let library = client.library().unwrap();
    let mut queue = Queue::default();
    queue.replace(client.album_tracks(&library.albums[0].id).unwrap(), 0);
    let player = Player::spawn(Config {
        executable: std::env::var("MPV_BIN").unwrap_or_else(|_| "mpv".into()),
        extra_args: vec!["--ao=null".into()],
        ..Config::default()
    })
    .unwrap();
    wait(&player, |event| *event == Event::Ready);
    load(&client, &player, queue.current_track().unwrap(), 10);
    wait(&player, |event| *event == Event::FileLoaded(10));
    client
        .scrobble(queue.current_track().unwrap(), false)
        .unwrap();
    // Do not drain the old stop event before loading again: the adapter must tag
    // that delayed event with generation 10 rather than the new load's generation.
    player.send(Command::Stop).unwrap();
    let mut generation = 11u64;
    load(&client, &player, queue.current_track().unwrap(), generation);
    let mut retired_events = 0;
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut completed = Vec::new();
    let mut loaded = HashMap::new();
    loop {
        assert!(
            Instant::now() < deadline,
            "listening pipeline did not finish both tracks"
        );
        match player.try_recv() {
            Ok(Event::FileLoaded(current)) if current == generation => {
                loaded.insert(current, true);
                client
                    .scrobble(queue.current_track().unwrap(), false)
                    .unwrap();
            }
            Ok(Event::EndFile {
                generation: current,
                reason: EndReason::Eof,
            }) if current == generation => {
                assert_eq!(
                    loaded.get(&current),
                    Some(&true),
                    "EOF arrived before file-loaded for this generation"
                );
                let track = queue.current_track().unwrap();
                completed.push(track.id.clone());
                client.scrobble(track, true).unwrap();
                if queue.advance(true).is_some() {
                    generation += 1;
                    load(&client, &player, queue.current_track().unwrap(), generation);
                } else {
                    break;
                }
            }
            Ok(Event::EndFile {
                generation: current,
                reason,
            }) if current != generation => {
                assert_eq!(current, 10, "unexpected retired generation");
                assert_eq!(reason, EndReason::Stop, "explicit stop must not become EOF");
                assert_eq!(
                    queue.current_track().unwrap().id,
                    "first",
                    "stale stop must not advance the queue"
                );
                retired_events += 1;
            }
            Ok(Event::Error(error)) => panic!("MPV rejected synthetic stream: {error}"),
            Ok(Event::Disconnected) | Err(TryRecvError::Disconnected) => {
                panic!("MPV disconnected during listening pipeline")
            }
            Ok(_) => (),
            Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(5)),
        }
    }
    assert_eq!(completed, vec!["first", "second"]);
    assert_eq!(
        retired_events, 1,
        "the delayed stop event should have been filtered"
    );
    assert_eq!(queue.current_track().unwrap().id, "second");
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.authentication_failures, 0);
    assert!(state.streams.iter().any(|id| id == "first"));
    assert!(state.streams.iter().any(|id| id == "second"));
    let completed = state
        .scrobbles
        .iter()
        .filter(|(_, submitted)| *submitted)
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(completed, vec!["first", "second"]);
    drop(state);
    drop(player);
}
