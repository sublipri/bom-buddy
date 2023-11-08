use crate::client::Client;
use crate::config::{default_config_path, default_dirs, Config};
use crate::location::Location;
use crate::logging::{setup_logging, LogLevel};
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use inquire::{Select, Text};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, error, info};

fn default_path(path: &Path) -> String {
    format!("[default: {}]", path.as_os_str().to_string_lossy())
}
/// Australian weather tool
#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "DIR", help = default_path(&default_dirs().state)) ]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<PathBuf>,

    #[arg(short, long = "config", value_name = "FILE", help = default_path(&default_config_path()))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<PathBuf>,

    /// Change the log verbosity
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    log_level: Option<LogLevel>,

    /// 6 character geohash representing a location. Can be specified multiple times
    #[arg(short, long = "geohash", value_name = "GEOHASH")]
    #[serde(skip_serializing_if = "Option::is_none")]
    geohashes: Option<Vec<String>>,

    #[command(subcommand)]
    #[serde(skip)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run continuously and check the weather when an update is due.
    Monitor,
    /// Search for a location and save it in the config file
    AddLocation,
}

pub fn cli() -> Result<()> {
    let args = Cli::parse();

    if let Some(level) = args.log_level {
        setup_logging(level);
    }
    let mut config = Config::from_cli(&args)?;
    debug!("Config: {:?}", config);
    if args.log_level.is_none() {
        setup_logging(config.main.log_level);
    }

    match &args.command {
        Some(Commands::Monitor) => monitor(&config)?,
        Some(Commands::AddLocation) => add_location(&mut config)?,
        None => {}
    }
    Ok(())
}

fn monitor(config: &Config) -> Result<()> {
    let client = Client::new();
    let mut locations = config.load_locations()?;
    if locations.is_empty() {
        return Err(anyhow!("No locations in config. Try adding one."));
    }

    debug!("Monitoring locations: {:?}", config.main.geohashes);
    loop {
        for (location, state_file) in &mut locations {
            let was_updated = client.update_if_due(location)?;
            if was_updated {
                state_file.write(location)?;
            }
        }
        sleep(Duration::from_secs(1));
    }
}

fn add_location(config: &mut Config) -> Result<()> {
    let client = Client::new();
    let location = search_for_location(&client)?;
    config.add_location(location)?;
    Ok(())
}

fn search_for_location(client: &Client) -> Result<Location> {
    loop {
        let input = Text::new("Enter your location").prompt().unwrap();
        let results = client.search(&input)?;
        if results.is_empty() {
            info!("No search results for {input}");
            continue;
        } else if results.len() == 1 {
            let result = &results[0];
            info!("Selecting only result: {result}");
            return client.get_location(&result.geohash);
        };

        let selection = match Select::new("Select a result: ", results).prompt() {
            Ok(s) => s,
            Err(_) => {
                error!("An error occured. Please try again.");
                continue;
            }
        };
        return client.get_location(&selection.geohash);
    }
}
