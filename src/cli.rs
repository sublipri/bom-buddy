use crate::client::Client;
use crate::config::Config;
use crate::ftp::FtpClient;
use crate::location::SearchResult;
use crate::logging::{setup_logging, LogLevel};
use crate::persistence::Database;
use crate::radar::{
    get_radar_image_managers, update_radar_images, Radar, RadarImageFeature, RadarImageManager,
    RadarType,
};
use crate::services::{create_location, get_nearby_radars, ids_to_locations, update_if_due};
use crate::station::StationsTable;
use anyhow::{anyhow, Result};
use chrono::{Duration, Local, Utc};
use clap::{Parser, Subcommand};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use inquire::{Select, Text};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use tracing::{debug, error, info};

fn default(path: &Path) -> String {
    format!("[default: {}]", path.as_os_str().to_string_lossy())
}
/// Australian weather tool
#[skip_serializing_none]
#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "DIR", help = default(&Config::default_dirs().state)) ]
    pub state_dir: Option<PathBuf>,

    #[arg(short, long = "config", value_name = "FILE", help = default(&Config::default_path()))]
    pub config_path: Option<PathBuf>,

    /// [default: info]
    #[arg(short, long)]
    pub log_level: Option<LogLevel>,

    /// Suburb followed by geohash e.g. Canberra-r3dp5hh (overrides config)
    #[arg(short = 'i', long = "location-id", value_name = "ID")]
    pub locations: Option<Vec<String>>,

    #[command(subcommand)]
    #[serde(skip)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Initialize the database and setup your location
    Init,
    /// Run continuously and check the weather when an update is due.
    Monitor,
    /// Search for a location and save it in the config file
    AddLocation,
    /// Print the 7-day forecast
    Daily(DailyArgs),
    /// Print the current weather
    Current(CurrentArgs),
    /// Download and view radar images
    Radar(RadarArgs),
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
        Some(Commands::Daily(args)) => print_daily(&config, args)?,
        Some(Commands::Current(args)) => print_current(&config, args)?,
        Some(Commands::Radar(args)) => radar(&config, args.monitor)?,
        None => {}
    }
    Ok(())
}

fn init(config: &mut Config) -> Result<()> {
    let client = Client::default();
    let mut db = config.get_database()?;
    db.init()?;
    info!("Downloading weather stations");
    let stations = client.get_station_list()?;
    info!("Inserting weather stations into database");
    let stations = StationsTable::new(&stations);
    // Skip discontinued stations and those in Antarctica
    let stations = stations.filter(|s| s.end.is_none() && s.state != "ANT");
    db.insert_stations(stations)?;
    let mut ftp = FtpClient::new()?;
    info!("Downloading radar data");
    let all_radars: Vec<Radar> = ftp.get_public_radars()?.collect();
    let legends = ftp.get_radar_legends()?;
    info!("Inserting radars into database");
    db.insert_radars(&all_radars, &legends)?;
    let result = search_for_location(&client)?;
    let location = create_location(result, &client, &db)?;
    config.add_location(&location)?;
    let nearby_radars = get_nearby_radars(&location, &all_radars);
    let radar_id = if nearby_radars.len() == 1 {
        info!("Selecting only nearby radar {}", nearby_radars[0]);
        nearby_radars[0].id
    } else {
        let selection = Select::new("Select a Radar", nearby_radars).prompt()?;
        selection.id
    };
    let radar = all_radars.iter().find(|r| r.id == radar_id).unwrap();
    config.add_radar(radar)?;
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
        sleep(Duration::seconds(1).to_std().unwrap());
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

#[derive(Parser, Debug, Serialize, Deserialize)]
pub struct CurrentArgs {
    /// Check for updates if due
    #[arg(short, long)]
    check: bool,
    /// Custom format string
    #[arg(short, long)]
    fstring: Option<String>,
}

fn print_current(config: &Config, args: &CurrentArgs) -> Result<()> {
    if config.main.locations.is_empty() {
        return Err(anyhow!("No locations specified"));
    }
    let client = Client::default();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;
    if args.check {
        update_if_due(&mut locations, &client, &database)?
    }
    let fstring = args
        .fstring
        .as_ref()
        .unwrap_or(&config.main.current_fstring);
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

#[derive(Parser, Debug, Serialize, Deserialize)]
pub struct DailyArgs {
    /// Check for updates if due
    #[arg(short, long)]
    check: bool,
    /// Force an update even if a new forecast isn't due
    #[arg(short, long)]
    force_check: bool,
    /// Show the extended description for each day's forecast
    #[arg(short, long)]
    extended: bool,
}

fn print_daily(config: &Config, args: &DailyArgs) -> Result<()> {
    if config.main.locations.is_empty() {
        return Err(anyhow!("No locations specified"));
    }
    let client = Client::default();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;

    if args.force_check {
        for location in &mut locations {
            let new_daily = client.get_daily(&location.geohash)?;
            let was_updated = location.weather.update_daily(Utc::now(), new_daily);
            if was_updated {
                database.update_weather(location)?;
            }
        }
    } else if args.check {
        update_if_due(&mut locations, &client, &database)?;
    }

    for location in locations {
        let mut table = Table::new();

        let issued = location
            .weather
            .daily_forecast
            .issue_time
            .with_timezone(&Local)
            .format("%r");

        let header = format!("Forecast for {} issued at {}", location, issued);
        println!("{header}");
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec!["Day", "Min", "Max", "Rain", "Chance", "Description"]);

        for day in &location.weather.daily_forecast.days {
            let date = day
                .date
                .with_timezone(&Local)
                .format("%a %d %b")
                .to_string();

            let max = day.temp_max.to_string();
            let min = day.temp_min.map_or("".to_string(), |t| t.to_string());
            let description = if args.extended {
                day.extended_text.clone().unwrap_or(String::new())
            } else {
                day.short_text.clone().unwrap_or(String::new())
            };

            let rain = if let Some(max) = day.rain.amount.max {
                format!(
                    "{}-{}{}",
                    day.rain.amount.lower_range, max, day.rain.amount.units
                )
            } else {
                "0mm".to_string()
            };
            let chance = format!("{}%", day.rain.chance);

            table.add_row(vec![
                Cell::new(&date),
                Cell::new(&min),
                Cell::new(&max),
                Cell::new(&rain),
                Cell::new(&chance),
                Cell::new(&description),
            ]);
        }
        println!("{table}");
    }
    Ok(())
}

#[skip_serializing_none]
#[derive(Parser, Debug, Deserialize, Serialize)]
pub struct RadarArgs {
    /// Can be specified multiple times
    #[arg(short = 'F', long = "feature")]
    pub features: Option<Vec<RadarImageFeature>>,
    /// Can be specified multiple times
    #[arg(short, long = "radar-type")]
    pub radar_types: Option<Vec<RadarType>>,
    /// Remove the header at the top of each image
    #[arg(short = 'R', long)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub remove_header: bool,
    /// Re-generate images that already exist
    #[arg(short = 'f', long)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
    /// Create PNG files for each radar image
    #[arg(short = 'p', long)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub create_png: bool,
    /// Combine all images into an animated PNG file
    #[arg(short = 'a', long)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub create_apng: bool,
    /// View the images as a loop in MPV
    #[arg(short = 'v', long)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub open_mpv: bool,
    /// Time between each frame in milliseconds (applies to APNG and MPV)
    #[arg(short = 'd', long = "frame-delay")]
    pub frame_delay_ms: Option<u16>,
    /// Maximum amount of frames to create
    #[arg(short, long)]
    pub max_frames: Option<u64>,
    /// Output directory for image files
    #[arg(short = 'o', long, value_name = "DIRECTORY")]
    pub image_dir: Option<PathBuf>,
    /// Run continuously and fetch new radar images when available
    #[serde(skip)]
    #[arg(short = 'M', long)]
    pub monitor: bool,
}

fn radar(config: &Config, monitor: bool) -> Result<()> {
    let mut db = config.get_database()?;
    let mut ftp = FtpClient::new()?;
    let mut managers = Vec::new();
    for radar in &config.main.radars {
        managers.extend(get_radar_image_managers(
            radar.id,
            &mut db,
            &mut ftp,
            &radar.opts,
        )?);
    }

    let mut next_check = update_radar_images(&mut managers, &mut db, &mut ftp)?;
    manage_radar_images(&mut managers, &mut db)?;

    if !monitor {
        return Ok(());
    }

    loop {
        let sleep_duration = next_check - Utc::now();
        // The FTP connection will timeout irrecoverably if we wait too long without checking
        let sleep_duration = sleep_duration.min(Duration::seconds(150));
        debug!(
            "Next check for radar images in {} seconds",
            sleep_duration.num_seconds()
        );
        if sleep_duration > Duration::seconds(0) {
            sleep(sleep_duration.to_std().unwrap());
        }
        next_check = update_radar_images(&mut managers, &mut db, &mut ftp)?;
        manage_radar_images(&mut managers, &mut db)?;
    }
}

fn manage_radar_images(managers: &mut Vec<RadarImageManager>, db: &mut Database) -> Result<()> {
    for manager in managers {
        if manager.opts.create_png {
            manager.write_pngs()?;
        }
        if manager.opts.create_apng {
            manager.create_apng()?;
        }
        if manager.opts.open_mpv {
            manager.open_images()?;
        }
        let removed = manager.prune()?;
        db.delete_radar_data_layers(&removed)?;
    }
    Ok(())
}
