use serde::{Deserialize, Serialize};

/// Item returned from Spotify's API: GetCurrentlyPlayingTrack
/// https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-tracka
#[derive(Serialize, Deserialize, Debug)]
pub struct CurrentlyPlayingTrack {
    pub timestamp: u64,
    pub progress_ms: Option<u32>,
    pub currently_playing_type: String,
    pub is_playing: bool,
    // Partially parse to check if this will be a valid track
    pub item: Option<serde_json::Value>,
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
    pub name: String,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Album {
    pub name: String,
    pub id: String,
    pub total_tracks: i32,
    pub release_date: String,
    pub album_type: String,
    pub artists: Vec<Artist>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExternalId {
    pub isrc: Option<String>,
    pub ean: Option<String>,
    pub upc: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Track {
    pub name: String,
    pub id: String,
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
