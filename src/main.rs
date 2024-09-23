use spotify_rs::spotify_api::SpotifyClient;
use tracing::{error, info, Level};

const USER: &str = "jorge";

/// Depends on the "blocking" feature flags
fn main() {
    setup_tracing(Level::INFO);
    info!("Running the spotify test cli!");
    let mut spotify = SpotifyClient::new(USER.to_string()).unwrap();
    spotify.setup_creds().unwrap();

    match spotify.get_currently_playing_track() {
        Err(e) => {
            error!("API Failed with: {e}");
        }
        Ok(res) => {
            info!("Currently Playing Track: {res}");
        }
    }
}

fn setup_tracing(level: Level) {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(true)
        .init();
}
