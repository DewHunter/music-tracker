use anyhow::Result;
use spotify_rs::spotify_api::SpotifyClient;
use tracing::{info, warn, Level};

const USER: &str = "jorge";

/// Depends on the "blocking" feature flags
fn main() -> Result<()> {
    setup_tracing(Level::INFO);
    info!("Running the spotify test cli!");
    let mut spotify = SpotifyClient::new(USER.to_string()).unwrap();
    spotify.setup_creds().unwrap();

    let resp = spotify.get_currently_playing_track()?;
    let track_d = resp.and_then(|t| t.get_track_data());
    match track_d {
        Some(t) => info!("Currently Playing: {}", t.name),
        None => warn!("No track info found"),
    }

    Ok(())
}

fn setup_tracing(level: Level) {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(true)
        .init();
}
