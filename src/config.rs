use crate::cli::{Cli, Commands};
use crate::persistence::Database;
use crate::radar::{Radar, RadarId, RadarImageOptions};
use crate::util::remove_if_exists;
use crate::{location::Location, logging::LoggingOptions};
use anyhow::{anyhow, Result};
use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
use figment::providers::{Env, Format, Serialized, Yaml};
use figment::Figment;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(skip)]
    pub config_path: PathBuf,
    pub main: MainConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_path: Self::default_path(),
            main: MainConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MainConfig {
    pub db_path: PathBuf,
    pub locations: Vec<String>,
    pub logging: LoggingOptions,
    pub radars: Vec<RadarConfig>,
    pub current_fstring: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RadarConfig {
    pub id: RadarId,
    pub name: String,
    pub opts: RadarImageOptions,
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            db_path: Config::default_dirs().state.join("bom-buddy.db"),
            logging: LoggingOptions::default(),
            radars: Vec::new(),
            locations: Vec::new(),
            current_fstring: "{icon} {temp} ({next_temp})".to_string(),
        }
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        Self::default_dirs().config.join("config.yml")
    }

    pub fn from_default_path() -> Result<Self> {
        let config_path = Self::default_path();

        let main = Figment::from(Serialized::defaults(MainConfig::default()))
            .merge(Yaml::file(&config_path))
            .merge(Env::prefixed("BOM_"))
            .extract()?;

        Ok(Self { config_path, main })
    }

    pub fn default_dirs() -> &'static DefaultDirs {
        if let Some(defaults) = DEFAULT_DIRS.get() {
            defaults
        } else {
            let strategy = choose_app_strategy(AppStrategyArgs {
                top_level_domain: "org".to_string(),
                author: "sublipri".to_string(),
                app_name: "BOM Buddy".to_string(),
            })
            .unwrap();
            let defaults = DefaultDirs {
                home: strategy.home_dir().to_path_buf(),
                config: strategy.config_dir(),
                cache: strategy.cache_dir(),
                data: strategy.data_dir(),
                run: strategy.runtime_dir().unwrap_or(strategy.data_dir()),
                state: strategy.state_dir().unwrap_or(strategy.data_dir()),
            };
            DEFAULT_DIRS.set(defaults).unwrap();
            DEFAULT_DIRS.get().unwrap()
        }
    }

    pub fn from_cli(args: &Cli) -> Result<Self> {
        let config_path = if let Some(path) = &args.config_path {
            path.to_owned()
        } else {
            Self::default_path().to_owned()
        };
        if let Some(Commands::Init(iargs)) = &args.command {
            if iargs.force {
                remove_if_exists(&config_path)?;
            }
        }

        let main = Figment::from(Serialized::defaults(MainConfig::default()))
            .merge(Yaml::file(&config_path))
            .merge(Env::prefixed("BOM_"))
            .merge(Serialized::defaults(args));

        let main = match &args.command {
            Some(Commands::Radar(rargs)) => {
                let arg_opts = serde_json::to_value(rargs)?;
                let mut radar_array: serde_json::Value = main.extract_inner("radars")?;
                override_array_opts(&mut radar_array, &arg_opts);
                main.merge(("radars", radar_array))
            }
            _ => main,
        };

        let mut main: MainConfig = main.extract()?;

        if let Some(level) = args.log_level {
            main.logging.console_level = level;
        }
        if let Some(level) = args.log_file_level {
            main.logging.file_level = level;
        }
        if let Some(path) = &args.log_path {
            main.logging.file_path = path.clone();
        }
        Ok(Config { config_path, main })
    }

    pub fn write_config_file(&self) -> Result<()> {
        let yaml = serde_yaml::to_string(&self.main)?;
        fs::write(&self.config_path, yaml)?;
        Ok(())
    }

    pub fn get_database(&self) -> Result<Database> {
        Database::from_path(self.main.db_path.clone())
    }

    pub fn add_location(&mut self, location: &Location) -> Result<()> {
        if self.main.locations.contains(&location.id) {
            return Err(anyhow!(
                "{} already in {}",
                location.id,
                self.config_path.display()
            ));
        }
        info!("Adding {} to {}", location.id, self.config_path.display());
        self.main.locations.push(location.id.to_owned());
        self.write_config_file()?;
        Ok(())
    }

    pub fn add_radar(&mut self, radar: &Radar) -> Result<()> {
        let radar_config = RadarConfig {
            id: radar.id,
            name: radar.full_name.clone(),
            opts: RadarImageOptions::default(),
        };
        info!(
            "Adding radar {} {} to {}",
            radar_config.id,
            &radar_config.name,
            self.config_path.display()
        );
        self.main.radars.push(radar_config);
        self.write_config_file()?;
        Ok(())
    }
}

static DEFAULT_DIRS: OnceCell<DefaultDirs> = OnceCell::new();

#[derive(Debug, Deserialize, Serialize)]
pub struct DefaultDirs {
    pub home: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub run: PathBuf,
}

// A hacky way to allow different radars to have different options in the config file
// that are still overwritten by CLI arguments
fn override_array_opts(config_array: &mut serde_json::Value, arg_opts: &serde_json::Value) {
    let config_array = config_array.as_array_mut().unwrap();
    for element in &mut *config_array {
        let Some(conf_opts) = element.get_mut("opts") else {
            continue;
        };
        let Some(conf_opts) = conf_opts.as_object_mut() else {
            continue;
        };
        for (key, arg_value) in arg_opts.as_object().unwrap() {
            if let Some(conf_value) = conf_opts.get_mut(key) {
                *conf_value = arg_value.clone();
            }
        }
    }
}
