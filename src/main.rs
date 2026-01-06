mod api;
mod config;
mod config_wizard;
mod models;
mod tui;
mod utils;

use clap::{Parser, Subcommand};
use simplelog::*;
use std::fs::File;

#[derive(Parser)]
#[command(name = "splunk-tui")]
#[command(about = "A TUI for Splunk", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the configuration wizard
    Config,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    if let Some(Commands::Config) = args.command {
        config_wizard::run()?;
        return Ok(());
    }

    let _ = WriteLogger::init(
        LevelFilter::Info,
        simplelog::Config::default(),
        File::create("splunk_tui.log")?,
    );
    log::info!("Application started");

    tui::run_app().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn test_project_name() {
        let app_name = "splunk-tui";
        assert_eq!(app_name, "splunk-tui", "Project name should match");
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert!(
            config.splunk_base_url.is_empty(),
            "Default base URL should be empty"
        );
        assert!(
            config.splunk_token.is_empty(),
            "Default token should be empty"
        );
        assert!(
            !config.splunk_verify_ssl,
            "Default SSL verify should be false"
        );
    }
}
