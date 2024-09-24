use serde::{Deserialize, Serialize};

/// Item returned from Spotify's API: GetCurrentlyPlayingTrack
/// https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-tracka
#[derive(Serialize, Deserialize, Debug)]
pub struct CurrentlyPlayingTrack {
    timestamp: u64,
    progress_ms: Option<u32>,
    currently_playing_type: String,
    is_playing: bool,
    // Partially parse to check if this will be a valid track
    item: Option<serde_json::Value>,
}

impl CurrentlyPlayingTrack {
    pub fn get_track_data(&self) -> Option<Track> {
        self.item
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Artist {
    name: String,
    id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Album {
    name: String,
    id: String,
    total_tracks: i32,
    release_date: String,
    album_type: String,
    artists: Vec<Artist>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExternalId {
    isrc: Option<String>,
    ean: Option<String>,
    upc: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Track {
    pub name: String,
    pub album: Album,
    pub artists: Vec<Artist>,
    pub disc_number: i32,
    pub duration_ms: u32,
    pub external_ids: ExternalId,
    pub explicit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_currently_playing() {
        let full_response =
            std::fs::read_to_string("sample_data/currently_playing_track.json").unwrap();
        let res: CurrentlyPlayingTrack = serde_json::from_str(&full_response).unwrap();
        assert_eq!(res.currently_playing_type, "track");
        let track: Track = serde_json::from_value(res.item.unwrap()).unwrap();
        println!("parsed track: {track:?}");
        assert!(false);
    }
}
