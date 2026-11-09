//! Explicit integration checks; run with MPV_BIN set and `--ignored`.
#![cfg(unix)]

use feishin_core::playback::{AudioChannels, PlaybackSettings, ReplayGain};
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

fn stereo_tone() -> Vec<u8> {
    let sample_rate = 8000_u32;
    let samples = sample_rate * 3;
    let data_size = samples * 4;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..samples {
        let phase = index as f64 / sample_rate as f64 * 440.0 * std::f64::consts::TAU;
        let left = (phase.sin() * 16384.0) as i16;
        let right = (phase.sin() * 8192.0) as i16;
        bytes.extend_from_slice(&left.to_le_bytes());
        bytes.extend_from_slice(&right.to_le_bytes());
    }
    bytes
}

#[test]
#[ignore = "requires MPV with the astats filter; verifies real stereo RMS/peak values and silence"]
fn audio_analysis_reports_measured_stereo_levels_and_real_silence() {
    let directory = tempfile::tempdir().unwrap();
    let tone = directory.path().join("stereo.wav");
    let silence = directory.path().join("silence.wav");
    fs::write(&tone, stereo_tone()).unwrap();
    fs::write(&silence, wav()).unwrap();
    let player = player();
    wait(&player, |event| *event == Event::Ready);
    player.send(Command::EnableAnalysis(true)).unwrap();
    player
        .send(Command::Load {
            generation: 30,
            paused: false,
            url: tone.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    let level = wait(
        &player,
        |event| matches!(event, Event::AudioLevels {left_peak_db,..} if *left_peak_db > -20.0),
    );
    if let Event::AudioLevels {
        left_rms_db,
        right_rms_db,
        left_peak_db,
        right_peak_db,
    } = level
    {
        assert!(
            (left_peak_db + 6.02).abs() < 0.5,
            "unexpected measured left peak {left_peak_db}"
        );
        assert!(
            (right_peak_db + 12.04).abs() < 0.5,
            "unexpected measured right peak {right_peak_db}"
        );
        assert!(
            (left_rms_db + 9.03).abs() < 0.7,
            "unexpected measured left RMS {left_rms_db}"
        );
        assert!(
            (right_rms_db + 15.05).abs() < 0.7,
            "unexpected measured right RMS {right_rms_db}"
        );
    }
    player
        .send(Command::Load {
            generation: 31,
            paused: false,
            url: silence.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(
        &player,
        |event| matches!(event, Event::AudioLevels {left_peak_db,right_peak_db,..} if *left_peak_db == -120.0 && *right_peak_db == -120.0),
    );
    player.send(Command::EnableAnalysis(false)).unwrap();
}

#[test]
#[ignore = "requires an installed MPV executable; verifies real audio settings and observed parameters"]
fn audio_settings_devices_stats_and_volume_limits_reach_mpv() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("settings.wav");
    fs::write(&file, wav()).unwrap();
    let player = player();
    wait(&player, |event| *event == Event::Ready);
    let mut settings = PlaybackSettings {
        speed: 1.25,
        sample_rate: Some(44100),
        channels: AudioChannels::Stereo,
        replay_gain: ReplayGain::Track,
        replay_gain_preamp: 1.0,
        replay_gain_fallback: -2.0,
        preamp_db: -3.0,
        volume_max: 150.0,
        normalization: true,
        ..PlaybackSettings::default()
    };
    settings.equalizer.enabled = true;
    settings.equalizer.bands_db[3] = 3.0;
    player.send(Command::ApplySettings(settings)).unwrap();
    player.send(Command::ListAudioDevices).unwrap();
    player.send(Command::Volume(120.0)).unwrap();
    player
        .send(Command::Load {
            generation: 20,
            paused: true,
            url: file.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    let mut flags = [false; 5];
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline && !flags.iter().all(|flag| *flag) {
        match player.try_recv() {
            Ok(Event::Error(message)) => panic!("MPV audio settings failed: {message}"),
            Ok(Event::Disconnected) => panic!("MPV disconnected while applying audio settings"),
            Ok(Event::FileLoaded(20)) => flags[0] = true,
            Ok(Event::Speed(speed)) if (speed - 1.25).abs() < 0.01 => flags[1] = true,
            Ok(Event::Volume(volume)) if (volume - 120.0).abs() < 0.01 => flags[2] = true,
            Ok(Event::AudioDevices(devices))
                if devices.iter().any(|device| device.name == "auto") =>
            {
                flags[3] = true
            }
            Ok(Event::AudioStats(stats))
                if stats.sample_rate == 8000
                    && stats.output_sample_rate == 44100
                    && stats.output_channels == "stereo"
                    && !stats.codec.is_empty() =>
            {
                flags[4] = true
            }
            _ => thread::sleep(Duration::from_millis(5)),
        }
    }
    assert!(
        flags.iter().all(|flag| *flag),
        "MPV did not report all requested settings: {flags:?}"
    );
    player
        .send(Command::ApplySettings(PlaybackSettings::default()))
        .unwrap();
    player.send(Command::Volume(120.0)).unwrap();
    wait(
        &player,
        |event| matches!(event, Event::Volume(volume) if (*volume - 100.0).abs() < 0.01),
    );
}

#[test]
#[ignore = "requires an installed MPV executable; verifies actual pause-fade gain and volume isolation"]
fn pause_fades_change_mpv_gain_and_preserve_user_volume() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("fade.wav");
    fs::write(&file, stereo_tone()).unwrap();
    let player = Player::spawn(Config {
        executable: std::env::var("MPV_BIN").unwrap_or_else(|_| "mpv".into()),
        extra_args: vec!["--ao=null".into()],
        settings: PlaybackSettings {
            fade_on_pause: true,
            preamp_db: -3.0,
            ..PlaybackSettings::default()
        },
        ..Config::default()
    })
    .unwrap();
    wait(&player, |event| *event == Event::Ready);
    player.send(Command::Volume(73.0)).unwrap();
    wait(
        &player,
        |event| matches!(event, Event::Volume(volume) if (*volume - 73.0).abs() < 0.01),
    );
    player
        .send(Command::Load {
            generation: 1,
            paused: false,
            url: file.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(&player, |event| *event == Event::FileLoaded(1));
    wait(
        &player,
        |event| matches!(event, Event::Position(position) if *position >= 0.05),
    );
    let began = Instant::now();
    player.send(Command::Pause(true)).unwrap();
    let mut saw_intermediate_gain = false;
    let mut saw_quiet_gain = false;
    loop {
        assert!(began.elapsed() < Duration::from_secs(3));
        match player.try_recv() {
            Ok(Event::Error(error)) => panic!("MPV fade failed: {error}"),
            Ok(Event::AudioGain(gain)) => {
                saw_intermediate_gain |= (-62.0..-3.1).contains(&gain);
                saw_quiet_gain |= (gain + 63.0).abs() < 0.1;
            }
            Ok(Event::Volume(volume)) => assert!((volume - 73.0).abs() < 0.01),
            Ok(Event::Pause(true)) => break,
            Ok(Event::Disconnected) => panic!("MPV disconnected during fade"),
            _ => thread::sleep(Duration::from_millis(5)),
        }
    }
    assert!(began.elapsed() >= Duration::from_millis(100));
    assert!(saw_intermediate_gain && saw_quiet_gain);
    player.send(Command::Pause(false)).unwrap();
    wait(&player, |event| *event == Event::Pause(false));
    wait(
        &player,
        |event| matches!(event, Event::AudioGain(gain) if (*gain + 3.0).abs() < 0.01),
    );
    // A paused, fully faded source must not leave a new load attenuated.
    player.send(Command::Pause(true)).unwrap();
    wait(&player, |event| *event == Event::Pause(true));
    player
        .send(Command::Load {
            generation: 2,
            paused: false,
            url: file.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(
        &player,
        |event| matches!(event, Event::AudioGain(gain) if (*gain + 3.0).abs() < 0.01),
    );
    wait(&player, |event| *event == Event::FileLoaded(2));
    wait(
        &player,
        |event| matches!(event, Event::Position(position) if *position >= 0.05),
    );
    player.send(Command::Pause(true)).unwrap();
    wait(&player, |event| *event == Event::Pause(true));
    player.send(Command::Stop).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut restored = false;
    let mut stopped = false;
    while Instant::now() < deadline && !(restored && stopped) {
        match player.try_recv() {
            Ok(Event::AudioGain(gain)) if (gain + 3.0).abs() < 0.01 => restored = true,
            Ok(Event::EndFile {
                generation: 2,
                reason: EndReason::Stop,
            }) => stopped = true,
            Ok(Event::Error(error)) => panic!("MPV faded stop failed: {error}"),
            Ok(Event::Volume(volume)) => assert!((volume - 73.0).abs() < 0.01),
            _ => thread::sleep(Duration::from_millis(5)),
        }
    }
    assert!(restored && stopped);
}

#[test]
#[ignore = "requires an installed MPV executable; verifies automatic same-process next-track playback"]
fn prepared_track_starts_automatically_with_its_own_generation() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.wav");
    let second = directory.path().join("second.wav");
    fs::write(&first, wav()).unwrap();
    fs::write(&second, wav()).unwrap();
    let player = player();
    wait(&player, |event| *event == Event::Ready);
    player
        .send(Command::Load {
            generation: 101,
            paused: false,
            url: first.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(&player, |event| *event == Event::FileLoaded(101));
    player
        .send(Command::PrepareNext {
            current_generation: 101,
            generation: 202,
            url: second.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(&player, |event| *event == Event::PreparedNext(202));
    wait(&player, |event| {
        *event
            == Event::EndFile {
                generation: 101,
                reason: EndReason::Eof,
            }
    });
    wait(&player, |event| *event == Event::FileLoaded(202));
    wait(&player, |event| {
        *event
            == Event::EndFile {
                generation: 202,
                reason: EndReason::Eof,
            }
    });
}

#[test]
#[ignore = "requires an installed MPV executable; verifies prepared successor cancellation"]
fn clearing_prepared_next_and_stale_prepare_do_not_change_current_track() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("current.wav");
    fs::write(&file, wav()).unwrap();
    let player = player();
    wait(&player, |event| *event == Event::Ready);
    player
        .send(Command::Load {
            generation: 7,
            paused: true,
            url: file.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(&player, |event| *event == Event::FileLoaded(7));
    player
        .send(Command::PrepareNext {
            current_generation: 7,
            generation: 8,
            url: file.display().to_string(),
            headers: vec![],
        })
        .unwrap();
    wait(&player, |event| *event == Event::PreparedNext(8));
    player.send(Command::ClearNext).unwrap();
    player
        .send(Command::PrepareNext {
            current_generation: 6,
            generation: 9,
            url: directory.path().join("missing.wav").display().to_string(),
            headers: vec![],
        })
        .unwrap();
    player.send(Command::Pause(false)).unwrap();
    wait(&player, |event| {
        *event
            == Event::EndFile {
                generation: 7,
                reason: EndReason::Eof,
            }
    });
    let deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < deadline {
        match player.try_recv() {
            Ok(Event::FileLoaded(generation)) => panic!("cancelled successor {generation} started"),
            Ok(Event::PreparedNext(9)) => panic!("stale preparation was accepted"),
            Ok(Event::Error(message)) => panic!("cancelled preparation error: {message}"),
            _ => thread::sleep(Duration::from_millis(5)),
        }
    }
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
    check_authenticated_headers(false);
}

#[test]
#[ignore = "requires an installed MPV executable; verifies prepared-track HTTP header isolation"]
fn authenticated_headers_do_not_leak_to_prepared_successor() {
    check_authenticated_headers(true);
}

fn check_authenticated_headers(prepared: bool) {
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
    let next = if prepared {
        Command::PrepareNext {
            current_generation: 1,
            generation: 2,
            url: format!("http://{address}/second.wav"),
            headers: vec![],
        }
    } else {
        Command::Load {
            generation: 2,
            paused: false,
            url: format!("http://{address}/second.wav"),
            headers: vec![],
        }
    };
    player.send(next).unwrap();
    wait(&player, |event| *event == Event::FileLoaded(2));
    let second = requests_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(second.starts_with("GET /second.wav "), "{second:?}");
    assert!(
        !second.to_ascii_lowercase().contains("authorization:"),
        "synthetic successor inherited credentials: {second:?}"
    );
    drop(player);
    server.join().unwrap();
}
