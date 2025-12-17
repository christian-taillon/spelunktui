mod api;
mod models;
mod tui;
mod config;

use simplelog::*;
use std::fs::File;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    WriteLogger::init(
        LevelFilter::Info,
        Config::default(),
        File::create("splunk_tui.log")?,
    )?;
    log::info!("Application started");

    let config = crate::config::Config::load()?;
    log::info!("Loaded configuration. Base URL: {}", config.splunk_base_url);
    // Do not log token!

    tui::run_app().await?;
    Ok(())
}
