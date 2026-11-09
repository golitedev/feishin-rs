//! Explicit checks against a disposable Navidrome fixture, never a private music library.
use feishin_core::features::{
    PlaylistMetadata, RadioStation, SmartPlaylist, StreamFormat, StreamOptions,
};
use feishin_core::{ServerKind, ServerProfile};
use feishin_servers::Client;
use std::time::Duration;

struct Cleanup {
    client: Client,
    playlists: Vec<String>,
    radios: Vec<String>,
    rating: Option<(String, u8)>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        for id in &self.playlists {
            let _ = self.client.delete_playlist(id);
        }
        for id in &self.radios {
            let _ = self.client.delete_radio(id);
        }
        if let Some((id, rating)) = &self.rating {
            let _ = self.client.set_rating(id, *rating);
        }
    }
}

#[test]
#[ignore = "mutates a disposable Navidrome fixture; set FEISHIN_TEST_URL/USER/PASSWORD explicitly"]
fn live_advanced_server_workflows_round_trip() {
    let url =
        std::env::var("FEISHIN_TEST_URL").expect("set FEISHIN_TEST_URL for a disposable server");
    let username =
        std::env::var("FEISHIN_TEST_USER").expect("set FEISHIN_TEST_USER for a disposable server");
    let password = std::env::var("FEISHIN_TEST_PASSWORD")
        .expect("set FEISHIN_TEST_PASSWORD for a disposable server");
    let client = Client::login(
        ServerProfile {
            name: "Disposable parity fixture".into(),
            url,
            username,
            kind: ServerKind::Navidrome,
        },
        &password,
    )
    .unwrap();
    let mut cleanup = Cleanup {
        client: client.clone(),
        playlists: vec![],
        radios: vec![],
        rating: None,
    };
    let library = client.library().unwrap();
    assert!(
        library.tracks.len() >= 2,
        "fixture needs at least two original synthetic tracks"
    );
    let first = &library.tracks[0];
    let second = &library.tracks[1];
    let details = client.track_details(&first.id).unwrap();
    assert!(details.size_bytes > 0);
    assert!(details.sample_rate > 0);
    assert!(!details.codec.is_empty());
    cleanup.rating = Some((first.id.clone(), details.rating.unwrap_or(0)));
    client.set_rating(&first.id, 4).unwrap();
    assert_eq!(client.track_details(&first.id).unwrap().rating, Some(4));
    assert_eq!(client.ratings().get(&first.id), Some(&4));
    let lyric_track = library
        .tracks
        .iter()
        .find(|track| {
            client
                .lyrics_timed(track)
                .is_ok_and(|lyrics| lyrics.synced && !lyrics.lines.is_empty())
        })
        .expect("fixture needs one synchronized LRC track");
    let lyrics = client.lyrics_timed(lyric_track).unwrap();
    assert!(lyrics.lines.iter().any(|line| line.start_ms.is_some()));
    assert!(!lyrics.plain_text().is_empty());
    let playlist = client
        .create_playlist(
            "feishin-rs disposable advanced fixture",
            &[first.id.clone(), second.id.clone(), first.id.clone()],
        )
        .unwrap();
    cleanup.playlists.push(playlist.clone());
    client
        .update_playlist(
            &playlist,
            &PlaylistMetadata {
                name: "feishin-rs renamed fixture".into(),
                description: "Original synthetic fixture only".into(),
                public: false,
            },
        )
        .unwrap();
    let list = client.playlist_details(&playlist).unwrap();
    assert_eq!(list.metadata.name, "feishin-rs renamed fixture");
    assert_eq!(list.metadata.description, "Original synthetic fixture only");
    assert!(!list.metadata.public);
    assert!(list.editable);
    client.move_playlist_track(&playlist, 0, 1).unwrap();
    let reordered = client.playlist_tracks(&playlist).unwrap();
    assert_eq!(
        reordered.iter().map(|track| &track.id).collect::<Vec<_>>(),
        vec![&second.id, &first.id, &first.id]
    );
    let mut bytes = Vec::new();
    let count = client.download(first, &mut bytes).unwrap();
    assert_eq!(count, details.size_bytes);
    assert_eq!(bytes.len() as u64, count);
    let stream = client
        .stream_with_options(
            first,
            &StreamOptions {
                format: StreamFormat::Mp3,
                max_bitrate_kbps: Some(128),
                sample_rate: None,
            },
        )
        .unwrap();
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap()
        .get(stream.url)
        .send()
        .unwrap();
    assert!(response.status().is_success());
    let encoded = response.bytes().unwrap();
    assert!(!encoded.is_empty());
    assert_ne!(encoded.as_ref(), bytes.as_slice());
    let mut radio = RadioStation {
        id: String::new(),
        name: "feishin-rs disposable radio fixture".into(),
        stream_url: "http://127.0.0.1:14533/synthetic-radio".into(),
        homepage_url: "https://example.org/".into(),
    };
    client.create_radio(&radio).unwrap();
    radio = client
        .radio_stations()
        .unwrap()
        .into_iter()
        .find(|station| station.name == radio.name)
        .unwrap();
    cleanup.radios.push(radio.id.clone());
    radio.name = "feishin-rs renamed radio fixture".into();
    client.update_radio(&radio).unwrap();
    assert!(
        client
            .radio_stations()
            .unwrap()
            .iter()
            .any(|station| station.id == radio.id && station.name == radio.name)
    );
    client.enable_navidrome(&password).unwrap();
    assert!(client.capabilities().smart_playlists);
    let mut smart = SmartPlaylist {
        id: String::new(),
        name: "feishin-rs disposable smart fixture".into(),
        description: "Original fixture rules".into(),
        public: false,
        rules_json: r#"{"all":[{"gt":{"playCount":-1}}],"sort":"random","limit":3}"#.into(),
    };
    smart.id = client.save_smart_playlist(&smart).unwrap();
    cleanup.playlists.push(smart.id.clone());
    let loaded = client.smart_playlist(&smart.id).unwrap();
    assert!(loaded.rules_json.contains("playCount"));
    assert!(client.playlist_details(&smart.id).unwrap().smart);
    smart.name = "feishin-rs updated smart fixture".into();
    smart.rules_json = r#"{"all":[{"gt":{"playCount":-1}}],"sort":"title","limit":2}"#.into();
    client.save_smart_playlist(&smart).unwrap();
    let updated = client.smart_playlist(&smart.id).unwrap();
    assert_eq!(updated.name, smart.name);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&updated.rules_json).unwrap()["limit"],
        2
    );
    assert!(
        client
            .smart_playlists()
            .unwrap()
            .iter()
            .any(|playlist| playlist.id == smart.id)
    );
    assert_eq!(client.playlist_tracks(&smart.id).unwrap().len(), 2);
    let folders = client.music_folders().unwrap();
    assert!(!folders.is_empty());
    let root = client.folder_root(&folders[0].id).unwrap();
    assert!(!root.entries.is_empty());
    let artist = root
        .entries
        .iter()
        .find(|entry| entry.is_directory)
        .unwrap();
    let artist_folder = client.directory(&artist.id).unwrap();
    assert!(!artist_folder.entries.is_empty());
    let album = artist_folder
        .entries
        .iter()
        .find(|entry| entry.is_directory)
        .unwrap();
    assert!(
        client
            .directory(&album.id)
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.track.is_some())
    );
    assert!(!client.album_artists().unwrap().is_empty());
    client.delete_smart_playlist(&smart.id).unwrap();
    cleanup.playlists.retain(|id| id != &smart.id);
    client.delete_playlist(&playlist).unwrap();
    cleanup.playlists.retain(|id| id != &playlist);
    client.delete_radio(&radio.id).unwrap();
    cleanup.radios.clear();
}
