use crate::spotify_api::{self, AppAuthData, UserAuthData};

use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Write;
use std::{fs, fs::OpenOptions};
use tokio::runtime::Runtime;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use bitwarden::{
    auth::login::AccessTokenLoginRequest, secrets_manager::secrets::SecretGetRequest,
    secrets_manager::secrets::SecretIdentifiersRequest, secrets_manager::secrets::SecretResponse,
    secrets_manager::ClientSecretsExt, Client,
};

const BITWARDEN_CONFIG: &str = "bitwarden_config.json";
const APP_AUTH_DATA: &str = "app_auth.json";
const LOCAL_USER_AUTH_DATA: &str = "user_auth.json";

const BW_SPOTIFY_APP_CLIENTID_KEY: &str = "spotify_client_id";
const BW_SPOTIFY_TOKEN_KEY: &str = "spotify_access_token";
const BW_SPOTIFY_REFRESH_KEY: &str = "spotify_refresh_token";

#[derive(Deserialize)]
struct BitwardenCreds {
    access_token: String,
    org_id: Uuid,
}

pub struct CredStorage {
    org_id: SecretIdentifiersRequest,
    rt: Runtime,
    bw_client: Client,
}

fn load_bitwarden_data() -> Result<BitwardenCreds> {
    let bitwarden_data = fs::read_to_string(BITWARDEN_CONFIG)?;
    let config: BitwardenCreds = serde_json::from_str(&bitwarden_data)?;
    Ok(config)
}

impl CredStorage {
    pub fn new() -> Result<CredStorage> {
        let creds = load_bitwarden_data()?;
        let access_token = creds.access_token;
        let org_id = creds.org_id;

        let bw_client = Client::new(None);
        let token = AccessTokenLoginRequest {
            access_token,
            state_file: None,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let _ = rt.block_on(async { bw_client.auth().login_access_token(&token).await })?;
        let org_id = SecretIdentifiersRequest {
            organization_id: org_id,
        };

        Ok(CredStorage {
            org_id,
            rt,
            bw_client,
        })
    }

    fn list_secrets(&self) -> Result<HashMap<String, Uuid>> {
        let res = self
            .rt
            .block_on(async { self.bw_client.secrets().list(&self.org_id).await })
            .unwrap();
        debug!("List Secrets: {:?}", res);
        let data = res.data;
        let secrets: HashMap<String, Uuid> = data
            .iter()
            .map(|secret| (secret.key.clone(), secret.id))
            .collect();

        Ok(secrets)
    }

    fn get_secret(&self, key: &str) -> Result<String> {
        let secrets_md = self.list_secrets()?;
        let id = match secrets_md.get(key) {
            Some(id) => id,
            None => bail!("Secret key <{key}> does not exist in bitwarden"),
        };

        let get_secret = SecretGetRequest { id: id.clone() };
        let res: SecretResponse = self
            .rt
            .block_on(async { self.bw_client.secrets().get(&get_secret).await })?;
        debug!("Get Secret: {:?}", res);

        Ok(res.value)
    }

    /// Loads an AppAuthData struct.
    /// First we look for the app auth data in a local file, if that fails,
    /// we look for a value in bitwarden.
    /// If we find the value in bitwarden, we save it to a file.
    ///
    /// Client App id should be written into secrets manager, this value rarely changes.
    ///
    /// Returns Err if bitwarden fails to respond or if it fails to
    /// write the json data file.
    pub fn load_app_auth_data(&self) -> Result<AppAuthData> {
        if let Ok(data) = load_json_data(APP_AUTH_DATA) {
            return Ok(data);
        }

        let app_id = self.get_secret(BW_SPOTIFY_APP_CLIENTID_KEY)?;
        let app_data = AppAuthData {
            client_id: app_id,
            client_secret: None,
        };

        if let Err(e) = store_json_data(APP_AUTH_DATA, &app_data) {
            warn!("Problem writting data into a file: {e}");
        };

        Ok(app_data)
    }

    /// Loads an UserAuthData struct.
    /// The first attempt is using a local json file,
    /// if that fails, we can construct one using the remote value
    /// stored in Bitwarden Secrets Manager.
    ///
    /// Returns Err if bitwarden fails to respond or if it fails to
    /// write the json data file.
    pub fn load_user_auth_data(&self, user_id: &str) -> Option<UserAuthData> {
        let mut local_data = None;
        if let Ok(data) = load_json_data::<UserAuthData>(LOCAL_USER_AUTH_DATA) {
            if !data.token_needs_refresh() {
                return Some(data);
            }
            info!("User auth data from file is expired, will check bitwarden");
            local_data = Some(data);
        }

        let refresh = self.get_secret(&format!("{BW_SPOTIFY_REFRESH_KEY}_{user_id}"));
        // If we find data locally and remotely, and the refresh tokens match
        // then we can assume all the data is the same, and return the local value
        // If we find both and the values don't match, we will favor the ones remotely
        // they are more likely to be current.
        if let (Ok(bw_tok), Some(local)) = (refresh.as_ref(), local_data.as_ref()) {
            if *bw_tok == local.refresh_token {
                info!("Found user auth data locally that matches secrets manager");
                return local_data;
            }
            warn!("Found user auth data locally and in bitwarden but they don't match");
        } else if refresh.is_err() {
            // If local and remote are missing, this will be None
            return local_data;
        }

        let refresh = refresh.unwrap();
        let token = match self.get_secret(&format!("{BW_SPOTIFY_TOKEN_KEY}_{user_id}")) {
            Err(_) => {
                warn!("Did not find access token in bitwarden, but we did find a refresh token");
                String::new()
            }
            Ok(tok) => tok,
        };

        Some(UserAuthData {
            access_token: token,
            refresh_token: refresh,
            token_type: "Bearer".to_string(),
            scope: spotify_api::SCOPE.to_string(),
            // We don't know when was the last refresh
            expires_in: 0,
            last_refresh: None,
        })
    }

    pub fn store_user_auth_data(&self, user_auth: &UserAuthData) {
        match store_json_data(LOCAL_USER_AUTH_DATA, user_auth) {
            Err(_) => warn!("Failed to write User auth data file"),
            _ => {}
        }
        // TODO: Store access and refresh tokens in bitwarden
    }
}

fn load_json_data<D>(file_name: &str) -> Result<D>
where
    D: serde::de::DeserializeOwned,
{
    if fs::exists(file_name).is_err() {
        error!("Failed search for a local file, it is probably a permissions issue.");
        bail!("Error while checking if file exists");
    };
    let data_str = fs::read_to_string(file_name)?;
    let data: D = serde_json::from_str(&data_str)?;
    Ok(data)
}

/// Stores the given Serializable struct as json into the
/// given file name. Any existing file will be completely
/// overwritten, and a missing file will be created.
///
/// It just stores it in the local working directory of the binary
/// running.
fn store_json_data<D>(file_name: &str, data: &D) -> Result<()>
where
    D: serde::Serialize,
{
    let j = serde_json::to_string(&data)?;
    let mut app_file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(file_name)?;
    let _ = app_file.write(j.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_file(filename: &str) {
        match fs::exists(filename) {
            Ok(true) => {
                panic!("ERROR: Cannot run test, it will delete your current data!");
            }
            _ => {}
        }
    }

    #[test]
    fn test_load_json_data_but_file_is_missing() {
        let file = "random_file.json";
        check_file(&file);
        let auth_data: Result<AppAuthData> = load_json_data(&file);
        assert!(auth_data.is_err());
    }
}
