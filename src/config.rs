use crate::{cli::Cli, location::Location, logging::LogLevel, persistence::StateFile};
use anyhow::Result;
use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
use figment::providers::{Env, Format, Serialized, Yaml};
use figment::Figment;
use once_cell::sync::OnceCell;
use path_dsl::path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::debug;

static DEFAULT_DIRS: OnceCell<DefaultDirs> = OnceCell::new();

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(skip)]
    pub config_path: PathBuf,
    pub main: MainConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_path: default_config_path(),
            main: MainConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MainConfig {
    pub state_dir: PathBuf,
    pub geohashes: Vec<String>,
    pub log_level: LogLevel,
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            state_dir: default_dirs().state.clone(),
            log_level: LogLevel::Info,
            geohashes: Vec::new(),
        }
    }
}

impl Config {
    pub fn from_default_file() -> Result<Self> {
        let config_path = default_config_path();

        let main = Figment::from(Serialized::defaults(MainConfig::default()))
            .merge(Yaml::file(&config_path))
            .merge(Env::prefixed("BOM"))
            .extract()?;

        Ok(Self { config_path, main })
    }

    pub fn from_cli(args: &Cli) -> Result<Self> {
        debug!("Loading config from CLI args: {:?}", args);

        let config_path = if let Some(path) = &args.config_path {
            path.canonicalize()?.to_owned()
        } else {
            default_config_path().to_owned()
        };

        let main = Figment::from(Serialized::defaults(MainConfig::default()))
            .merge(Yaml::file(&config_path))
            .merge(Env::prefixed("BOM"))
            .merge(Serialized::defaults(args))
            .extract()?;

        Ok(Config { config_path, main })
    }

    pub fn write_config_file(&self) -> Result<()> {
        let yaml = serde_yaml::to_string(&self.main)?;
        fs::write(&self.config_path, yaml)?;
        Ok(())
    }

    pub fn add_location(&mut self, location: Location) -> Result<()> {
        let state_file = self.get_state_file(&location.geohash)?;
        state_file.write(&location)?;
        self.main.geohashes.push(location.geohash.to_owned());
        self.write_config_file()?;
        Ok(())
    }

    fn get_state_file(&self, geohash: &str) -> Result<StateFile> {
        let filename = format!("{geohash}.json");
        let dir = &self.main.state_dir;
        let path = path!(dir | filename);
        StateFile::from_path(path)
    }

    pub fn get_state_files(&self) -> Result<Vec<StateFile>> {
        let mut state_files = Vec::new();
        for geohash in &self.main.geohashes {
            state_files.push(self.get_state_file(geohash)?);
        }
        Ok(state_files)
    }

    pub fn load_locations(&self) -> Result<Vec<(Location, StateFile)>> {
        let mut locations = Vec::new();
        for geohash in &self.main.geohashes {
            let state_file = self.get_state_file(geohash)?;
            let location = state_file.load()?;
            locations.push((location, state_file));
        }
        Ok(locations)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DefaultDirs {
    pub home: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub run: PathBuf,
}

pub fn default_config_path() -> PathBuf {
    let dir = &default_dirs().config;
    path!(dir | "config.yml")
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
