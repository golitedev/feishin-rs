use super::*;
use feishin_core::features::{Directory, FolderEntry, MusicFolder};

impl Client {
    pub fn music_folders(&self) -> Result<Vec<MusicFolder>> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic("getMusicFolders", &[])?;
                let folders = array(
                    value.get("musicFolders").ok_or(Error::InvalidResponse)?,
                    "musicFolder",
                );
                if folders.len() > MAX_ITEMS {
                    return Err(Error::LibraryLimit);
                }
                folders
                    .iter()
                    .map(|folder| {
                        Ok(MusicFolder {
                            id: identifier(folder, "id")?,
                            name: text(folder, "name"),
                        })
                    })
                    .collect()
            }
            Auth::Jellyfin { user_id, .. } => {
                let value = match self
                    .jellyfin_get(self.url(&["UserViews"])?, &[("UserId", user_id.clone())])
                {
                    Err(Error::Http(404)) => {
                        self.jellyfin_get(self.url(&["Users", user_id, "Views"])?, &[])?
                    }
                    result => result?,
                };
                array(&value, "Items")
                    .iter()
                    .filter(|item| text(item, "CollectionType") == "music")
                    .map(|item| {
                        Ok(MusicFolder {
                            id: required_text(item, "Id")?,
                            name: text(item, "Name"),
                        })
                    })
                    .collect()
            }
        }
    }

    /// The initial directory of a music folder; Subsonic roots use getIndexes.
    pub fn folder_root(&self, folder_id: &str) -> Result<Directory> {
        match self.auth {
            Auth::Subsonic { .. } => {
                let value =
                    self.subsonic("getIndexes", &[("musicFolderId", folder_id.to_owned())])?;
                let indexes = value.get("indexes").ok_or(Error::InvalidResponse)?;
                let mut entries = array(indexes, "index")
                    .iter()
                    .flat_map(|group| array(group, "artist"))
                    .map(|artist| {
                        Ok(FolderEntry {
                            id: identifier(artist, "id")?,
                            name: text(artist, "name"),
                            is_directory: true,
                            ..Default::default()
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                entries.extend(
                    array(indexes, "child")
                        .iter()
                        .map(subsonic_entry)
                        .collect::<Result<Vec<_>>>()?,
                );
                if entries.len() > MAX_ITEMS {
                    return Err(Error::LibraryLimit);
                }
                Ok(Directory {
                    id: folder_id.to_owned(),
                    name: "Music folders".into(),
                    entries,
                    parent_id: None,
                })
            }
            Auth::Jellyfin { .. } => self.directory(folder_id),
        }
    }

    pub fn directory(&self, id: &str) -> Result<Directory> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic("getMusicDirectory", &[("id", id.to_owned())])?;
                let folder = value.get("directory").ok_or(Error::InvalidResponse)?;
                let children = array(folder, "child");
                if children.len() > MAX_ITEMS {
                    return Err(Error::LibraryLimit);
                }
                Ok(Directory {
                    id: identifier(folder, "id")?,
                    name: text(folder, "name"),
                    parent_id: folder.get("parent").and_then(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .or_else(|| v.as_u64().map(|id| id.to_string()))
                    }),
                    entries: children
                        .iter()
                        .map(subsonic_entry)
                        .collect::<Result<Vec<_>>>()?,
                })
            }
            Auth::Jellyfin { user_id, .. } => {
                let value =
                    self.jellyfin_get(self.url(&["Items", id])?, &[("UserId", user_id.clone())])?;
                let items = self.jellyfin_pages(
                    self.url(&["Items"])?,
                    &[
                        ("UserId", user_id.clone()),
                        ("ParentId", id.to_owned()),
                        ("Recursive", "false".into()),
                        ("SortBy", "IsFolder,SortName".into()),
                        ("Fields", jellyfin_fields().into()),
                    ],
                )?;
                let entries = items
                    .iter()
                    .map(|item| {
                        let is_directory = item
                            .get("IsFolder")
                            .and_then(Value::as_bool)
                            .unwrap_or_else(|| text(item, "Type") != "Audio");
                        Ok(FolderEntry {
                            id: required_text(item, "Id")?,
                            name: text(item, "Name"),
                            is_directory,
                            cover_id: jellyfin_cover(item),
                            track: if is_directory {
                                None
                            } else {
                                Some(jellyfin_track(item)?)
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Directory {
                    id: id.to_owned(),
                    name: text(&value, "Name"),
                    parent_id: value
                        .get("ParentId")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    entries,
                })
            }
        }
    }

    /// Album-artist roles are separate in Jellyfin; Subsonic exposes its ID3 artist index.
    pub fn album_artists(&self) -> Result<Vec<Artist>> {
        match &self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic("getArtists", &[])?;
                let artists = value.get("artists").ok_or(Error::InvalidResponse)?;
                array(artists, "index")
                    .iter()
                    .flat_map(|group| array(group, "artist"))
                    .map(subsonic_artist)
                    .collect()
            }
            Auth::Jellyfin { user_id, .. } => self
                .jellyfin_pages(
                    self.url(&["Artists", "AlbumArtists"])?,
                    &[
                        ("UserId", user_id.clone()),
                        ("Fields", jellyfin_fields().into()),
                        ("EnableUserData", "true".into()),
                    ],
                )?
                .iter()
                .map(jellyfin_artist)
                .collect(),
        }
    }

    /// Server recommendations for Auto DJ; may depend on server-side integrations.
    pub fn similar_tracks(&self, track: &Track, count: u32) -> Result<Vec<Track>> {
        let count = count.clamp(1, 200);
        let result = match &self.auth {
            Auth::Subsonic { .. } => {
                let value = self.subsonic(
                    "getSimilarSongs2",
                    &[("id", track.id.clone()), ("count", count.to_string())],
                )?;
                array(
                    value.get("similarSongs2").ok_or(Error::InvalidResponse)?,
                    "song",
                )
                .iter()
                .take(count as usize)
                .map(subsonic_track)
                .collect::<Result<Vec<_>>>()?
            }
            Auth::Jellyfin { user_id, .. } => {
                let value = self.jellyfin_get(
                    self.url(&["Items", &track.id, "InstantMix"])?,
                    &[
                        ("UserId", user_id.clone()),
                        ("Limit", count.to_string()),
                        ("Fields", jellyfin_fields().into()),
                    ],
                )?;
                array(&value, "Items")
                    .iter()
                    .take(count as usize)
                    .map(jellyfin_track)
                    .collect::<Result<Vec<_>>>()?
            }
        };
        let mut seen = HashSet::new();
        Ok(result
            .into_iter()
            .filter(|candidate| candidate.id != track.id && seen.insert(candidate.id.clone()))
            .collect())
    }
}

fn identifier(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(|value| {
            value
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .or_else(|| value.as_u64().map(|value| value.to_string()))
        })
        .ok_or(Error::InvalidResponse)
}
fn subsonic_entry(value: &Value) -> Result<FolderEntry> {
    let is_directory = value.get("isDir").and_then(Value::as_bool).unwrap_or(false);
    Ok(FolderEntry {
        id: identifier(value, "id")?,
        name: if text(value, "title").is_empty() {
            text(value, "name")
        } else {
            text(value, "title")
        },
        is_directory,
        cover_id: text(value, "coverArt"),
        track: if is_directory {
            None
        } else {
            Some(subsonic_track(value)?)
        },
    })
}
