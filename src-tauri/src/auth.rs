use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug)]
struct AppConfig {
    pub master_key: String,
}

fn get_config_path() -> Option<PathBuf> {
    let base_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
        })
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join(".routingall"))
        })?;

    let dir = base_dir.join("routingall");
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("config.json"))
}

fn load_persisted_key() -> Option<String> {
    let path = get_config_path()?;
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                if config.master_key.starts_with("rg-master-key-") && config.master_key.len() > 14 {
                    return Some(config.master_key);
                }
            }
        }
    }
    None
}

fn save_persisted_key(key: &str) {
    if let Some(path) = get_config_path() {
        let config = AppConfig {
            master_key: key.to_string(),
        };
        if let Ok(content) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(path, content);
        }
    }
}

#[derive(Clone)]
pub struct AuthState {
    master_key: Arc<RwLock<String>>,
}

impl AuthState {
    pub fn new() -> Self {
        let key = load_persisted_key().unwrap_or_else(|| {
            let new_k = format!("rg-master-key-{}", Uuid::new_v4().simple());
            save_persisted_key(&new_k);
            new_k
        });
        Self {
            master_key: Arc::new(RwLock::new(key)),
        }
    }

    pub fn get_master_key(&self) -> String {
        self.master_key.read().unwrap().clone()
    }

    pub fn rotate_master_key(&self) -> String {
        let new_key = format!("rg-master-key-{}", Uuid::new_v4().simple());
        let mut key_writer = self.master_key.write().unwrap();
        *key_writer = new_key.clone();
        save_persisted_key(&new_key);
        new_key
    }

    pub fn validate_bearer_token(&self, token: &str) -> bool {
        let clean_token = token.trim_start_matches("Bearer ").trim();
        let current_key = self.master_key.read().unwrap();
        clean_token == *current_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_validation() {
        let auth = AuthState::new();
        let key = auth.get_master_key();
        assert!(key.starts_with("rg-master-key-"));
        assert!(auth.validate_bearer_token(&key));
        assert!(auth.validate_bearer_token(&format!("Bearer {}", key)));
        assert!(!auth.validate_bearer_token("invalid-key"));
    }

    #[test]
    fn test_rotate_master_key() {
        let auth = AuthState::new();
        let key1 = auth.get_master_key();
        let key2 = auth.rotate_master_key();
        assert_ne!(key1, key2);
        assert!(auth.validate_bearer_token(&key2));
        assert!(!auth.validate_bearer_token(&key1));
    }
}
