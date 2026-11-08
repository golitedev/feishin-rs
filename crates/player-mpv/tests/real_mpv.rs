//! Explicit integration checks; run with MPV_BIN set and `--ignored`.
#![cfg(unix)]

use feishin_player_mpv::{Command, Config, EndReason, Event, Player};
use std::{
    fs,
    sync::mpsc::TryRecvError,
    thread,
    time::{Duration, Instant},
};

fn wait(player: &Player, predicate: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        match player.try_recv() {
            Ok(event) => {
                assert!(!matches!(event, Event::Error(_)), "MPV error: {event:?}");
                assert_ne!(event, Event::Disconnected, "MPV disconnected");
                if predicate(&event) {
                    return event;
                }
            }
            Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(10)),
            Err(TryRecvError::Disconnected) => panic!("MPV worker disconnected"),
        }
    }
    panic!("timed out waiting for MPV event");
}

fn player() -> Player {
    Player::spawn(Config {
        executable: std::env::var("MPV_BIN").unwrap_or_else(|_| "mpv".into()),
        extra_args: vec!["--ao=null".into()],
        ..Config::default()
    })
    .unwrap()
}

fn wav() -> Vec<u8> {
    let sample_rate = 8000_u32;
    let sample_count = sample_rate * 2;
    let data_size = sample_count * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.resize(bytes.len() + data_size as usize, 0);
    bytes
}

#[test]
#[ignore = "requires an installed MPV executable; uses its null audio output"]
fn playback_controls_eof_and_bad_file_are_distinct() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("test track.wav");
    fs::write(&file, wav()).unwrap();
    let player = player();
    wait(&player, |event| *event == Event::Ready);
    let load = || Command::Load {
        generation: 1,
        paused: false,
        url: file.display().to_string(),
        headers: vec![],
    };
    player.send(load()).unwrap();
    wait(&player, |event| *event == Event::FileLoaded(1));
    player.send(Command::Pause(true)).unwrap();
    wait(&player, |event| *event == Event::Pause(true));
    player.send(Command::Volume(37.0)).unwrap();
    wait(
        &player,
        |event| matches!(event, Event::Volume(value) if (*value - 37.0).abs() < 0.1),
    );
    player.send(Command::Seek(0.5)).unwrap();
    wait(
        &player,
        |event| matches!(event, Event::Position(value) if *value >= 0.4),
    );
    player.send(Command::Stop).unwrap();
    wait(&player, |event| {
        *event
            == Event::EndFile {
                generation: 1,
                reason: EndReason::Stop,
            }
    });
    player.send(load()).unwrap();
    wait(&player, |event| *event == Event::FileLoaded(1));
    wait(&player, |event| {
        *event
            == Event::EndFile {
                generation: 1,
                reason: EndReason::Eof,
            }
    });
    player
        .send(Command::Load {
            generation: 1,
            paused: false,
            url: directory.path().join("missing.wav").display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(&player, |event| {
        *event
            == Event::EndFile {
                generation: 1,
                reason: EndReason::Error,
            }
    });
    let shutdown = Instant::now();
    drop(player);
    assert!(shutdown.elapsed() < Duration::from_secs(1));
}

#[test]
#[ignore = "requires an installed MPV executable; uses its null audio output"]
fn load_can_begin_paused_without_losing_user_intent() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("paused.wav");
    fs::write(&file, wav()).unwrap();
    let player = player();
    wait(&player, |event| *event == Event::Ready);
    player
        .send(Command::Load {
            generation: 9,
            paused: true,
            url: file.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let (mut loaded, mut paused) = (false, false);
    while Instant::now() < deadline && !(loaded && paused) {
        if let Ok(event) = player.try_recv() {
            assert!(!matches!(event, Event::Error(_) | Event::Disconnected));
            loaded |= event == Event::FileLoaded(9);
            paused |= event == Event::Pause(true);
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(loaded && paused, "file must load in a paused state");
    player.send(Command::Pause(false)).unwrap();
    wait(&player, |event| {
        *event
            == Event::EndFile {
                generation: 9,
                reason: EndReason::Eof,
            }
    });
}

#[test]
#[ignore = "requires an installed MPV executable; serves a synthetic WAV on loopback"]
fn authenticated_headers_are_exact_and_do_not_leak_to_next_load() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (requests_tx, requests_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = 0;
        while requests < 2 && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut byte = [0];
                    while request.len() < 16384 && stream.read(&mut byte).unwrap_or(0) == 1 {
                        request.push(byte[0]);
                        if request.ends_with(b"\r\n\r\n") {
                            break;
                        }
                    }
                    requests_tx
                        .send(String::from_utf8_lossy(&request).to_string())
                        .unwrap();
                    let body = wav();
                    let headers = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream
                        .write_all(headers.as_bytes())
                        .and_then(|_| stream.write_all(&body));
                    requests += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("test server: {error}"),
            }
        }
        assert_eq!(requests, 2);
    });
    let player = player();
    wait(&player, |event| *event == Event::Ready);
    player
        .send(Command::Load {
            generation: 1,
            paused: false,
            url: format!("http://{address}/first.wav"),
            headers: vec![("Authorization".into(), "Bearer a,b\\c".into())],
        })
        .unwrap();
    wait(&player, |event| *event == Event::FileLoaded(1));
    let first = requests_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        first.contains("Authorization: Bearer a,b\\c\r\n"),
        "synthetic header did not survive structured encoding: {first:?}"
    );
    player
        .send(Command::Load {
            generation: 1,
            paused: false,
            url: format!("http://{address}/second.wav"),
            headers: vec![],
        })
        .unwrap();
    wait(&player, |event| *event == Event::FileLoaded(1));
    let second = requests_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!second.to_ascii_lowercase().contains("authorization:"));
    drop(player);
    server.join().unwrap();
}
