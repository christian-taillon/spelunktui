use crate::config::FileConfig;
use anyhow::Result;
use directories::ProjectDirs;
use keyring::Entry;
use std::io::{self, Write};

pub fn run() -> Result<()> {
    println!("Welcome to splunk-tui configuration wizard!");
    println!("This wizard will help you set up your configuration.");
    println!();

    // 1. SPLUNK_BASE_URL
    print!("Enter Splunk Base URL: ");
    io::stdout().flush()?;
    let mut base_url = String::new();
    io::stdin().read_line(&mut base_url)?;
    let base_url = base_url.trim().to_string();

    // 2. SPLUNK_TOKEN
    print!("Enter Splunk Token (hidden): ");
    io::stdout().flush()?;
    let token = rpassword::read_password()?;
    let token = token.trim().to_string();

    // 3. SPLUNK_VERIFY_SSL
    print!("Verify SSL? [Y/n]: ");
    io::stdout().flush()?;
    let mut verify_ssl_str = String::new();
    io::stdin().read_line(&mut verify_ssl_str)?;
    let verify_ssl = match verify_ssl_str.trim().to_lowercase().as_str() {
        "n" | "no" | "false" => false,
        _ => true,
    };

    println!();
    println!("Saving configuration...");

    // Try to save token to keyring
    let service = "splunk-tui";
    let user = "token"; // We use a fixed username 'token' for the service

    let mut token_saved_to_keyring = false;

    match Entry::new(service, user) {
        Ok(entry) => {
            if let Err(e) = entry.set_password(&token) {
                eprintln!("Warning: Failed to save token to OS keyring: {}", e);
                eprintln!("Falling back to saving token in config file (plaintext).");
            } else {
                token_saved_to_keyring = true;
                println!("Token saved securely to OS keyring.");
            }
        }
        Err(e) => {
            eprintln!("Warning: Failed to access OS keyring: {}", e);
            eprintln!("Falling back to saving token in config file (plaintext).");
        }
    }

    // Save other config to toml
    if let Some(proj_dirs) = ProjectDirs::from("", "", "splunk-tui") {
        let config_dir = proj_dirs.config_dir();
        std::fs::create_dir_all(config_dir)?;
        let config_path = config_dir.join("config.toml");

        // Read existing config to preserve theme
        let mut file_config: FileConfig = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content).unwrap_or_default()
        } else {
            FileConfig::default()
        };

        // Update fields
        file_config.splunk_base_url = Some(base_url);
        file_config.splunk_verify_ssl = Some(verify_ssl);

        if !token_saved_to_keyring {
            file_config.splunk_token = Some(token);
        } else {
            // If successfully saved to keyring, remove from file if it exists to be safe
            file_config.splunk_token = None;
        }

        let toml_string = toml::to_string(&file_config)?;
        std::fs::write(&config_path, toml_string)?;
        println!("Configuration saved to {:?}", config_path);
    } else {
        anyhow::bail!("Could not determine configuration directory.");
    }

    println!("Setup complete! You can now run `splunk-tui`.");
    Ok(())
}
