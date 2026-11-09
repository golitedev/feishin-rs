use feishin_core::{ServerKind, ServerProfile, Track};
use feishin_servers::{Client, Error, ItemKind};
use md5::{Digest, Md5};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    headers: String,
    body: Vec<u8>,
}
impl Request {
    fn url(&self) -> reqwest::Url {
        reqwest::Url::parse(&format!("http://localhost{}", self.path)).unwrap()
    }
    fn query(&self, key: &str) -> Option<String> {
        self.url()
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    }
}
struct Reply {
    status: u16,
    body: Value,
    declared_length: Option<usize>,
}
impl From<Value> for Reply {
    fn from(body: Value) -> Self {
        Self {
            status: 200,
            body,
            declared_length: None,
        }
    }
}
struct Mock {
    addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Mock {
    fn start(handler: impl Fn(&Request) -> Reply + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            for connection in listener.incoming() {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(mut stream) = connection else { break };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let header_end = loop {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                    }
                    if let Some(index) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        break index + 4;
                    }
                    assert!(bytes.len() < 65536);
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < header_end + length {
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let first = headers.lines().next().unwrap();
                let mut parts = first.split_whitespace();
                let request = Request {
                    method: parts.next().unwrap().to_owned(),
                    path: parts.next().unwrap().to_owned(),
                    headers: headers.clone(),
                    body: bytes[header_end..].to_vec(),
                };
                recorded.lock().unwrap().push(request.clone());
                let reply = handler(&request);
                let body = reply.body.to_string();
                let length = reply.declared_length.unwrap_or(body.len());
                let _ = write!(
                    stream,
                    "HTTP/1.1 {} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.status, length, body
                );
            }
        });
        Self {
            addr,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    fn profile(&self, kind: ServerKind) -> ServerProfile {
        ServerProfile {
            name: "test".into(),
            url: format!("http://{}/music", self.addr),
            username: "alice".into(),
            kind,
        }
    }
    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.addr);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn ok(value: Value) -> Reply {
    let mut response = json!({"status":"ok","version":"1.16.1"});
    if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            response[key] = value.clone();
        }
    }
    json!({"subsonic-response":response}).into()
}
fn subsonic_song(id: &str) -> Value {
    json!({"id":id,"title":"Example track","artist":"Artist","album":"Album","albumId":"a","artistId":"ar","duration":123,"track":2,"discNumber":1,"coverArt":"cover","starred":"2026-01-01T00:00:00Z"})
}
fn jellyfin_song(id: &str) -> Value {
    json!({"Id":id,"Type":"Audio","Name":"Example track","Artists":["Artist"],"ArtistItems":[{"Id":"ar","Name":"Artist"}],"Album":"Album","AlbumId":"a","RunTimeTicks":1230000000u64,"IndexNumber":2,"ParentIndexNumber":1,"ProductionYear":2026,"Genres":["Jazz"],"UserData":{"IsFavorite":true}})
}

#[test]
fn subsonic_token_auth_preserves_base_path_and_never_sends_password() {
    let server = Mock::start(|_| ok(json!({})));
    let client = Client::login(server.profile(ServerKind::Navidrome), "sesame").unwrap();
    let request = server.requests().remove(0);
    assert_eq!(request.url().path(), "/music/rest/ping.view");
    assert_eq!(request.query("u").as_deref(), Some("alice"));
    let salt = request.query("s").unwrap();
    let digest = Md5::digest(format!("sesame{salt}"));
    assert_eq!(request.query("t"), Some(format!("{digest:x}")));
    assert!(!request.path.contains("sesame"));
    assert!(!format!("{client:?}").contains("alice"));
    let source = client
        .stream(&Track {
            id: "song/with?special".into(),
            ..Default::default()
        })
        .unwrap();
    let url = reqwest::Url::parse(&source.url).unwrap();
    assert_eq!(
        url.query_pairs().find(|(k, _)| k == "id").unwrap().1,
        "song/with?special"
    );
    assert_eq!(format!("{source:?}"), "StreamSource([redacted])");
}

#[test]
fn normalizes_album_tracks_and_keeps_original_playlist_order() {
    let server = Mock::start(|r| {
        if r.path.contains("getAlbum") {
            ok(
                json!({"album":{"coverArt":"fallback","song":[subsonic_song("b"),{"id":"a","title":"First","track":1}]}}),
            )
        } else if r.path.contains("getPlaylist") {
            ok(
                json!({"playlist":{"entry":[subsonic_song("b"),{"id":"a","title":"First","track":1}]}}),
            )
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::OpenSubsonic), "pw").unwrap();
    let tracks = client.album_tracks("a").unwrap();
    assert_eq!(tracks[0].id, "a");
    assert_eq!(tracks[0].album_id, "a");
    assert_eq!(tracks[0].cover_id, "fallback");
    assert_eq!(tracks[1].duration, 123.0);
    assert!(tracks[1].favorite);
    assert_eq!(client.playlist_tracks("p").unwrap()[0].id, "b");
}

#[test]
fn library_paginates_albums_and_songs_and_normalizes_artists_playlists() {
    let server = Mock::start(|r| {
        if r.path.contains("getAlbumList2") {
            let offset = r.query("offset").unwrap().parse::<usize>().unwrap();
            let count = if offset == 0 { 200 } else { 1 };
            let albums=(offset..offset+count).map(|i|json!({"id":format!("a{i}"),"name":format!("Album {i}"),"artist":"Artist","genre":"Jazz"})).collect::<Vec<_>>();
            ok(json!({"albumList2":{"album":albums}}))
        } else if r.path.contains("getArtists") {
            ok(
                json!({"artists":{"index":[{"name":"A","artist":[{"id":"ar","name":"Artist","albumCount":201,"starred":"2026-11-09T00:00:00Z"}]}]}}),
            )
        } else if r.path.contains("getPlaylists") {
            ok(json!({"playlists":{"playlist":[{"id":"p","name":"Playlist","songCount":2}]}}))
        } else if r.path.contains("search3") {
            let offset = r.query("songOffset").unwrap().parse::<usize>().unwrap();
            let count = if offset == 0 { 200 } else { 1 };
            ok(
                json!({"searchResult3":{"song":(offset..offset+count).map(|i|subsonic_song(&format!("s{i}"))).collect::<Vec<_>>()}}),
            )
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::Navidrome), "pw").unwrap();
    let library = client.library().unwrap();
    assert_eq!(library.albums.len(), 201);
    assert_eq!(library.tracks.len(), 201);
    assert_eq!(library.artists[0].album_count, 201);
    assert!(library.artists[0].favorite);
    assert_eq!(library.playlists[0].name, "Playlist");
    assert_eq!(library.genres, vec!["Jazz"]);
    assert!(
        server
            .requests()
            .iter()
            .any(|r| r.query("songOffset").as_deref() == Some("200"))
    );
}

#[test]
fn api_errors_and_http_errors_do_not_expose_server_messages_or_credentials() {
    let server = Mock::start(|_| {
        json!({"subsonic-response":{"status":"failed","error":{"code":40,"message":"secret sesame https://private/?token=secret"}}}).into()
    });
    let error = Client::login(server.profile(ServerKind::Navidrome), "sesame").unwrap_err();
    assert!(matches!(error, Error::Api(40)));
    let diagnostic = format!("{error} {error:?}");
    assert!(!diagnostic.contains("secret"));
    assert!(!diagnostic.contains("sesame"));
    assert!(!diagnostic.contains("private"));
    let server = Mock::start(|_| Reply {
        status: 401,
        body: json!({"message":"secret"}),
        declared_length: None,
    });
    assert!(matches!(
        Client::login(server.profile(ServerKind::Navidrome), "pw"),
        Err(Error::Http(401))
    ));
}

#[test]
fn rejects_credential_urls_redirects_and_oversized_responses() {
    for url in [
        "file:///etc/passwd",
        "ftp://example.org",
        "http://user:pw@example.org",
        "https://example.org/?token=x",
        "https://example.org/#token",
    ] {
        assert!(matches!(
            Client::login(
                ServerProfile {
                    url: url.into(),
                    ..Default::default()
                },
                "pw"
            ),
            Err(Error::InvalidUrl)
        ));
    }
    let server = Mock::start(|_| Reply {
        status: 302,
        body: json!({}),
        declared_length: None,
    });
    assert!(matches!(
        Client::login(server.profile(ServerKind::Navidrome), "pw"),
        Err(Error::Http(302))
    ));
    let server = Mock::start(|_| Reply {
        status: 200,
        body: json!({}),
        declared_length: Some(17 * 1024 * 1024),
    });
    assert!(matches!(
        Client::login(server.profile(ServerKind::Navidrome), "pw"),
        Err(Error::ResponseTooLarge)
    ));
}

#[test]
fn jellyfin_login_stream_headers_and_favorite_requests() {
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"private-token","User":{"Id":"user"}}).into()
        } else if r.path.contains("FavoriteItems") {
            json!({"IsFavorite":true}).into()
        } else if r.path.contains("/Items") {
            json!({"Items":[jellyfin_song("s")],"TotalRecordCount":1}).into()
        } else {
            json!({}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "password").unwrap();
    let request = &server.requests()[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.url().path(), "/music/Users/AuthenticateByName");
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body).unwrap()["Pw"],
        "password"
    );
    assert!(request.headers.contains("MediaBrowser"));
    assert!(!request.path.contains("password"));
    let tracks = client.album_tracks("a").unwrap();
    assert_eq!(tracks[0].duration, 123.0);
    assert_eq!(tracks[0].artist_id, "ar");
    assert!(tracks[0].favorite);
    let source = client.stream(&tracks[0]).unwrap();
    assert!(!source.url.contains("private-token"));
    assert_eq!(source.headers.len(), 1);
    assert_eq!(source.headers[0].0, "Authorization");
    assert!(source.headers[0].1.contains("Token=\"private-token\""));
    assert!(source.url.contains("/music/Audio/s/stream"));
    client.set_favorite("s", ItemKind::Track, true).unwrap();
    client.set_favorite("s", ItemKind::Track, false).unwrap();
    let requests = server.requests();
    let last = requests.last().unwrap();
    assert_eq!(last.method, "DELETE");
    assert!(last.headers.contains("private-token"));
    client.scrobble(&tracks[0], false).unwrap();
    client.playback_progress(&tracks[0], 12.5, true).unwrap();
    client.scrobble(&tracks[0], true).unwrap();
    let requests = server.requests();
    let reports = requests
        .iter()
        .filter(|r| r.path.contains("/Sessions/Playing"))
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 3);
    let start: Value = serde_json::from_slice(&reports[0].body).unwrap();
    let progress: Value = serde_json::from_slice(&reports[1].body).unwrap();
    let stop: Value = serde_json::from_slice(&reports[2].body).unwrap();
    assert_eq!(start["ItemId"], "s");
    assert_eq!(progress["PositionTicks"], 125000000);
    assert_eq!(progress["IsPaused"], true);
    assert_eq!(stop["PositionTicks"], 1230000000);
    assert_eq!(start["PlaySessionId"], stop["PlaySessionId"]);
}

#[test]
fn jellyfin_pagination_uses_total_record_count_even_with_short_pages() {
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else if r.path.contains("/Items") {
            let start = r.query("StartIndex").unwrap();
            let song = jellyfin_song(if start == "0" { "one" } else { "two" });
            json!({"Items":[song],"TotalRecordCount":2}).into()
        } else {
            json!({"Items":[],"TotalRecordCount":0}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    assert_eq!(client.album_tracks("a").unwrap().len(), 2);
    assert!(
        server
            .requests()
            .iter()
            .any(|r| r.query("StartIndex").as_deref() == Some("1"))
    );
}

#[test]
fn detects_servers_ignoring_pagination_instead_of_looping_or_returning_partial_results() {
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else {
            json!({"Items":[jellyfin_song("same")],"TotalRecordCount":2}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    assert!(matches!(client.album_tracks("a"), Err(Error::LibraryLimit)));
}

#[test]
fn subsonic_favorites_scrobble_and_structured_lyrics() {
    let server = Mock::start(|r| {
        if r.path.contains("getLyricsBySongId") {
            ok(
                json!({"lyricsList":{"structuredLyrics":[{"synced":false,"line":[{"value":"First line"},{"value":"Second line"}]}]}}),
            )
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::Navidrome), "pw").unwrap();
    let track = Track {
        id: "s".into(),
        ..Default::default()
    };
    client.set_favorite("a", ItemKind::Album, true).unwrap();
    client.playback_started(&track, true).unwrap();
    client.scrobble(&track, true).unwrap();
    assert_eq!(client.lyrics(&track).unwrap(), "First line\nSecond line");
    let requests = server.requests();
    assert!(
        requests
            .iter()
            .any(|r| r.query("albumId").as_deref() == Some("a"))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.query("submission").as_deref() == Some("false"))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.query("submission").as_deref() == Some("true"))
    );
}

#[test]
fn subsonic_playlist_mutations_preserve_repeated_query_parameters() {
    let server = Mock::start(|r| {
        if r.path.contains("createPlaylist") {
            ok(json!({"playlist":{"id":"new"}}))
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::OpenSubsonic), "pw").unwrap();
    let ids = vec!["one".to_owned(), "two".to_owned()];
    assert_eq!(client.create_playlist("New list", &ids).unwrap(), "new");
    client.add_to_playlist("new", &ids).unwrap();
    client.remove_playlist_track("new", 1).unwrap();
    client.delete_playlist("new").unwrap();
    let requests = server.requests();
    let add = requests
        .iter()
        .find(|r| r.query("songIdToAdd").is_some())
        .unwrap();
    assert_eq!(
        add.url()
            .query_pairs()
            .filter(|(k, _)| k == "songIdToAdd")
            .count(),
        2
    );
    assert!(
        requests
            .iter()
            .any(|r| r.query("songIndexToRemove").as_deref() == Some("1"))
    );
}

#[test]
fn jellyfin_playlist_removal_uses_entry_id_not_track_id_and_accepts_duplicate_songs() {
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else if r.path == "/music/Playlists" {
            json!({"Id":"new"}).into()
        } else if r.method == "GET" && r.path.contains("/Playlists/") {
            let mut first = jellyfin_song("same-song");
            first["PlaylistItemId"] = json!("entry-one");
            let mut second = first.clone();
            second["PlaylistItemId"] = json!("entry-two");
            json!({"Items":[first,second],"TotalRecordCount":2}).into()
        } else {
            json!({}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let ids = vec!["same-song".into()];
    assert_eq!(client.create_playlist("New list", &ids).unwrap(), "new");
    assert_eq!(client.playlist_tracks("new").unwrap().len(), 2);
    client.add_to_playlist("new", &ids).unwrap();
    client.remove_playlist_track("new", 1).unwrap();
    client.delete_playlist("new").unwrap();
    let requests = server.requests();
    let remove = requests
        .iter()
        .find(|r| r.query("entryIds").is_some())
        .unwrap();
    assert_eq!(remove.method, "DELETE");
    assert_eq!(remove.query("entryIds").as_deref(), Some("entry-two"));
    let create = requests
        .iter()
        .find(|r| r.path == "/music/Playlists")
        .unwrap();
    let body: Value = serde_json::from_slice(&create.body).unwrap();
    assert_eq!(body["Name"], "New list");
    assert_eq!(body["IsPublic"], false);
}

#[test]
fn stale_jellyfin_reports_do_not_stop_the_new_track_and_duplicate_stops_are_ignored() {
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else {
            json!({}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let old = Track {
        id: "old".into(),
        ..Default::default()
    };
    let new = Track {
        id: "new".into(),
        ..Default::default()
    };
    client.scrobble(&old, false).unwrap();
    client.scrobble(&new, false).unwrap();
    client.playback_progress(&old, 1.0, false).unwrap();
    client.playback_stopped(&old, 1.0).unwrap();
    client.playback_progress(&new, 2.0, false).unwrap();
    client.playback_stopped(&new, 2.0).unwrap();
    client.playback_stopped(&new, 2.0).unwrap();
    let requests = server.requests();
    let reports = requests
        .iter()
        .filter(|r| r.path.contains("/Sessions/Playing"))
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 4);
    let report: Value = serde_json::from_slice(&reports[3].body).unwrap();
    assert_eq!(report["ItemId"], "new");
}

#[test]
fn rich_track_details_ratings_and_timed_lyrics_retain_protocol_data() {
    use feishin_core::features::StreamOptions;
    let server = Mock::start(|r| {
        if r.path.contains("getSong") {
            ok(
                json!({"song":{"id":"s","title":"Song","userRating":4,"bitRate":320,"samplingRate":48000,"bitDepth":24,"channelCount":2,"suffix":"flac","contentType":"audio/flac","size":123456789u64,"path":"Artist/Album/Song.flac","playCount":42,"musicBrainzId":"mbid"}}),
            )
        } else if r.path.contains("getLyricsBySongId") {
            ok(
                json!({"lyricsList":{"structuredLyrics":[{"kind":"translation","lang":"spa","synced":false,"line":[{"value":"Translation"}]},{"kind":"main","lang":"eng","synced":true,"offset":-100,"cueLine":[{"start":1000,"end":3000,"value":"Hello world","agentId":"lead","cue":[{"start":1000,"end":1800,"value":"Hello "},{"start":1800,"end":3000,"value":"world"}]}]}]}}),
            )
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::Navidrome), "pw").unwrap();
    let details = client.track_details("s").unwrap();
    assert_eq!(details.rating, Some(4));
    assert_eq!(client.ratings().get("s"), Some(&4));
    assert_eq!(details.sample_rate, 48000);
    assert_eq!(details.size_bytes, 123456789);
    assert_eq!(details.play_count, 42);
    client.set_rating("s", 5).unwrap();
    assert_eq!(client.ratings().get("s"), Some(&5));
    assert!(matches!(
        client.set_rating("s", 6),
        Err(Error::InvalidInput(_))
    ));
    let lyrics = client.lyrics_timed(&details.track).unwrap();
    assert_eq!(lyrics.plain_text(), "Hello world");
    assert_eq!(lyrics.offset_ms, -100);
    assert_eq!(lyrics.active_line(1099), None);
    assert_eq!(lyrics.active_line(1100), Some(0));
    assert_eq!(lyrics.lines[0].words[1].start_ms, Some(1800));
    assert_eq!(
        client.stream(&details.track).unwrap().url,
        client
            .stream_with_options(&details.track, &StreamOptions::default())
            .unwrap()
            .url
    );
}

#[test]
fn subsonic_playlist_metadata_reorder_and_radio_crud_are_real_requests() {
    use feishin_core::features::{PlaylistMetadata, RadioStation};
    let server = Mock::start(|r| {
        if r.path.contains("getPlaylist") {
            ok(
                json!({"playlist":{"id":"p","name":"Old","comment":"Description","owner":"alice","public":false,"entry":[subsonic_song("same"),subsonic_song("other"),subsonic_song("same")]}}),
            )
        } else if r.path.contains("getInternetRadioStations") {
            ok(
                json!({"internetRadioStations":{"internetRadioStation":[{"id":"r","name":"Station","streamUrl":"https://radio.example/stream","homePageUrl":"https://radio.example"}]}}),
            )
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::OpenSubsonic), "pw").unwrap();
    let details = client.playlist_details("p").unwrap();
    assert!(details.editable);
    assert_eq!(details.metadata.description, "Description");
    client
        .update_playlist(
            "p",
            &PlaylistMetadata {
                name: "New".into(),
                description: "Changed".into(),
                public: true,
            },
        )
        .unwrap();
    client.move_playlist_track("p", 0, 1).unwrap();
    let requests = server.requests();
    let replace = requests
        .iter()
        .find(|r| r.path.contains("createPlaylist"))
        .unwrap();
    assert_eq!(
        replace
            .url()
            .query_pairs()
            .filter(|(k, _)| k == "songId")
            .map(|(_, v)| v.into_owned())
            .collect::<Vec<_>>(),
        vec!["other", "same", "same"]
    );
    let station = client.radio_stations().unwrap().remove(0);
    assert_eq!(station.homepage_url, "https://radio.example");
    client
        .create_radio(&RadioStation {
            id: String::new(),
            ..station.clone()
        })
        .unwrap();
    client.update_radio(&station).unwrap();
    client.delete_radio(&station.id).unwrap();
    assert!(matches!(
        client.create_radio(&RadioStation {
            stream_url: "file:///etc/passwd".into(),
            ..station
        }),
        Err(Error::InvalidInput(_))
    ));
    assert!(
        server
            .requests()
            .iter()
            .any(|r| r.path.contains("updatePlaylist")
                && r.query("public").as_deref() == Some("true"))
    );
}

#[test]
fn transcoding_maps_codec_bitrate_and_sample_rate_without_leaking_jellyfin_token() {
    use feishin_core::features::{StreamFormat, StreamOptions};
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"private-token","User":{"Id":"user"}}).into()
        } else {
            ok(json!({}))
        }
    });
    let track = Track {
        id: "s".into(),
        ..Default::default()
    };
    let options = StreamOptions {
        format: StreamFormat::Opus,
        max_bitrate_kbps: Some(160),
        sample_rate: None,
    };
    let subsonic = Client::login(server.profile(ServerKind::OpenSubsonic), "pw").unwrap();
    let source = subsonic.stream_with_options(&track, &options).unwrap();
    let url = reqwest::Url::parse(&source.url).unwrap();
    assert!(url.query_pairs().any(|(k, v)| k == "format" && v == "opus"));
    assert!(
        url.query_pairs()
            .any(|(k, v)| k == "maxBitRate" && v == "160")
    );
    let options = StreamOptions {
        sample_rate: Some(48000),
        ..options
    };
    assert!(matches!(
        subsonic.stream_with_options(&track, &options),
        Err(Error::Unsupported)
    ));
    let jellyfin = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let source = jellyfin.stream_with_options(&track, &options).unwrap();
    assert!(source.url.contains("/stream.ogg?"));
    assert!(!source.url.contains("private-token"));
    let url = reqwest::Url::parse(&source.url).unwrap();
    assert!(
        url.query_pairs()
            .any(|(k, v)| k == "AudioBitRate" && v == "160000")
    );
    assert!(
        url.query_pairs()
            .any(|(k, v)| k == "AudioSampleRate" && v == "48000")
    );
    assert!(!jellyfin.capabilities().numeric_ratings);
    assert!(!jellyfin.capabilities().radio);
    assert!(matches!(
        jellyfin.set_rating("s", 3),
        Err(Error::Unsupported)
    ));
    assert!(matches!(jellyfin.radio_stations(), Err(Error::Unsupported)));
}

#[test]
fn jellyfin_play_method_stays_with_active_session_when_preparing_another_source() {
    use feishin_core::features::{StreamFormat, StreamOptions};
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else {
            json!({}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let current = Track {
        id: "current".into(),
        ..Default::default()
    };
    let next = Track {
        id: "next".into(),
        ..Default::default()
    };
    let transcode = StreamOptions {
        format: StreamFormat::Mp3,
        max_bitrate_kbps: Some(128),
        sample_rate: None,
    };
    client.stream(&current).unwrap();
    client.stream_with_options(&next, &transcode).unwrap();
    client.scrobble(&current, false).unwrap();
    client.playback_progress(&current, 1.0, false).unwrap();
    // Repeating the same item with new stream preferences must also leave its
    // already-started session unchanged.
    client.stream_with_options(&current, &transcode).unwrap();
    client.playback_progress(&current, 2.0, false).unwrap();
    client.playback_stopped(&current, 3.0).unwrap();
    client.scrobble(&next, false).unwrap();
    client.stream(&next).unwrap();
    client.playback_progress(&next, 4.0, true).unwrap();
    let reports = server
        .requests()
        .into_iter()
        .filter(|r| r.path.contains("/Sessions/Playing"))
        .map(|r| serde_json::from_slice::<Value>(&r.body).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 6);
    for report in &reports[..3] {
        assert_eq!(report["ItemId"], "current");
        assert_eq!(report["PlayMethod"], "DirectPlay");
        assert_eq!(report["PlaySessionId"], reports[0]["PlaySessionId"]);
    }
    for report in &reports[4..] {
        assert_eq!(report["ItemId"], "next");
        assert_eq!(report["PlayMethod"], "Transcode");
        assert_eq!(report["PlaySessionId"], reports[4]["PlaySessionId"]);
    }
    assert_ne!(reports[0]["PlaySessionId"], reports[4]["PlaySessionId"]);
}

#[test]
fn delayed_jellyfin_start_uses_its_loaded_source_when_same_item_was_resolved_again() {
    use feishin_core::features::{StreamFormat, StreamOptions};
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else {
            json!({}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let track = Track {
        id: "same".into(),
        ..Default::default()
    };
    let direct = client.stream(&track).unwrap();
    let transcode = client
        .stream_with_options(
            &track,
            &StreamOptions {
                format: StreamFormat::Mp3,
                max_bitrate_kbps: Some(128),
                sample_rate: None,
            },
        )
        .unwrap();
    assert!(!direct.transcoded);
    assert!(transcode.transcoded);
    // The first load's Start was delayed until after a repeated queue item
    // resolved with new preferences. Its exact source still determines method.
    client.playback_started(&track, direct.transcoded).unwrap();
    client.playback_progress(&track, 1.0, false).unwrap();
    client.playback_stopped(&track, 2.0).unwrap();
    client.stream(&track).unwrap();
    client
        .playback_started(&track, transcode.transcoded)
        .unwrap();
    client.playback_progress(&track, 3.0, false).unwrap();
    let reports = server
        .requests()
        .into_iter()
        .filter(|request| request.path.contains("/Sessions/Playing"))
        .map(|request| serde_json::from_slice::<Value>(&request.body).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 5);
    for report in &reports[..2] {
        assert_eq!(report["PlayMethod"], "DirectPlay");
        assert_eq!(report["PlaySessionId"], reports[0]["PlaySessionId"]);
    }
    for report in &reports[3..] {
        assert_eq!(report["PlayMethod"], "Transcode");
        assert_eq!(report["PlaySessionId"], reports[3]["PlaySessionId"]);
    }
    assert_ne!(reports[0]["PlaySessionId"], reports[3]["PlaySessionId"]);
}

#[test]
fn jellyfin_playlist_metadata_preserves_unrelated_fields_and_reorder_uses_entry_id() {
    use feishin_core::features::PlaylistMetadata;
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else if r.path.contains("/Items/p") {
            json!({"Id":"p","Name":"Old","Overview":"Description","Genres":["Preserve me"],"ProviderIds":{"Custom":"keep"}}).into()
        } else if r.method == "GET" && r.path.contains("/Playlists/p/Items") {
            json!({"Items":[{"Id":"same","PlaylistItemId":"entry-a"},{"Id":"same","PlaylistItemId":"entry-b"}],"TotalRecordCount":2}).into()
        } else if r.method == "GET" && r.path == "/music/Playlists/p" {
            json!({"OpenAccess":true,"Shares":[{"UserId":"user","CanEdit":true}],"ItemIds":["same","same"]}).into()
        } else {
            json!({}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let details = client.playlist_details("p").unwrap();
    assert!(details.metadata.public);
    assert!(details.editable);
    client
        .update_playlist(
            "p",
            &PlaylistMetadata {
                name: "New".into(),
                description: "New description".into(),
                public: false,
            },
        )
        .unwrap();
    client.move_playlist_track("p", 1, 0).unwrap();
    let requests = server.requests();
    let update = requests
        .iter()
        .find(|r| r.method == "POST" && r.path == "/music/Items/p")
        .unwrap();
    let body: Value = serde_json::from_slice(&update.body).unwrap();
    assert_eq!(body["Genres"][0], "Preserve me");
    assert_eq!(body["ProviderIds"]["Custom"], "keep");
    assert_eq!(body["Overview"], "New description");
    assert!(
        requests
            .iter()
            .any(|r| r.method == "POST" && r.path == "/music/Playlists/p/Items/entry-b/Move/0")
    );
}

#[test]
fn native_smart_playlist_auth_crud_and_rule_validation_preserve_unknown_operators() {
    use feishin_core::features::SmartPlaylist;
    let server = Mock::start(|r| {
        if r.path.contains("/auth/login") {
            json!({"id":"user","token":"native-private-token"}).into()
        } else if r.path.contains("/api/playlist") {
            if r.method == "POST" && r.path == "/music/api/playlist" {
                json!({"id":"smart"}).into()
            } else if r.method == "GET" && r.path.contains("/smart") {
                json!({"id":"smart","name":"Smart","comment":"Old","public":false,"ownerId":"keep-owner","rules":{"all":[{"futureOperator":{"futureField":"value"}}],"sort":"random","limit":5}}).into()
            } else if r.method == "GET" {
                json!([{ "id":"smart","name":"Smart","rules":{"all":[{"gt":{"playCount":-1}}]}}])
                    .into()
            } else {
                json!({"id":"smart"}).into()
            }
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::Navidrome), "pw").unwrap();
    assert!(!client.capabilities().smart_playlists);
    assert!(matches!(
        client.smart_playlist("smart"),
        Err(Error::InvalidInput(_))
    ));
    client.enable_navidrome("pw").unwrap();
    assert!(client.capabilities().smart_playlists);
    let mut playlist = client.smart_playlist("smart").unwrap();
    assert!(playlist.rules_json.contains("futureOperator"));
    playlist.description = "Changed".into();
    client.save_smart_playlist(&playlist).unwrap();
    assert_eq!(client.smart_playlists().unwrap().len(), 1);
    let new = SmartPlaylist {
        id: String::new(),
        ..playlist.clone()
    };
    assert_eq!(client.save_smart_playlist(&new).unwrap(), "smart");
    for rules in [
        "{}",
        "{\"all\":[]}",
        "{\"all\":[{}]}",
        "{\"all\":[{\"gt\":{}}]}",
        "{bad json",
    ] {
        assert!(matches!(
            client.save_smart_playlist(&SmartPlaylist {
                rules_json: rules.into(),
                ..new.clone()
            }),
            Err(Error::InvalidInput(_))
        ));
    }
    client.delete_smart_playlist("smart").unwrap();
    let requests = server.requests();
    let update = requests.iter().find(|r| r.method == "PUT").unwrap();
    let body: Value = serde_json::from_slice(&update.body).unwrap();
    assert_eq!(body["ownerId"], "keep-owner");
    assert_eq!(
        body["rules"]["all"][0]["futureOperator"]["futureField"],
        "value"
    );
    assert!(update.headers.contains("native-private-token"));
    assert!(!update.path.contains("native-private-token"));
    assert!(!format!("{client:?}").contains("native-private-token"));
}

#[test]
fn folder_browsing_distinguishes_numeric_library_ids_directories_and_tracks() {
    let server = Mock::start(|r| {
        if r.path.contains("getMusicFolders") {
            ok(json!({"musicFolders":{"musicFolder":[{"id":1,"name":"Music"}]}}))
        } else if r.path.contains("getIndexes") {
            ok(
                json!({"indexes":{"index":[{"artist":[{"id":"folder-artist","name":"Artist folder"}]}]}}),
            )
        } else if r.path.contains("getMusicDirectory") {
            ok(
                json!({"directory":{"id":"directory","name":"Album folder","parent":"folder-artist","child":[{"id":"nested","title":"Nested folder","isDir":true},subsonic_song("song")]}}),
            )
        } else if r.path.contains("getSimilarSongs2") {
            ok(
                json!({"similarSongs2":{"song":[subsonic_song("seed"),subsonic_song("related"),subsonic_song("related")]}}),
            )
        } else {
            ok(json!({}))
        }
    });
    let client = Client::login(server.profile(ServerKind::OpenSubsonic), "pw").unwrap();
    let folders = client.music_folders().unwrap();
    assert_eq!(folders[0].id, "1");
    let root = client.folder_root(&folders[0].id).unwrap();
    assert!(root.entries[0].is_directory);
    let directory = client.directory(&root.entries[0].id).unwrap();
    assert_eq!(directory.parent_id.as_deref(), Some("folder-artist"));
    assert!(directory.entries[0].track.is_none());
    assert_eq!(directory.entries[1].track.as_ref().unwrap().id, "song");
    let similar = client
        .similar_tracks(
            &Track {
                id: "seed".into(),
                ..Default::default()
            },
            20,
        )
        .unwrap();
    assert_eq!(similar.len(), 1);
    assert_eq!(similar[0].id, "related");
}

#[test]
fn jellyfin_folders_are_nonrecursive_and_album_artists_use_their_own_endpoint() {
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else if r.path.contains("/UserViews") {
            json!({"Items":[{"Id":"music","Name":"Music","CollectionType":"music"},{"Id":"movies","Name":"Movies","CollectionType":"movies"}]}).into()
        } else if r.path.contains("/Items/music") {
            json!({"Id":"music","Name":"Music"}).into()
        } else if r.path.contains("/Artists/AlbumArtists") {
            json!({"Items":[{"Id":"album-artist","Name":"Album artist","UserData":{"IsFavorite":true}}],"TotalRecordCount":1})
                .into()
        } else if r.path.contains("/InstantMix") {
            json!({"Items":[jellyfin_song("related")]}).into()
        } else {
            json!({"Items":[{"Id":"folder","Name":"Nested","IsFolder":true},jellyfin_song("song")],"TotalRecordCount":2}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let folders = client.music_folders().unwrap();
    assert_eq!(folders.len(), 1);
    let directory = client.folder_root(&folders[0].id).unwrap();
    assert!(directory.entries[0].is_directory);
    assert_eq!(directory.entries[1].track.as_ref().unwrap().id, "song");
    let artists = client.album_artists().unwrap();
    assert_eq!(artists[0].id, "album-artist");
    assert!(artists[0].favorite);
    assert!(server.requests().iter().any(|request| {
        request.path.contains("/Artists/AlbumArtists")
            && request.query("EnableUserData").as_deref() == Some("true")
    }));
    assert_eq!(
        client
            .similar_tracks(
                &Track {
                    id: "seed".into(),
                    ..Default::default()
                },
                3
            )
            .unwrap()[0]
            .id,
        "related"
    );
    assert!(
        server
            .requests()
            .iter()
            .any(|r| r.query("Recursive").as_deref() == Some("false")
                && r.query("ParentId").as_deref() == Some("music"))
    );
}

#[test]
fn downloads_stream_original_bytes_and_reject_sink_failures_and_oversized_files() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut requests = 0;
        while requests < 4 && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut byte = [0u8; 1];
                    while bytes.len() < 16384 && stream.read(&mut byte).unwrap_or(0) == 1 {
                        bytes.push(byte[0]);
                        if bytes.ends_with(b"\r\n\r\n") {
                            break;
                        }
                    }
                    let request = String::from_utf8_lossy(&bytes);
                    if requests == 0 {
                        assert!(request.contains("/rest/ping.view?"));
                        let body = json!({"subsonic-response":{"status":"ok"}}).to_string();
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                    } else {
                        assert!(request.contains("/rest/download.view?"));
                        let body = b"\0original audio bytes\0";
                        let length = if requests == 3 {
                            9u64 * 1024 * 1024 * 1024
                        } else {
                            body.len() as u64
                        };
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: audio/flac\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
                        );
                        let _ = stream.write_all(body);
                    }
                    requests += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("fixture listener: {error}"),
            }
        }
        assert_eq!(requests, 4);
    });
    let client = Client::login(
        ServerProfile {
            url: format!("http://{address}"),
            ..Default::default()
        },
        "pw",
    )
    .unwrap();
    let track = Track {
        id: "s".into(),
        ..Default::default()
    };
    let mut bytes = Vec::new();
    let count = client.download(&track, &mut bytes).unwrap();
    assert_eq!(bytes, b"\0original audio bytes\0");
    assert_eq!(count, bytes.len() as u64);
    struct FailingSink;
    impl Write for FailingSink {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("synthetic full disk"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        client.download(&track, &mut FailingSink),
        Err(Error::Write)
    ));
    assert!(matches!(
        client.download(&track, &mut Vec::new()),
        Err(Error::ResponseTooLarge)
    ));
    worker.join().unwrap();
}

#[test]
fn jellyfin_timed_lyrics_convert_ticks_and_utf16_word_positions_safely() {
    let server = Mock::start(|r| {
        if r.path.ends_with("AuthenticateByName") {
            json!({"AccessToken":"token","User":{"Id":"user"}}).into()
        } else {
            json!({"Metadata":{"Language":"eng"},"Lyrics":[{"Start":10000000,"Text":"A🌟 world","Cues":[{"Position":1,"EndPosition":3,"Start":10000000,"End":20000000},{"Position":2,"EndPosition":3,"Start":20000000}]}]}).into()
        }
    });
    let client = Client::login(server.profile(ServerKind::Jellyfin), "pw").unwrap();
    let lyrics = client
        .lyrics_timed(&Track {
            id: "s".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(lyrics.synced);
    assert_eq!(lyrics.language, "eng");
    assert_eq!(lyrics.lines[0].start_ms, Some(1000));
    assert_eq!(lyrics.lines[0].words[0].text, "🌟");
    assert_eq!(lyrics.lines[0].words[0].end_ms, Some(2000));
    assert!(lyrics.lines[0].words[1].text.is_empty());
}
