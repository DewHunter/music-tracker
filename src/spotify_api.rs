use crate::local_store::{load_app_auth_data, load_user_auth_data, store_user_auth_data};
use crate::pkce;

use core::result::Result;
use std::time::SystemTime;
use std::io;

use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use url::Url;

const SCOPE: &str = "user-read-playback-state user-read-currently-playing playlist-read-private user-read-playback-position user-top-read user-read-recently-played user-library-read";
const SPOTIFY_AUTH_URL:  &str = "https://accounts.spotify.com/authorize";
const SPOTIFY_TOKENS_URL: &str = "https://accounts.spotify.com/api/token";
const SPOTIFY_API_URL: &str = "https://api.spotify.com/v1/me/player";
const CUR_PLAYING_API_PATH: &str = "/currently-playing";
const REDIRECT_URI: &str = "http://localhost:8080";
const CHALLENGE_METHOD: &str = "S256";
const CONTENT_TYPE: &str = "Content-Type";
const CONTENT_TYPE_URL_ENCODED: &str = "application/x-www-form-urlencoded";

#[derive(Serialize, Deserialize, Clone)]
pub struct AppAuthData {
    client_id: String,
    client_secret: Option<String>,
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
    app_auth: Option<AppAuthData>,
    user_auth: Option<UserAuthData>,
    http_client: Client,
}

impl SpotifyClient {
    pub fn new() -> SpotifyClient {
        SpotifyClient {
            app_auth: None,
            user_auth: None,
            http_client: Client::new(),
        }
    }

    fn creds_are_loaded(&self) -> bool {
        self.app_auth.is_some() && self.user_auth.is_some()
    }

    fn access_token(&self) -> String {
        let auth = self.user_auth.as_ref().unwrap();
        auth.access_token.clone()
    }

    fn update_user_auth(&mut self, response: Response) -> Result<(), &str> {
        let mut user_auth_data: UserAuthData = match response.json() {
            Err(_) => {
                return Err("Could not parse response json into a UserAuthData struct");
            },
            Ok(auth) => auth,
        };
        user_auth_data.last_refresh = Some(SystemTime::now());
        store_user_auth_data(&user_auth_data);
        self.user_auth = Some(user_auth_data);

        Ok(())
    }

    /// Checks if access token has expired or is about to expire within 5 seconds.
    /// If so, an attempt is made to refresh the token and store the new values.
    fn refresh_access_token(&mut self) -> Result<(), &str> {
        let app = self.app_auth.as_ref().ok_or("Missing app_auth data")?;
        let auth = self.user_auth.as_ref().ok_or("Missing user_auth data")?;

        if let Some(last_refresh) = auth.last_refresh {
            match last_refresh.elapsed() {
                Ok(elapsed) => {
                    // Adding a 5 second buffer
                    if elapsed.as_secs() < (auth.expires_in as u64 - 5) {
                        info!("No need to refresh the access token at this time");
                        return Ok(());
                    }
                }
                Err(e) => {
                    warn!("Can't check time elapsed since last token refresh: {e}");
                }
            }
        }
        info!("Refreshing API access token");

        let response = self.http_client.post(SPOTIFY_TOKENS_URL)
            .header(CONTENT_TYPE, CONTENT_TYPE_URL_ENCODED)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &auth.refresh_token),
                ("client_id", &app.client_id),
            ])
            .send();

        let response = match response {
            Ok(resp) => resp,
            Err(_) => return Err("Reqwest response is error"),
        };

        match self.update_user_auth(response) {
            Err(e) => {
                error!("There was a problem {e}");
            }
            _ => {}
        }

        Ok(())
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

    pub fn setup_creds(&mut self) {
        let app_auth = load_app_auth_data();
        if app_auth.is_none() {
            panic!("Critical app auth data cannot be loaded");
        } else {
            self.app_auth = app_auth;
        }
        self.user_auth = load_user_auth_data();
        if self.creds_are_loaded() {
            match self.refresh_access_token() {
                Err(e) => {
                    error!("Problem while refreshing access tokens: {e}, please delete user_auth.json and get brand-new creds");
                }
                _ => {}
            }
            debug!("Spotify API creds all-good");
            return;
        }

        // Step 1: Auth with Spotify
        let code_verifier = pkce::generate_code_verifier();
        let code_challenge = pkce::encode_s256(&code_verifier);
        let url = Url::parse_with_params(
            SPOTIFY_AUTH_URL,
            &[
                ("response_type", "code"),
                ("client_id", &self.app_auth.clone().unwrap().client_id),
                ("scope", SCOPE),
                ("code_challenge_method", CHALLENGE_METHOD),
                ("code_challenge", &code_challenge),
                ("redirect_uri", REDIRECT_URI)
            ]
        ).unwrap();
        info!("Paste this into your browser to auth this app: \n{}", url);

        // Step 2: User must input code/state into this CLI
        let spotify_auth_code = match Self::read_spotify_code() {
            None => return,
            Some(c) => c,
        };
        info!("Parsed auth code: {}", spotify_auth_code);

        // Step 3: Ask spotify for an access token using the code
        let response = self.http_client.post(SPOTIFY_TOKENS_URL)
            .header(CONTENT_TYPE, CONTENT_TYPE_URL_ENCODED)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &spotify_auth_code),
                ("client_id", &self.app_auth.clone().unwrap().client_id),
                ("code_verifier", &String::from_utf8(code_verifier).unwrap()),
                ("redirect_uri", REDIRECT_URI)
            ])
            .send();

        debug!("Full Response from Spotify: {:?}", response);

        if response.is_err() {
            error!("There was an error while getting spotify's auth token");
            return;
        } else if let Ok(resp) = response {
            match self.update_user_auth(resp) {
                Err(e) => {
                    error!("There was a problem {e}");
                }
                _ => {}
            }
        }
    }

    pub fn get_currently_playing_track(&self) -> Result<String, &str> {
        if !self.creds_are_loaded() {
            return Err("Creds are misconfigured, cannot execute API");
        }
        let access_token = self.access_token();
        let api_url = format!("{SPOTIFY_API_URL}{CUR_PLAYING_API_PATH}");
        let request = self.http_client.get(api_url).bearer_auth(access_token);
        debug!("Full request to Spotify: {:?}", request);
        let response = request.send();
        debug!("Full Response from Spotify: {:?}", response);
        if response.is_err() {
            return Err("There was an error while getting spotify's auth token");
        }
        let payload= response.unwrap();
        let body = payload.text().unwrap();
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
        let string = String::from("{\"secs_since_epoch\":1726602033,\"nanos_since_epoch\":365022800}");
        let systime: serde_json::error::Result<SystemTime> = serde_json::from_str(&string);
        assert!(systime.is_ok());
    }
}