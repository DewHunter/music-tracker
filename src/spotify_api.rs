use crate::local_store::CredStorage;
use crate::pkce;

use std::io;
use std::time::SystemTime;

use anyhow::{bail, Result};
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use url::Url;

pub const SCOPE: &str = "user-read-playback-state user-read-currently-playing playlist-read-private user-read-playback-position user-top-read user-read-recently-played user-library-read";
const SPOTIFY_AUTH_URL: &str = "https://accounts.spotify.com/authorize";
const SPOTIFY_TOKENS_URL: &str = "https://accounts.spotify.com/api/token";
const SPOTIFY_API_URL: &str = "https://api.spotify.com/v1/me/player";
const CUR_PLAYING_API_PATH: &str = "/currently-playing";
const REDIRECT_URI: &str = "http://localhost:8080";
const CHALLENGE_METHOD: &str = "S256";
const CONTENT_TYPE: &str = "Content-Type";
const CONTENT_TYPE_URL_ENCODED: &str = "application/x-www-form-urlencoded";

#[derive(Serialize, Deserialize, Clone)]
pub struct AppAuthData {
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UserAuthData {
    pub access_token: String,
    // token type is always "Bearer"
    pub token_type: String,
    // A space-separated list of scopes which have been granted for this access_token
    pub scope: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub last_refresh: Option<SystemTime>,
}

pub struct SpotifyClient {
    user_id: String,
    app_client_id: Option<String>,
    user_auth: Option<UserAuthData>,
    creds_storage: CredStorage,
    http_client: Client,
}

impl UserAuthData {
    pub fn token_needs_refresh(&self) -> bool {
        if let Some(last_refresh) = self.last_refresh {
            match last_refresh.elapsed() {
                Ok(elapsed) => {
                    // Adding a 5 second buffer
                    if elapsed.as_secs() < (self.expires_in as u64 - 5) {
                        info!("No need to refresh the access token at this time");
                        return false;
                    }
                }
                Err(e) => {
                    warn!("Can't check time elapsed since last token refresh: {e}");
                }
            }
        }

        true
    }
}

impl SpotifyClient {
    pub fn new(user_id: String) -> Result<SpotifyClient> {
        let creds_storage = CredStorage::new()?;
        Ok(SpotifyClient {
            user_id,
            app_client_id: None,
            user_auth: None,
            creds_storage,
            http_client: Client::new(),
        })
    }

    fn creds_are_loaded(&self) -> bool {
        self.app_client_id.is_some() && self.user_auth.is_some()
    }

    fn access_token(&self) -> String {
        let auth = self.user_auth.as_ref().unwrap();
        auth.access_token.clone()
    }

    fn update_user_auth(&mut self, response: Response) -> Result<()> {
        let mut user_auth_data: UserAuthData = match response.json() {
            Err(_) => {
                bail!("Could not parse response json into a UserAuthData struct");
            }
            Ok(auth) => auth,
        };
        user_auth_data.last_refresh = Some(SystemTime::now());
        self.creds_storage.store_user_auth_data(&user_auth_data, &self.user_id);
        self.user_auth = Some(user_auth_data);

        Ok(())
    }

    /// Checks if access token has expired or is about to expire within 5 seconds.
    /// If so, an attempt is made to refresh the token and store the new values.
    ///
    /// On Error: access token failed to refresh, there was an issue interacting with Spotify's API
    fn refresh_access_token(&mut self) -> Result<()> {
        let app_client_id = self
            .app_client_id
            .clone()
            .expect("Missing app_client_id data");
        let auth = self.user_auth.as_ref().expect("Missing user_auth data");

        if !auth.token_needs_refresh() {
            return Ok(());
        }
        info!("Refreshing API access token");

        let response = self
            .http_client
            .post(SPOTIFY_TOKENS_URL)
            .header(CONTENT_TYPE, CONTENT_TYPE_URL_ENCODED)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &auth.refresh_token),
                ("client_id", &app_client_id),
            ])
            .send();

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                bail!("Problem interacting with Spotify API trying to refresh token: {e}")
            }
        };

        self.update_user_auth(response)
    }

    fn read_spotify_code() -> Option<String> {
        let mut in_buffer = String::new();
        info!("Paste full redirected URL:\n");
        io::stdin().read_line(&mut in_buffer).unwrap();
        let parsed_url = Url::parse(&in_buffer);
        if let Err(e) = parsed_url {
            error!("Invalid input URL/URI, failed parsing {e}");
            return None;
        }

        get_code_from_query_pairs(parsed_url.unwrap())
    }

    pub fn setup_creds(&mut self) -> Result<()> {
        let client_id = self.creds_storage.load_app_auth_data()?.client_id;
        self.app_client_id = Some(client_id.clone());
        self.user_auth = self.creds_storage.load_user_auth_data(&self.user_id);

        if self.creds_are_loaded() {
            let _ = self.refresh_access_token()?;
            info!("Spotify API creds are ready to go");
            return Ok(());
        }

        warn!("We need to generate auth tokens from Spotify, starting now");

        // Step 1: Auth with Spotify
        let code_verifier = pkce::generate_code_verifier();
        let code_challenge = pkce::encode_s256(&code_verifier);
        let url = Url::parse_with_params(
            SPOTIFY_AUTH_URL,
            &[
                ("response_type", "code"),
                ("client_id", &client_id),
                ("scope", SCOPE),
                ("code_challenge_method", CHALLENGE_METHOD),
                ("code_challenge", &code_challenge),
                ("redirect_uri", REDIRECT_URI),
            ],
        )?;
        info!("Paste this into your browser to auth this app: \n{}", url);

        // Step 2: User must input code/state into this CLI
        let spotify_auth_code = match Self::read_spotify_code() {
            None => bail!("Could not get user input"),
            Some(c) => c,
        };
        info!("Parsed auth code: {}", spotify_auth_code);

        // Step 3: Ask spotify for an access token using the code
        let response = self
            .http_client
            .post(SPOTIFY_TOKENS_URL)
            .header(CONTENT_TYPE, CONTENT_TYPE_URL_ENCODED)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &spotify_auth_code),
                ("client_id", &client_id),
                ("code_verifier", &String::from_utf8(code_verifier)?),
                ("redirect_uri", REDIRECT_URI),
            ])
            .send();

        debug!("Full Response from Spotify: {:?}", response);

        let resp = response?;
        self.update_user_auth(resp)
    }

    pub fn get_currently_playing_track(&self) -> Result<String> {
        if !self.creds_are_loaded() {
            bail!("Creds are misconfigured, cannot execute API");
        }
        let access_token = self.access_token();
        let api_url = format!("{SPOTIFY_API_URL}{CUR_PLAYING_API_PATH}");
        let request = self.http_client.get(api_url).bearer_auth(access_token);
        debug!("Full request to Spotify: {:?}", request);
        let response = request.send();
        debug!("Full Response from Spotify: {:?}", response);
        if let Err(e) = response {
            bail!("Problem calling Spotify API: {e}");
        }
        let payload = response?;
        let body = payload.text()?;
        Ok(body)
    }
}

fn get_code_from_query_pairs(url: Url) -> Option<String> {
    let mut qpairs = url.query_pairs();
    while let Some((k, v)) = qpairs.next() {
        if k.eq("error") {
            let issue = v;
            error!("Auth process encountered an issue {}", issue);
            return None;
        }
        if k.eq("code") {
            debug!("Successfully found code in url");
            return Some(String::from(v));
        }
    }

    debug!("Did not find code or error in parsed url");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getting_code_from_params() {
        let url = String::from("http://localhost:8080/?code=AQAJQs0ZXTxhvkRUMXn1PVLQQBw2VXSldRqfou5RPM_RPkHdexx7v7lUNcjXjWzPKFW3bxxPLuHCJqoQy6NbIr-70-ZpPszqktjxBgzqqmKLv653gjh_f_-ELVPdWscUvlNlICrcyUGtGPCIIdDLWHg9bVEsBMFtyrEtA8S6bYoUbC-3YhqhNr6GC90rM3AmmTUqhTC2jkINQ9aFMCalO2l34NLE9kXqIVe2hBMaEdOuBNfi3zXhdG0kulgAJ8a03nAVMs9HBJXKFzD5bVFvl7eXj3p6DwMOnQFxFJq9wJHbg57a507DPmVr8vO_nYRcr6uXhVgMEY4WkR0djj3CgeKSUNOVGB-VwUs8YcyZH-kfaUoeOsY-6hyiDUizDPGXorL0vskU7GmTGsat2UwsSkanGeJvr3BP9-GVVIQFcU91WNiG2rkAa8rIWJz_EgRtqco7yA");
        let url = Url::parse(&url).unwrap();
        let spotify_auth_code = get_code_from_query_pairs(url);
        assert_eq!(spotify_auth_code, Some(String::from("AQAJQs0ZXTxhvkRUMXn1PVLQQBw2VXSldRqfou5RPM_RPkHdexx7v7lUNcjXjWzPKFW3bxxPLuHCJqoQy6NbIr-70-ZpPszqktjxBgzqqmKLv653gjh_f_-ELVPdWscUvlNlICrcyUGtGPCIIdDLWHg9bVEsBMFtyrEtA8S6bYoUbC-3YhqhNr6GC90rM3AmmTUqhTC2jkINQ9aFMCalO2l34NLE9kXqIVe2hBMaEdOuBNfi3zXhdG0kulgAJ8a03nAVMs9HBJXKFzD5bVFvl7eXj3p6DwMOnQFxFJq9wJHbg57a507DPmVr8vO_nYRcr6uXhVgMEY4WkR0djj3CgeKSUNOVGB-VwUs8YcyZH-kfaUoeOsY-6hyiDUizDPGXorL0vskU7GmTGsat2UwsSkanGeJvr3BP9-GVVIQFcU91WNiG2rkAa8rIWJz_EgRtqco7yA")));
    }

    #[test]
    fn test_system_time_parsing() {
        let string =
            String::from("{\"secs_since_epoch\":1726602033,\"nanos_since_epoch\":365022800}");
        let systime: serde_json::error::Result<SystemTime> = serde_json::from_str(&string);
        assert!(systime.is_ok());
    }
}
