use crate::spotify_api::{AppAuthData, UserAuthData};

use std::{fs, fs::OpenOptions};
use std::io::Write;
use tracing::{error, warn};

const APP_AUTH_DATA: &str = "app_auth.json";
const LOCAL_USER_AUTH_DATA: &str = "user_auth.json";

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
    use std::time::SystemTime;
    use super::*;

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
        fs::remove_file(LOCAL_USER_AUTH_DATA);
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
        fs::remove_file(LOCAL_USER_AUTH_DATA);
        assert!(!fs::exists(LOCAL_USER_AUTH_DATA).unwrap());
    }
}