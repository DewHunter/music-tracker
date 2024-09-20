use crate::spotify_api::{AppAuthData, UserAuthData};

use anyhow::Result;
use bitwarden::secrets_manager::secrets::SecretGetRequest;
use serde::Deserialize;
use std::io::Write;
use std::{fs, fs::OpenOptions};
use tracing::info;
use tracing::{error, warn};
use uuid::Uuid;

use bitwarden::{
    auth::login::AccessTokenLoginRequest, secrets_manager::secrets::SecretIdentifiersRequest,
    secrets_manager::secrets::SecretResponse, secrets_manager::ClientSecretsExt, Client,
};

const BITWARDEN_CONFIG: &str = "bitwarden_config.json";
const APP_AUTH_DATA: &str = "app_auth.json";
const LOCAL_USER_AUTH_DATA: &str = "user_auth.json";

#[derive(Deserialize)]
pub struct BitwardenConfig {
    pub access_token: String,
    org_id: Uuid,
}

pub fn load_bitwarden_data() -> Result<BitwardenConfig> {
    let bitwarden_data = fs::read_to_string(BITWARDEN_CONFIG)?;
    let config: BitwardenConfig = serde_json::from_str(&bitwarden_data)?;
    Ok(config)
}

impl BitwardenConfig {
    pub fn list_secrets(&self) {
        let client = Client::new(None);
        let token = AccessTokenLoginRequest {
            access_token: self.access_token.clone(),
            state_file: None,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let res = rt
            .block_on(async { client.auth().login_access_token(&token).await })
            .unwrap();
        info!("BW Auth Result {:?}", res);

        let org_id = SecretIdentifiersRequest {
            organization_id: self.org_id.clone(),
        };
        let res = rt
            .block_on(async { client.secrets().list(&org_id).await })
            .unwrap();
        info!("List Secrets: {:?}", res);
        let secret_md = res.data.get(0).unwrap();

        let get_secret = SecretGetRequest { id: secret_md.id };
        let res: SecretResponse = rt
            .block_on(async { client.secrets().get(&get_secret).await })
            .unwrap();

        info!(
            "Shhhhh: key: {} value: {} Note: {}",
            res.key, res.value, res.note
        );
    }
}

pub fn load_app_auth_data() -> Option<AppAuthData> {
    match fs::exists(APP_AUTH_DATA) {
        Err(_) => {
            error!("Couldn't search for a local file, it is probably a permissions issue.");
            return None;
        }
        Ok(false) => {
            warn!("Looks like the app auth file doesn't exist, please create it 🙏!");
            return None;
        }
        Ok(true) => {}
    }
    let app_auth_data = fs::read_to_string(APP_AUTH_DATA).unwrap();
    if let Ok(data) = serde_json::from_str(&app_auth_data) {
        Some(data)
    } else {
        error!("App auth file is probably corrupted, please recreate it.");
        None
    }
}

pub fn load_user_auth_data() -> Option<UserAuthData> {
    match fs::exists(LOCAL_USER_AUTH_DATA) {
        Err(_) => {
            error!("Couldn't search for a local file, it is probably a permissions issue.");
            return None;
        }
        Ok(false) => {
            warn!("Looks like the user auth file doesn't exist yet");
            return None;
        }
        Ok(true) => {}
    }
    let user_auth_data = fs::read_to_string(LOCAL_USER_AUTH_DATA).unwrap();
    if let Ok(data) = serde_json::from_str(&user_auth_data) {
        Some(data)
    } else {
        error!("User auth file is probably corrupted");
        None
    }
}

pub fn store_user_auth_data(data: &UserAuthData) {
    if let Ok(j) = serde_json::to_string(data) {
        let mut f_user_auth = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(LOCAL_USER_AUTH_DATA)
            .unwrap();
        f_user_auth.write(j.as_bytes()).unwrap();
    } else {
        error!("Failed to translate UserAuthData into json");
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn check_files() {
        match fs::exists(LOCAL_USER_AUTH_DATA) {
            Ok(true) => {
                panic!("ERROR: Cannot run test, it delete your current data!");
            }
            _ => {}
        }
    }

    #[test]
    fn test_store_user_auth() {
        check_files();
        let user_auth = UserAuthData {
            access_token: String::from("AAA"),
            token_type: String::from("Bearer"),
            scope: String::from("user-can-fk-themselves"),
            expires_in: 42,
            last_refresh: Some(SystemTime::now()),
            refresh_token: String::from("BBBB"),
        };
        store_user_auth_data(&user_auth);
        assert!(fs::exists(LOCAL_USER_AUTH_DATA).unwrap());
        let _ = fs::remove_file(LOCAL_USER_AUTH_DATA);
        assert!(!fs::exists(LOCAL_USER_AUTH_DATA).unwrap());
    }

    #[test]
    fn test_store_user_auth_already_exists() {
        check_files();
        let user_auth = UserAuthData {
            access_token: String::from("AAA"),
            token_type: String::from("Bearer"),
            scope: String::from("user-can-fk-themselves"),
            expires_in: 42,
            last_refresh: Some(SystemTime::now()),
            refresh_token: String::from("BBBB"),
        };
        store_user_auth_data(&user_auth);
        assert!(fs::exists(LOCAL_USER_AUTH_DATA).unwrap());
        let user_auth = UserAuthData {
            access_token: String::from("XXX"),
            token_type: String::from("Bearer"),
            scope: String::from("user-can-fk-themselves"),
            expires_in: 42,
            last_refresh: Some(SystemTime::now()),
            refresh_token: String::from("CCCC"),
        };
        store_user_auth_data(&user_auth);
        assert!(fs::exists(LOCAL_USER_AUTH_DATA).unwrap());
        // TODO: assert file contents changed
        let _ = fs::remove_file(LOCAL_USER_AUTH_DATA);
        assert!(!fs::exists(LOCAL_USER_AUTH_DATA).unwrap());
    }
}
