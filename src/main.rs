mod api;
mod models;
mod tui;
mod config;

use simplelog::*;
use std::fs::File;
use crate::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = WriteLogger::init(
        LevelFilter::Info,
        simplelog::Config::default(),
        File::create("splunk_tui.log")?,
    );
    log::info!("Application started");

    tui::run_app().await?;
    Ok(())
}
