#![cfg(target_os = "linux")]

use feishin_core::{Repeat, Track};
use feishin_desktop::{Desktop, DesktopCommand, DesktopState};
use std::{
    collections::HashMap,
    sync::mpsc::TryRecvError,
    thread,
    time::{Duration, Instant},
};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{OwnedObjectPath, OwnedValue},
};

const NAME: &str = "org.mpris.MediaPlayer2.feishin_rs";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

fn command(desktop: &Desktop) -> DesktopCommand {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match desktop.try_recv() {
            Ok(command) => return command,
            Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(10)),
            Err(TryRecvError::Disconnected) => panic!("desktop worker disconnected"),
        }
    }
    panic!("desktop command did not arrive");
}

#[test]
#[ignore = "run explicitly inside dbus-run-session"]
fn publishes_metadata_properties_signals_and_routes_desktop_commands() {
    let desktop = Desktop::start().unwrap();
    let connection = Connection::session().unwrap();
    let proxy: Proxy<'_> = zbus::blocking::proxy::Builder::new(&connection)
        .destination(NAME)
        .unwrap()
        .path(PATH)
        .unwrap()
        .interface(PLAYER)
        .unwrap()
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .unwrap();
    let root = Proxy::new(&connection, NAME, PATH, "org.mpris.MediaPlayer2").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if proxy.get_property::<String>("PlaybackStatus").is_ok() {
            break;
        }
        if let Ok(error) = desktop.try_recv() {
            panic!("desktop startup: {error:?}");
        }
        assert!(Instant::now() < deadline, "MPRIS service did not register");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        root.get_property::<String>("Identity").unwrap(),
        "Feishin RS"
    );
    assert!(!root.get_property::<bool>("HasTrackList").unwrap());
    let properties =
        Proxy::new(&connection, NAME, PATH, "org.freedesktop.DBus.Properties").unwrap();
    let mut changes = properties.receive_signal("PropertiesChanged").unwrap();
    let state = DesktopState {
        track: Some(Track {
            id: "fixture-track".into(),
            title: "Synthetic track".into(),
            artist: "Fixture artist".into(),
            album: "Fixture album".into(),
            duration: 60.0,
            number: 1,
            ..Track::default()
        }),
        paused: false,
        position: 12.0,
        volume: 75.0,
        can_next: true,
        can_previous: true,
        shuffle: false,
        repeat: Repeat::Off,
    };
    desktop.update(state).unwrap();
    let message = changes.next().expect("missing PropertiesChanged signal");
    let (interface, changed, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
        message.body().deserialize().unwrap();
    assert_eq!(interface, PLAYER);
    assert!(changed.contains_key("Metadata"));
    assert!(!changed.contains_key("Position"));
    assert_eq!(
        proxy.get_property::<String>("PlaybackStatus").unwrap(),
        "Playing"
    );
    assert_eq!(proxy.get_property::<i64>("Position").unwrap(), 12_000_000);
    assert_eq!(proxy.get_property::<f64>("Volume").unwrap(), 0.75);
    let metadata = proxy
        .get_property::<HashMap<String, OwnedValue>>("Metadata")
        .unwrap();
    assert_eq!(
        <&str>::try_from(metadata.get("xesam:title").unwrap()).unwrap(),
        "Synthetic track"
    );
    assert_eq!(
        i64::try_from(metadata.get("mpris:length").unwrap()).unwrap(),
        60_000_000
    );
    let track_id =
        OwnedObjectPath::try_from(metadata.get("mpris:trackid").unwrap().try_clone().unwrap())
            .unwrap();

    for (method, expected) in [
        ("Play", DesktopCommand::Play),
        ("Pause", DesktopCommand::Pause),
        ("PlayPause", DesktopCommand::TogglePause),
        ("Stop", DesktopCommand::Stop),
        ("Next", DesktopCommand::Next),
        ("Previous", DesktopCommand::Previous),
    ] {
        proxy.call_method(method, &()).unwrap();
        assert_eq!(command(&desktop), expected);
    }
    proxy.call_method("Seek", &(-20_000_000_i64,)).unwrap();
    assert_eq!(command(&desktop), DesktopCommand::Seek(0.0));
    proxy
        .call_method("SetPosition", &(track_id, 23_000_000_i64))
        .unwrap();
    assert_eq!(command(&desktop), DesktopCommand::Seek(23.0));
    proxy
        .call_method(
            "SetPosition",
            &(
                OwnedObjectPath::try_from("/old/track").unwrap(),
                4_000_000_i64,
            ),
        )
        .unwrap();
    assert!(matches!(desktop.try_recv(), Err(TryRecvError::Empty)));

    proxy.set_property("Volume", 0.3_f64).unwrap();
    assert_eq!(command(&desktop), DesktopCommand::Volume(30.0));
    proxy.set_property("Shuffle", true).unwrap();
    assert_eq!(command(&desktop), DesktopCommand::Shuffle(true));
    proxy.set_property("LoopStatus", "Track").unwrap();
    assert_eq!(command(&desktop), DesktopCommand::Repeat(Repeat::One));
    assert!(proxy.set_property("LoopStatus", "Invalid").is_err());
    assert!(proxy.set_property("Rate", 2.0_f64).is_err());
    root.call_method("Raise", &()).unwrap();
    assert_eq!(command(&desktop), DesktopCommand::Raise);
    root.call_method("Quit", &()).unwrap();
    assert_eq!(command(&desktop), DesktopCommand::Quit);

    let mut seeks = proxy.receive_signal("Seeked").unwrap();
    desktop.seeked(23.0).unwrap();
    let message = seeks.next().expect("missing Seeked signal");
    let (position,): (i64,) = message.body().deserialize().unwrap();
    assert_eq!(position, 23_000_000);
}
