use serde::Deserialize;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use directories::ProjectDirs;
use anyhow::{Result, Context};
use log::{info, warn};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub splunk_base_url: String,
    pub splunk_token: String,
    pub splunk_verify_ssl: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut config = Config::default();

        if let Some(proj_dirs) = ProjectDirs::from("", "", "splunk-tui") {
            let config_dir = proj_dirs.config_dir();
            let config_path = config_dir.join("config.toml");
            
            if config_path.exists() {
                info!("Loading config from: {:?}", config_path);
                let content = std::fs::read_to_string(&config_path)
                    .context(format!("Failed to read config file at {:?}", config_path))?;
                
                let file_config: FileConfig = toml::from_str(&content)
                    .context("Failed to parse config.toml")?;
                
                config.merge(file_config);
            }
        }

        // 2. Load from Environment Variables (System/Shell Config)
        // Standard priority: Env vars first.
        if let Ok(val) = env::var("SPLUNK_BASE_URL") { config.splunk_base_url = val; }
        if let Ok(val) = env::var("SPLUNK_TOKEN") { config.splunk_token = val; }
        if let Ok(val) = env::var("SPLUNK_VERIFY_SSL") {
            config.splunk_verify_ssl = val.parse().unwrap_or(false);
        }

        // 3. Load from .env file (Project Config) - FORCE OVERRIDE
        // We manually read .env to ensure it overrides stale environment variables
        // which might be set in the shell (common issue).
        
        let dotenv_path = std::path::Path::new(".env");
        if dotenv_path.exists() {
             info!("Loading .env file from: {:?}", dotenv_path);
             if let Ok(file) = File::open(dotenv_path) {
                 let reader = BufReader::new(file);
                 for line in reader.lines() {
                     if let Ok(l) = line {
                         let l = l.trim();
                         if l.starts_with('#') || l.is_empty() { continue; }
                         if let Some((key, val)) = l.split_once('=') {
                             let key = key.trim();
                             let val = val.trim().trim_matches('"').trim_matches('\''); // Simple unquote
                             
                             match key {
                                 "SPLUNK_BASE_URL" => {
                                     if config.splunk_base_url != val {
                                         warn!("Overriding SPLUNK_BASE_URL from .env file: '{}' (was '{}')", val, config.splunk_base_url);
                                         config.splunk_base_url = val.to_string();
                                     }
                                 },
                                 "SPLUNK_TOKEN" => {
                                     if !val.is_empty() {
                                         config.splunk_token = val.to_string();
                                     }
                                 },
                                 "SPLUNK_VERIFY_SSL" => {
                                     config.splunk_verify_ssl = val.parse().unwrap_or(false);
                                 },
                                 _ => {}
                             }
                         }
                     }
                 }
             }
        }

        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.splunk_base_url.is_empty() {
            anyhow::bail!("SPLUNK_BASE_URL is missing.");
        }
        if self.splunk_token.is_empty() {
            anyhow::bail!("SPLUNK_TOKEN is missing.");
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            splunk_base_url: String::new(),
            splunk_token: String::new(),
            splunk_verify_ssl: false,
        }
    }
}

#[derive(Deserialize)]
struct FileConfig {
    splunk_base_url: Option<String>,
    splunk_token: Option<String>,
    splunk_verify_ssl: Option<bool>,
}

impl Config {
    fn merge(&mut self, other: FileConfig) {
        if let Some(v) = other.splunk_base_url { self.splunk_base_url = v; }
        if let Some(v) = other.splunk_token { self.splunk_token = v; }
        if let Some(v) = other.splunk_verify_ssl { self.splunk_verify_ssl = v; }
    }
}

