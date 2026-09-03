use keyring::Entry;
use std::sync::{Arc, RwLock};

const SERVICE_NAME: &str = "routingall";

#[derive(Clone)]
pub struct KeychainAdapter {
    // In-memory cache fallback if keychain operation is denied or during dev
    cache: Arc<RwLock<std::collections::HashMap<String, String>>>,
}

impl KeychainAdapter {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub fn validate_format(provider: &str, key: &str) -> Option<&'static str> {
        match provider.to_lowercase().as_str() {
            "groq" => {
                if !key.starts_with("gsk_") {
                    return Some("Key pattern mismatch: Groq keys typically start with 'gsk_'");
                }
            }
            "gemini" | "google" => {
                if !key.starts_with("AIzaSy") {
                    return Some("Key pattern mismatch: Google AI Studio keys typically start with 'AIzaSy'");
                }
            }
            "openrouter" => {
                if !key.starts_with("sk-or-v1-") {
                    return Some("Key pattern mismatch: OpenRouter keys typically start with 'sk-or-v1-'");
                }
            }
            _ => {}
        }
        None
    }

    pub fn set_key(&self, provider: &str, key: &str) -> Result<(), String> {
        let entry_name = format!("key_{}", provider.to_lowercase());
        let entry = Entry::new(SERVICE_NAME, &entry_name);
        
        match entry {
            Ok(e) => {
                let _ = e.set_password(key);
            }
            Err(_) => {}
        }

        // Always update cache
        self.cache.write().unwrap().insert(entry_name, key.to_string());
        Ok(())
    }

    pub fn get_key(&self, provider: &str) -> Result<String, String> {
        let entry_name = format!("key_{}", provider.to_lowercase());
        
        // Check cache first
        if let Some(cached) = self.cache.read().unwrap().get(&entry_name) {
            return Ok(cached.clone());
        }

        // Fallback to OS Keychain
        let entry = Entry::new(SERVICE_NAME, &entry_name)
            .map_err(|e| format!("Keychain lookup failed: {}", e))?;
        
        entry.get_password().map_err(|_| format!("No API key configured for provider: {}", provider))
    }

    pub fn has_key(&self, provider: &str) -> bool {
        self.get_key(provider).is_ok()
    }

    pub fn remove_all_keys(&self) -> Result<(), String> {
        let providers = ["groq", "gemini", "openrouter"];
        for p in providers {
            let entry_name = format!("key_{}", p);
            if let Ok(entry) = Entry::new(SERVICE_NAME, &entry_name) {
                let _ = entry.delete_password();
            }
        }
        self.cache.write().unwrap().clear();
        Ok(())
    }
}
