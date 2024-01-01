use crate::persistence::Database;
use crate::{cli::Cli, location::Location, logging::LoggingOptions};
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
    pub state_dir: PathBuf,
    pub locations: Vec<String>,
    pub logging: LoggingOptions,
    pub current_fstring: String,
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            state_dir: Config::default_dirs().state.clone(),
            logging: LoggingOptions::default(),
            locations: Vec::new(),
            current_fstring: "{icon} {temp} ({next_temp})".to_string(),
        }
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        let mut path = PathBuf::from(&Self::default_dirs().config);
        path.push("config.yml");
        path
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

        let main = Figment::from(Serialized::defaults(MainConfig::default()))
            .merge(Yaml::file(&config_path))
            .merge(Env::prefixed("BOM_"))
            .merge(Serialized::defaults(args))
            .extract()?;

        Ok(Config { config_path, main })
    }

    pub fn write_config_file(&self) -> Result<()> {
        let yaml = serde_yaml::to_string(&self.main)?;
        fs::write(&self.config_path, yaml)?;
        Ok(())
    }

    pub fn get_database(&self) -> Result<Database> {
        let mut path = PathBuf::from(&self.main.state_dir);
        path.push("bom-buddy.db");
        Database::from_path(path)
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
