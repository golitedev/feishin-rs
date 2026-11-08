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
    json!({"Id":id,"Name":"Example track","Artists":["Artist"],"ArtistItems":[{"Id":"ar","Name":"Artist"}],"Album":"Album","AlbumId":"a","RunTimeTicks":1230000000u64,"IndexNumber":2,"ParentIndexNumber":1,"ProductionYear":2026,"Genres":["Jazz"],"UserData":{"IsFavorite":true}})
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
                json!({"artists":{"index":[{"name":"A","artist":[{"id":"ar","name":"Artist","albumCount":201}]}]}}),
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
    client.scrobble(&track, false).unwrap();
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
