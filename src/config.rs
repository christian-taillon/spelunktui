use anyhow::{Context, Result};
use directories::ProjectDirs;
use keyring::Entry;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Config {
    pub splunk_base_url: String,
    pub splunk_token: String,
    pub splunk_verify_ssl: bool,
    pub theme: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut config = Config::default();

        // 1. Load from Config File (Global)
        if let Some(proj_dirs) = ProjectDirs::from("", "", "spelunktui") {
            let config_dir = proj_dirs.config_dir();
            let config_path = config_dir.join("config.toml");

            if config_path.exists() {
                info!("Loading config from: {:?}", config_path);
                let content = std::fs::read_to_string(&config_path)
                    .context(format!("Failed to read config file at {:?}", config_path))?;

                // Handle parsing errors gracefully
                match toml::from_str::<FileConfig>(&content) {
                    Ok(file_config) => config.merge(file_config),
                    Err(e) => warn!("Failed to parse config.toml: {}", e),
                }
            }
        }

        // 2. Load from Keyring (if token is missing)
        if config.splunk_token.is_empty() {
            let service = "spelunktui";
            let user = "token";
            if let Ok(entry) = Entry::new(service, user) {
                if let Ok(password) = entry.get_password() {
                    config.splunk_token = password;
                }
            }
        }

        // 3. Load from Environment Variables (System/Shell Config)
        // These override Config File & Keyring for flexibility
        if let Ok(val) = env::var("SPLUNK_BASE_URL") {
            config.splunk_base_url = val;
        }
        if let Ok(val) = env::var("SPLUNK_TOKEN") {
            config.splunk_token = val;
        }
        if let Ok(val) = env::var("SPLUNK_VERIFY_SSL") {
            config.splunk_verify_ssl = val.parse().unwrap_or(false);
        }

        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.splunk_base_url.is_empty() {
            anyhow::bail!("Splunk Base URL is not configured.\nRun 'spelunktui config' to set up your credentials.");
        }
        if self.splunk_token.is_empty() {
            anyhow::bail!("Splunk Token is not configured.\nRun 'spelunktui config' to set up your credentials.");
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Default)]
pub struct FileConfig {
    pub splunk_base_url: Option<String>,
    pub splunk_token: Option<String>,
    pub splunk_verify_ssl: Option<bool>,
    pub theme: Option<String>,
}

impl Config {
    fn merge(&mut self, other: FileConfig) {
        if let Some(v) = other.splunk_base_url {
            self.splunk_base_url = v;
        }
        if let Some(v) = other.splunk_token {
            self.splunk_token = v;
        }
        if let Some(v) = other.splunk_verify_ssl {
            self.splunk_verify_ssl = v;
        }
        if let Some(v) = other.theme {
            self.theme = Some(v);
        }
    }

    pub fn save_theme(theme_name: &str) -> Result<()> {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "spelunktui") {
            let config_dir = proj_dirs.config_dir();
            std::fs::create_dir_all(config_dir)?;
            let config_path = config_dir.join("config.toml");

            // Read existing or create new
            let mut file_config: FileConfig = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                toml::from_str(&content).unwrap_or_default()
            } else {
                FileConfig::default()
            };

            file_config.theme = Some(theme_name.to_string());
            let toml_string = toml::to_string(&file_config)?;
            std::fs::write(config_path, toml_string)?;
        }
        Ok(())
    }
}
