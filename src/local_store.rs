use crate::spotify_api::{self, AppAuthData, UserAuthData};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::time::SystemTime;
use std::{fs, fs::OpenOptions};
#[cfg(feature = "blocking")]
use tokio::runtime::Runtime;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use bitwarden::secrets_manager::secrets::{
    SecretCreateRequest, SecretGetRequest, SecretIdentifiersRequest, SecretPutRequest,
    SecretResponse,
};
use bitwarden::{auth::login::AccessTokenLoginRequest, secrets_manager::ClientSecretsExt, Client};

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
    project_id: Uuid,
}

#[derive(Serialize, Deserialize, Default)]
pub struct RefreshNote {
    pub expires_in: i64,
    pub last_refresh: Option<SystemTime>,
}

pub struct CredStorage {
    org_id: SecretIdentifiersRequest,
    project_id: Uuid,
    #[cfg(feature = "blocking")]
    rt: Runtime,
    bw_client: Client,
}

fn load_bitwarden_data() -> Result<BitwardenCreds> {
    let bitwarden_data = fs::read_to_string(BITWARDEN_CONFIG)?;
    let config: BitwardenCreds = serde_json::from_str(&bitwarden_data)?;
    Ok(config)
}

impl CredStorage {
    fn start_storage_setup() -> Result<(
        SecretIdentifiersRequest,
        Uuid,
        Client,
        AccessTokenLoginRequest,
    )> {
        let creds = load_bitwarden_data()?;
        let access_token = creds.access_token;
        let org_id = creds.org_id;
        let project_id = creds.project_id;

        let bw_client = Client::new(None);
        let token = AccessTokenLoginRequest {
            access_token,
            state_file: None,
        };

        let org_id = SecretIdentifiersRequest {
            organization_id: org_id,
        };

        Ok((org_id, project_id, bw_client, token))
    }

    #[cfg(feature = "blocking")]
    pub fn new() -> Result<CredStorage> {
        let (org_id, project_id, bw_client, token) = Self::start_storage_setup()?;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        let _ = rt.block_on(async { bw_client.auth().login_access_token(&token).await })?;

        Ok(CredStorage {
            org_id,
            project_id,
            rt,
            bw_client,
        })
    }

    #[cfg(not(feature = "blocking"))]
    pub async fn new() -> Result<CredStorage> {
        let (org_id, project_id, bw_client, token) = Self::start_storage_setup()?;

        bw_client.auth().login_access_token(&token).await?;

        Ok(CredStorage {
            org_id,
            project_id,
            bw_client,
        })
    }

    async fn list_secrets(&self) -> Result<HashMap<String, Uuid>> {
        let res = self.bw_client.secrets().list(&self.org_id).await?;
        debug!("List Secrets: {:?}", res);
        let data = res.data;
        let secrets: HashMap<String, Uuid> = data
            .iter()
            .map(|secret| (secret.key.clone(), secret.id))
            .collect();

        Ok(secrets)
    }

    /// Gien the name of a secret, also named a key, we look for it in
    /// secrets manager and return a tuple of the secret value and note.
    async fn get_secret(&self, key: &str) -> Result<(String, String)> {
        let secrets_md = self.list_secrets().await?;
        let id = match secrets_md.get(key) {
            Some(id) => id,
            None => bail!("Secret key <{key}> does not exist in bitwarden"),
        };

        let get_secret = SecretGetRequest { id: id.clone() };
        let res: SecretResponse = self.bw_client.secrets().get(&get_secret).await?;
        debug!("Get Secret: {:?}", res);

        Ok((res.value, res.note))
    }

    async fn put_secret(&self, key: &str, value: &str, note: Option<String>) -> Result<()> {
        let secrets_md = self.list_secrets().await?;
        let id = match secrets_md.get(key) {
            Some(id) => id,
            None => {
                warn!("Secret key <{key}> does not exist in bitwarden, we will try to create it");
                let create_request = SecretCreateRequest {
                    organization_id: self.org_id.organization_id,
                    key: key.to_string(),
                    value: value.to_string(),
                    note: note.unwrap_or(String::new()),
                    project_ids: Some(vec![self.project_id]),
                };
                let res: SecretResponse = self.bw_client.secrets().create(&create_request).await?;
                debug!("Create Secret Response: {:?}", res);
                debug!("Successfully created secret <{key}> in bitwarden");
                return Ok(());
            }
        };

        let put_request = SecretPutRequest {
            id: *id,
            organization_id: self.org_id.organization_id,
            key: key.to_string(),
            value: value.to_string(),
            note: note.unwrap_or(String::new()),
            project_ids: Some(vec![self.project_id]),
        };
        let res: SecretResponse = self.bw_client.secrets().update(&put_request).await?;
        debug!("Update Secret Response: {:?}", res);
        debug!("Successfully updated secret <{key}>");
        Ok(())
    }

    #[cfg(feature = "blocking")]
    pub fn load_app_auth_data(&self) -> Result<AppAuthData> {
        Ok(self
            .rt
            .block_on(async { self.load_app_auth_data_async().await })?)
    }

    #[cfg(not(feature = "blocking"))]
    pub async fn load_app_auth_data(&self) -> Result<AppAuthData> {
        self.load_app_auth_data_async().await
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
    async fn load_app_auth_data_async(&self) -> Result<AppAuthData> {
        if let Ok(data) = load_json_data(APP_AUTH_DATA) {
            info!("Using AppAuthData found in local json file");
            return Ok(data);
        }
        info!("Did not find {APP_AUTH_DATA} with usable data, fetching from bitwarden");

        let (app_id, _) = self.get_secret(BW_SPOTIFY_APP_CLIENTID_KEY).await?;
        let app_data = AppAuthData {
            client_id: app_id,
            client_secret: None,
        };

        if let Err(e) = store_json_data(APP_AUTH_DATA, &app_data) {
            warn!("Problem writting data into a file: {e}");
        };

        Ok(app_data)
    }

    #[cfg(feature = "blocking")]
    pub fn load_user_auth_data(&self, user_id: &str) -> Option<UserAuthData> {
        self.rt
            .block_on(async { self.load_user_auth_data_async(user_id).await })
    }

    #[cfg(not(feature = "blocking"))]
    pub async fn load_user_auth_data(&self, user_id: &str) -> Option<UserAuthData> {
        self.load_user_auth_data_async(user_id).await
    }

    /// Loads an UserAuthData struct.
    /// The first attempt is using a local json file,
    /// if that fails, we can construct one using the remote value
    /// stored in Bitwarden Secrets Manager.
    ///
    /// Returns Err if bitwarden fails to respond or if it fails to
    /// write the json data file.
    async fn load_user_auth_data_async(&self, user_id: &str) -> Option<UserAuthData> {
        let mut local_data = None;
        if let Ok(data) = load_json_data::<UserAuthData>(LOCAL_USER_AUTH_DATA) {
            if !data.token_needs_refresh() {
                return Some(data);
            }
            warn!("User auth data from file is expired, will check bitwarden");
            local_data = Some(data);
        }

        let refresh = self
            .get_secret(&format!("{BW_SPOTIFY_REFRESH_KEY}_{user_id}"))
            .await;
        debug!("Response from fetching refresh key: {refresh:?}");

        let (refresh_tok, note) = match refresh {
            Err(_) => {
                return local_data;
            }
            Ok(tuple) => tuple,
        };

        if local_data
            .as_ref()
            .is_some_and(|l| refresh_tok == l.refresh_token)
        {
            debug!("Found user auth data locally that matches secrets manager");
            return local_data;
        }
        warn!("Found user auth data locally and in bitwarden but they don't match");

        let (access_tok, _) = match self
            .get_secret(&format!("{BW_SPOTIFY_TOKEN_KEY}_{user_id}"))
            .await
        {
            Err(e) => {
                debug!("There was an error fetching spotify access token: {e}");
                warn!("Did not find access token in bitwarden, but we did find a refresh token");
                (String::new(), String::new())
            }
            Ok(tup) => tup,
        };

        let refresh_note = serde_json::from_str(&note).unwrap_or(RefreshNote::default());

        Some(UserAuthData {
            access_token: access_tok,
            refresh_token: refresh_tok,
            token_type: "Bearer".to_string(),
            scope: spotify_api::SCOPE.to_string(),
            // We don't know when was the last refresh
            expires_in: refresh_note.expires_in,
            last_refresh: refresh_note.last_refresh,
        })
    }

    #[cfg(feature = "blocking")]
    pub fn store_user_auth_data(&self, user_auth: &UserAuthData, user_id: &str) {
        self.rt
            .block_on(async { self.store_user_auth_data_async(user_auth, user_id).await });
    }

    #[cfg(not(feature = "blocking"))]
    pub async fn store_user_auth_data(&self, user_auth: &UserAuthData, user_id: &str) {
        self.store_user_auth_data_async(user_auth, user_id).await;
    }

    async fn store_user_auth_data_async(&self, user_auth: &UserAuthData, user_id: &str) {
        if let Err(e) = store_json_data(LOCAL_USER_AUTH_DATA, user_auth) {
            warn!("Failed to write User auth data file: {e}");
        }
        debug!("Storing UserAuthData into bitwarden");
        if let Err(e) = self
            .put_secret(
                &format!("{BW_SPOTIFY_REFRESH_KEY}_{user_id}"),
                &user_auth.refresh_token,
                make_refresh_note(user_auth),
            )
            .await
        {
            error!("Failed to write refresh token into bitwarden {e}");
        }
        if let Err(e) = self
            .put_secret(
                &format!("{BW_SPOTIFY_TOKEN_KEY}_{user_id}"),
                &user_auth.access_token,
                make_refresh_note(user_auth),
            )
            .await
        {
            error!("Failed to write refresh token into bitwarden: {e}");
        }
    }
}

fn make_refresh_note(data: &UserAuthData) -> Option<String> {
    data.last_refresh.and_then(|ts| {
        let note = RefreshNote {
            expires_in: data.expires_in,
            last_refresh: Some(ts),
        };
        serde_json::to_string(&note).ok()
    })
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
