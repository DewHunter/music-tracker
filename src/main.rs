use spotify_rs::local_store::load_bitwarden_data;
use spotify_rs::spotify_api::SpotifyClient;
use tracing::{error, info, Level};

fn main() {
    setup_tracing(Level::DEBUG);
    info!("Running the spotify test cli!");
    // let mut spotify = SpotifyClient::new();
    // spotify.setup_creds();

    // match spotify.get_currently_playing_track() {
    //     Err(e) => {
    //         error!("API Failed with: {e}");
    //     }
    //     Ok(res) => {
    //         info!("Currently Playing Track: {res}");
    //     }
    // }

    let bw = load_bitwarden_data().unwrap();
    bw.list_secrets();
}

fn setup_tracing(level: Level) {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(true)
        .init();
}
