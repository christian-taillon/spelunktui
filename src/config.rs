use anyhow::{Context, Result};
use directories::ProjectDirs;
use keyring::Entry;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

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
        if let Some(proj_dirs) = ProjectDirs::from("", "", "splunk-tui") {
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
            let service = "splunk-tui";
            let user = "token";
            if let Ok(entry) = Entry::new(service, user) {
                if let Ok(password) = entry.get_password() {
                    config.splunk_token = password;
                }
            }
        }

        // 3. Load from Environment Variables (System/Shell Config)
        // Overrides Config File & Keyring
        if let Ok(val) = env::var("SPLUNK_BASE_URL") {
            config.splunk_base_url = val;
        }
        if let Ok(val) = env::var("SPLUNK_TOKEN") {
            config.splunk_token = val;
        }
        if let Ok(val) = env::var("SPLUNK_VERIFY_SSL") {
            config.splunk_verify_ssl = val.parse().unwrap_or(false);
        }

        // 4. Load from .env file (Project Config) - FORCE OVERRIDE
        // We manually read .env to ensure it overrides stale environment variables
        // which might be set in the shell (common issue).

        let dotenv_path = std::path::Path::new(".env");
        if dotenv_path.exists() {
            info!("Loading .env file from: {:?}", dotenv_path);
            if let Ok(file) = File::open(dotenv_path) {
                let reader = BufReader::new(file);
                for l in reader.lines().map_while(Result::ok) {
                    let l = l.trim();
                    if l.starts_with('#') || l.is_empty() {
                        continue;
                    }
                    if let Some((key, val)) = l.split_once('=') {
                        let key = key.trim();
                        let val = val.trim().trim_matches('"').trim_matches('\''); // Simple unquote

                        match key {
                            "SPLUNK_BASE_URL" => {
                                if config.splunk_base_url != val {
                                    warn!("Overriding SPLUNK_BASE_URL from .env file: '{}' (was '{}')", val, config.splunk_base_url);
                                    config.splunk_base_url = val.to_string();
                                }
                            }
                            "SPLUNK_TOKEN" => {
                                if !val.is_empty() {
                                    config.splunk_token = val.to_string();
                                }
                            }
                            "SPLUNK_VERIFY_SSL" => {
                                config.splunk_verify_ssl = val.parse().unwrap_or(false);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.splunk_base_url.is_empty() {
            anyhow::bail!("SPLUNK_BASE_URL is missing. Run `splunk-tui config` to set it up.");
        }
        if self.splunk_token.is_empty() {
            anyhow::bail!("SPLUNK_TOKEN is missing. Run `splunk-tui config` to set it up.");
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
        if let Some(proj_dirs) = ProjectDirs::from("", "", "splunk-tui") {
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
