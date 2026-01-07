use crate::config::FileConfig;
use anyhow::Result;
use directories::ProjectDirs;
use std::io::{self, Write};

pub fn run() -> Result<()> {
    println!("Welcome to spelunktui configuration wizard!");
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
    let verify_ssl = !matches!(
        verify_ssl_str.trim().to_lowercase().as_str(),
        "n" | "no" | "false"
    );

    println!();
    println!("Saving configuration...");

    // Save to global config directory
    if let Some(proj_dirs) = ProjectDirs::from("", "", "spelunktui") {
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
        file_config.splunk_token = Some(token);

        let toml_string = toml::to_string(&file_config)?;
        std::fs::write(&config_path, toml_string)?;
        println!("Configuration saved to: {}", config_path.display());
        println!();
        println!("You can now run 'spelunktui' from any directory.");
    } else {
        anyhow::bail!("Could not determine configuration directory.");
    }

    println!("Setup complete!");
    Ok(())
}
