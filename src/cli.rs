use crate::client::Client;
use crate::config::Config;
use crate::ftp::FtpClient;
use crate::location::SearchResult;
use crate::logging::{setup_logging, LogLevel};
use crate::persistence::Database;
use crate::radar::{
    get_radar_image_managers, update_radar_images, Radar, RadarImageFeature, RadarImageManager,
    RadarImageOptions, RadarType,
};
use crate::services::{create_location, get_nearby_radars, ids_to_locations, update_if_due};
use crate::station::StationsTable;
use crate::util::{format_duration, remove_if_exists};
use crate::weather::{FstringKey, WeatherOptions};
use anyhow::{anyhow, Result};
use chrono::{Duration, Local, Utc};
use clap::{Parser, Subcommand};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use inquire::{Select, Text};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::thread::sleep;
use strum::IntoEnumIterator;
use tracing::{debug, error, info, trace};

// Hacky way to display a default config value on the CLI.
// Can't actually set a default value since it would override the config file
fn show_default(default: &impl Display, help: &str) -> String {
    format!("{help} [default: {default}]")
}
/// Australian weather tool
#[skip_serializing_none]
#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "FILE",
        help = show_default(&Config::default().main.db_path.display(), "Database file"))]
    pub db_path: Option<PathBuf>,

    #[arg(short, long = "config", value_name = "FILE",
        help = show_default(&Config::default_path().display(), "Config file"))]
    pub config_path: Option<PathBuf>,

    #[arg(short = 'L', long, value_name = "FILE",
        help = show_default(&Config::default().main.logging.file_path.display(), "Log file"))]
    pub log_path: Option<PathBuf>,

    #[arg(short, long,  value_name = "LEVEL",
        help = show_default(&Config::default().main.logging.console_level, "Console log level"))]
    pub log_level: Option<LogLevel>,

    #[arg(short = 'f', long, value_name = "LEVEL",
        help = show_default(&Config::default().main.logging.file_level, "File log level"))]
    pub log_file_level: Option<LogLevel>,

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
    Init(InitArgs),
    /// Run continuously and check the weather when an update is due.
    Monitor,
    /// Search for a location and save it in the config file
    AddLocation,
    /// Edit options used when updating the weather
    EditOpts,
    /// Display the 7-day forecast
    Daily(DailyArgs),
    /// Display the hourly forecast
    Hourly(HourlyArgs),
    /// Display the current weather
    Current(CurrentArgs),
    /// Download and view radar images
    Radar(RadarArgs),
}

pub fn cli() -> Result<()> {
    let args = Cli::parse();
    let mut config = Config::from_cli(&args)?;
    let _guard = setup_logging(&config.main.logging);
    trace!("Command line arguments: {:#?}", &args);
    trace!("Config: {:#?}", &config);

    match &args.command {
        Some(Commands::Init(args)) => init(&mut config, args)?,
        Some(Commands::Monitor) => monitor(&config)?,
        Some(Commands::AddLocation) => add_location(&mut config)?,
        Some(Commands::EditOpts) => edit_weather_opts(&config)?,
        Some(Commands::Daily(args)) => daily(&config, args)?,
        Some(Commands::Hourly(args)) => hourly(&config, args)?,
        Some(Commands::Current(args)) => current(&config, args)?,
        Some(Commands::Radar(args)) => radar(&config, args.monitor)?,
        None => {}
    }
    Ok(())
}

#[derive(Parser, Debug, Serialize, Deserialize)]
pub struct InitArgs {
    /// Overwrite any existing database and config
    #[arg(short, long)]
    pub force: bool,
}
fn init(config: &mut Config, args: &InitArgs) -> Result<()> {
    if args.force {
        remove_if_exists(&config.main.db_path)?;
    }
    let client = config.get_client();
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
    let client = config.get_client();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;

    for location in &locations {
        info!("Monitoring weather for {}", location.id);
    }
    loop {
        let next_check = update_if_due(&mut locations, &client, &database)?;
        let sleep_duration = (next_check - Utc::now()).max(Duration::seconds(1));
        debug!("Next weather update in {}", format_duration(sleep_duration));
        sleep((sleep_duration + Duration::seconds(1)).to_std().unwrap());
    }
}

fn add_location(config: &mut Config) -> Result<()> {
    let client = config.get_client();
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

fn edit_weather_opts(config: &Config) -> Result<()> {
    let client = config.get_client();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;
    let mut location_opts = BTreeMap::new();
    for location in &locations {
        location_opts.insert(&location.id, &location.weather.opts);
    }
    let to_edit = serde_yaml::to_string(&location_opts)?;
    let mut builder = tempfile::Builder::new();
    let edited = edit::edit_with_builder(to_edit, builder.suffix(".yml"))?;
    let mut edited_opts: BTreeMap<String, WeatherOptions> = serde_yaml::from_str(&edited)?;
    for location in &mut locations {
        location.weather.opts = edited_opts.remove(&location.id).unwrap();
        database.update_weather(location)?;
    }
    Ok(())
}

#[derive(Parser, Debug, Serialize, Deserialize)]
pub struct CurrentArgs {
    /// Check for updates if due
    #[arg(short, long)]
    check: bool,
    /// Custom format string
    #[arg(short, long)]
    fstring: Option<String>,
    /// List the keys that can be used in an fstring
    #[arg(short, long)]
    list_keys: bool,
}

fn current(config: &Config, args: &CurrentArgs) -> Result<()> {
    if args.list_keys {
        for key in FstringKey::iter() {
            println!("{}", key.as_ref());
        }
        return Ok(());
    }
    if config.main.locations.is_empty() {
        return Err(anyhow!("No locations specified"));
    }
    let client = config.get_client();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;
    if args.check {
        update_if_due(&mut locations, &client, &database)?;
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

fn daily(config: &Config, args: &DailyArgs) -> Result<()> {
    if config.main.locations.is_empty() {
        return Err(anyhow!("No locations specified"));
    }
    let client = config.get_client();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;

    if args.force_check {
        for location in &mut locations {
            let new_daily = client.get_daily(&location.geohash)?;
            location.weather.update_daily(Utc::now(), new_daily);
            database.update_weather(location)?;
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

            let max = day.temp_max.map_or("".to_string(), |t| t.to_string());
            let min = day.temp_min.map_or("".to_string(), |t| t.to_string());
            let description = if args.extended {
                day.extended_text.clone().unwrap_or(String::new())
            } else {
                day.short_text.clone().unwrap_or(String::new())
            };

            let rain = if day.rain.amount.max.is_some() && day.rain.amount.lower_range.is_some() {
                format!(
                    "{}-{}{}",
                    day.rain.amount.lower_range.unwrap(),
                    day.rain.amount.max.unwrap(),
                    day.rain.amount.units
                )
            } else {
                "0mm".to_string()
            };
            let chance = if let Some(chance) = day.rain.chance {
                format!("{}%", chance)
            } else {
                String::new()
            };

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

#[derive(Parser, Debug, Serialize, Deserialize)]
pub struct HourlyArgs {
    /// Check for updates if due
    #[arg(short, long)]
    check: bool,
    /// Force an update even if a new forecast isn't due
    #[arg(short, long)]
    force_check: bool,
    /// How many hours to show
    #[arg(short = 'H', long, default_value_t = 12)]
    hours: usize,
}

fn hourly(config: &Config, args: &HourlyArgs) -> Result<()> {
    if config.main.locations.is_empty() {
        return Err(anyhow!("No locations specified"));
    }
    let client = config.get_client();
    let database = config.get_database()?;
    let mut locations = ids_to_locations(&config.main.locations, &client, &database)?;

    if args.force_check {
        for location in &mut locations {
            let new_hourly = client.get_hourly(&location.geohash)?;
            location.weather.update_hourly(Utc::now(), new_hourly);
            database.update_weather(location)?;
        }
    } else if args.check {
        update_if_due(&mut locations, &client, &database)?;
    }

    for location in locations {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic);

        let issue_time = location
            .weather
            .hourly_forecast
            .issue_time
            .with_timezone(&Local)
            .format("%r");
        let title = format!("Hourly forecast for {} issued at {}", location, issue_time);

        let todo = location
            .weather
            .hourly_forecast
            .data
            .iter()
            .filter(|h| h.next_forecast_period > Utc::now())
            .take(args.hours);

        let show_rain = todo.clone().any(|h| h.rain.chance > 0);
        // TODO: Make this configurable. Perhaps let the user specify column names paired with
        // an fstring that's used to generate the row text.
        let columns = if show_rain {
            vec![
                "Time", "Temp", "Desc", "Rain", "Chance", "Wind", "Gust", "Humidity",
            ]
        } else {
            vec!["Time", "Temp", "Desc", "Wind", "Gust", "Humidity"]
        };
        table.set_header(columns);

        for hour in todo {
            let time = hour.time.with_timezone(&Local).format("%a %r").to_string();
            let chance = format!("{}%", hour.rain.chance);
            let wind = format!("{} {}", hour.wind.speed_kilometre, hour.wind.direction);
            let gust = format!("{}", hour.wind.gust_speed_kilometre);
            let temp = format!("{} ({})", hour.temp, hour.temp_feels_like);
            let desc = hour.icon_descriptor.get_description(hour.is_night);

            let cells = if show_rain {
                let rain = if let Some(max) = hour.rain.amount.max {
                    format!("{}-{}{}", hour.rain.amount.min, max, hour.rain.amount.units)
                } else {
                    "0mm".to_string()
                };
                vec![
                    Cell::new(&time),
                    Cell::new(&temp),
                    Cell::new(desc),
                    Cell::new(&rain),
                    Cell::new(&chance),
                    Cell::new(&wind),
                    Cell::new(&gust),
                    Cell::new(format!("{}%", &hour.relative_humidity)),
                ]
            } else {
                vec![
                    Cell::new(&time),
                    Cell::new(temp),
                    Cell::new(desc),
                    Cell::new(&wind),
                    Cell::new(&gust),
                    Cell::new(format!("{}%", &hour.relative_humidity)),
                ]
            };
            table.add_row(cells);
        }
        println!("{title}");
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
    #[arg(short = 'o', long, value_name = "DIR",
        help = show_default(&RadarImageOptions::default().image_dir.display(),
        "Output directory for image files"))]
    pub image_dir: Option<PathBuf>,
    #[arg(short = 'I', long, value_name = "DIR",
        help = show_default(&RadarImageOptions::default().mpv_ipc_dir.display(),
        "Runtime directory for MPV IPC sockets"))]
    pub mpv_ipc_dir: Option<PathBuf>,
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
