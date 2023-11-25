use crate::client::Client;
use crate::config::Config;
use crate::location::SearchResult;
use crate::logging::{setup_logging, LogLevel};
use crate::services::{create_location, ids_to_locations, update_if_due};
use crate::station::StationsTable;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use inquire::{Select, Text};
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, error, info};

fn default(path: &Path) -> String {
    format!("[default: {}]", path.as_os_str().to_string_lossy())
}
/// Australian weather tool
#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "DIR", help = default(&Config::default_dirs().state)) ]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<PathBuf>,

    #[arg(short, long = "config", value_name = "FILE", help = default(&Config::default_path()))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<PathBuf>,

    /// [default: info]
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    log_level: Option<LogLevel>,

    /// Suburb followed by geohash e.g. Canberra-r3dp5hh (overrides config)
    #[arg(short = 'i', long = "location-id", value_name = "ID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    locations: Option<Vec<String>>,

    #[command(subcommand)]
    #[serde(skip)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Initialize the database and setup your location
    Init,
    /// Run continuously and check the weather when an update is due.
    Monitor,
    /// Search for a location and save it in the config file
    AddLocation,
    /// Print the current weather
    Current {
        /// Check for updates before printing
        #[arg(short, long)]
        check: bool,
        /// Custom format string
        #[arg(short, long)]
        fstring: Option<String>,
    },
}

pub fn cli() -> Result<()> {
    let args = Cli::parse();

    let mut config = Config::from_cli(&args)?;
    if let Some(level) = args.log_level {
        config.main.logging.console_level = level;
        config.main.logging.file_level = level;
    }
    let _guard = setup_logging(&config.main.logging);
    debug!("Command line arguments: {:#?}", &args);
    debug!("Config: {:#?}", &config);

    match &args.command {
        Some(Commands::Init) => init(&mut config)?,
        Some(Commands::Monitor) => monitor(&config)?,
        Some(Commands::AddLocation) => add_location(&mut config)?,
        Some(Commands::Current { check, fstring }) => print_current(&config, *check, fstring)?,
        None => {}
    }
    Ok(())
}

fn init(config: &mut Config) -> Result<()> {
    let client = Client::default();
    let mut database = config.get_database()?;
    database.init()?;
    info!("Downloading weather stations");
    let stations = client.get_station_list()?;
    info!("Inserting weather stations into database");
    let stations = StationsTable::new(&stations);
    // Skip discontinued stations and those in Antarctica
    let stations = stations.filter(|s| s.end.is_none() && s.state != "ANT");
    database.insert_stations(stations)?;
    let result = search_for_location(&client)?;
    let location = create_location(result, &client, &database)?;
    config.add_location(&location)?;
    Ok(())
}

fn monitor(config: &Config) -> Result<()> {
    if config.main.locations.is_empty() {
        return Err(anyhow!("No locations specified"));
    }
    let client = Client::default();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;

    for location in &locations {
        info!("Monitoring weather for {}", location.id);
    }
    loop {
        update_if_due(&mut locations, &client, &database)?;
        sleep(Duration::from_secs(1));
    }
}

fn add_location(config: &mut Config) -> Result<()> {
    let client = Client::default();
    let database = config.get_database()?;
    let result = search_for_location(&client)?;
    let location = create_location(result, &client, &database)?;
    config.add_location(&location)?;
    Ok(())
}

fn search_for_location(client: &Client) -> Result<SearchResult> {
    loop {
        let input = Text::new("Enter your suburb").prompt().unwrap();
        let results = client.search(&input)?;
        if results.is_empty() {
            info!("No search results for {input}");
            continue;
        } else if results.len() == 1 {
            let result = &results[0];
            info!("Selecting only result: {result}");
            return Ok(result.clone());
        };

        let selection = match Select::new("Select a result: ", results).prompt() {
            Ok(s) => s,
            Err(_) => {
                error!("An error occured. Please try again.");
                continue;
            }
        };
        return Ok(selection);
    }
}

fn print_current(config: &Config, check: bool, fstring: &Option<String>) -> Result<()> {
    if config.main.locations.is_empty() {
        return Err(anyhow!("No locations specified"));
    }
    let client = Client::default();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;
    if check {
        update_if_due(&mut locations, &client, &database)?
    }
    let fstring = fstring.as_ref().unwrap_or(&config.main.current_fstring);
    for location in locations {
        let current = location.weather.current();
        let output = current.process_fstring(fstring)?;
        if std::io::stdout().is_terminal() {
            println!("{output}");
        } else {
            print!("{output}");
        }
    }
    Ok(())
}
