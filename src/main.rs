mod api;
mod models;
mod tui;
mod config;
mod utils;

use simplelog::*;
use std::fs::File;

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
