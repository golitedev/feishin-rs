# Server adapters

`feishin-servers` is a blocking, GUI-independent client. Call it on a worker thread. It implements the public OpenSubsonic protocol for Navidrome/OpenSubsonic servers and a separate Jellyfin adapter. No upstream Feishin implementation has been imported or translated.

## API

`Client::login(ServerProfile, &str)` validates the server address and authenticates. The cloneable client keeps a Subsonic token/salt pair or Jellyfin access token in memory. It does not retain the password. Profile persistence belongs to the desktop app; do not serialize the client or stream sources.

- `library`: paginated albums, songs, artists, playlists, and derived genres.
- `album_tracks`, `artist_albums`, `playlist_tracks`: detail views; playlists retain server order.
- `search`: albums, artists, and songs.
- `set_favorite`: add/remove a track, album, or artist favorite.
- `stream`: playback URL and HTTP headers for the MPV adapter.
- `cover`: artwork bytes, bounded to 8 MiB; decoding belongs on a worker.
- `lyrics`: plaintext lyrics; synchronized timing presentation is not implemented.
- `scrobble`: Subsonic now-playing or completed-play submission; Jellyfin playback-start and natural-end reports sharing a play-session ID.
- `playback_progress`, `playback_stopped`: Jellyfin progress/pause and interrupted-playback reporting at an observed position. The application must call these at the appropriate lifecycle points.
- `create_playlist`, `delete_playlist`, `add_to_playlist`, `remove_playlist_track`: playlist mutations for both protocol families. Removal identifies an occurrence by position so repeated songs remain distinct.

## Transport and limits

Server addresses may include a reverse-proxy base path. Only HTTP/HTTPS addresses without embedded credentials, query strings, or fragments are accepted. HTTPS certificate verification stays enabled. HTTP is allowed for local networks but sends authentication over an unencrypted connection.

Requests have an 8-second connection timeout and a 30-second total timeout. HTTP redirects are rejected to avoid forwarding credentials to another address. Configure the final server URL. JSON responses are bounded to 16 MiB, cover responses to 8 MiB, and paginated collections to 100,000 items. Duplicate IDs across pages, a non-advancing server, and exceeded limits return explicit errors rather than silently returning a partial library.

Subsonic authentication uses MD5(password + random UUID salt), as required by the protocol; it is not used as a general-purpose password-storage scheme. Query authentication is present in Subsonic stream URLs. Jellyfin stream tokens are supplied using the standard `Authorization: MediaBrowser` header. Neither stream sources nor clients reveal credentials through `Debug`; error strings exclude server response bodies and request URLs.

## Known limits

Initial library loading fetches the full normalized collection and can be slow on large servers. Empty-query song listing uses the OpenSubsonic search extension; servers rejecting that extension fall back to traversing albums. Server-specific features, smart-playlist editing, lyrics timing, transcoding selection, API-key authentication are not implemented. These adapters are validated against synthetic HTTP fixtures, not a private live server.

## Primary protocol references

- [OpenSubsonic API reference](https://opensubsonic.netlify.app/docs/api-reference/)
- [OpenSubsonic endpoints](https://opensubsonic.netlify.app/docs/endpoints/)
- [Navidrome Subsonic API documentation](https://www.navidrome.org/docs/developers/subsonic-api/)
- [Jellyfin API reference](https://api.jellyfin.org/)
- [Canonical stable Jellyfin OpenAPI schema](https://api.jellyfin.org/openapi/jellyfin-openapi-stable.json), inspected for endpoint paths, allowed field names, playlist payloads, and playback-report schemas. The adapter uses `/Items?UserId=...`, standard MediaBrowser authorization, and the current favorite route with a compatibility fallback for older servers.
