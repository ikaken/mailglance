use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Settings saved as JSON in the app data folder.
/// Refresh tokens are not here; they live in the Windows Credential Manager.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccountConfig {
    pub email: String,
    /// Label ids the user pinned to the label column.
    #[serde(default)]
    pub favorites: Vec<String>,
}

impl Config {
    pub fn load(path: &Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn account_mut(&mut self, email: &str) -> Option<&mut AccountConfig> {
        self.accounts.iter_mut().find(|a| a.email == email)
    }
}
