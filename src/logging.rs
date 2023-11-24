use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::{filter_fn, LevelFilter};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

#[derive(clap::ValueEnum, Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LoggingOptions {
    pub use_stderr: bool,
    pub console_level: LogLevel,
    pub file_path: PathBuf,
    pub file_level: LogLevel,
    pub journal_level: LogLevel,
    pub rotate_logs: bool,
    pub exclude_external: bool,
}

impl Default for LoggingOptions {
    fn default() -> Self {
        let mut file_path = Config::default_dirs().run.clone();
        file_path.push("bom-buddy.log");

        Self {
            use_stderr: true,
            console_level: LogLevel::Info,
            file_path,
            file_level: LogLevel::Debug,
            journal_level: LogLevel::Off,
            rotate_logs: false,
            exclude_external: true,
        }
    }
}

#[derive(Default)]
pub struct LogGuards {
    file: Option<WorkerGuard>,
    console: Option<WorkerGuard>,
}

pub fn setup_logging(opts: &LoggingOptions) -> LogGuards {
    let mut layers = Vec::new();
    let mut guards = LogGuards::default();
    let exclude_external = if opts.exclude_external {
        Some(filter_fn(|metadata| {
            metadata.target().starts_with("bom_buddy")
        }))
    } else {
        None
    };

    let console_level: LevelFilter = opts.console_level.into();
    if console_level > LevelFilter::OFF {
        let (console_writer, _guard) = if opts.use_stderr {
            tracing_appender::non_blocking(std::io::stderr())
        } else {
            tracing_appender::non_blocking(std::io::stdout())
        };
        guards.console = Some(_guard);
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(console_writer)
            .with_filter::<LevelFilter>(opts.console_level.into())
            .with_filter(exclude_external.clone())
            .boxed();
        layers.push(console_layer);
    }

    let file_level: LevelFilter = opts.file_level.into();
    if file_level > LevelFilter::OFF {
        let log_dir = opts.file_path.parent().unwrap();
        let file_name = opts.file_path.file_name().unwrap();
        let file_appender = if opts.rotate_logs {
            tracing_appender::rolling::daily(log_dir, file_name)
        } else {
            tracing_appender::rolling::never(log_dir, file_name)
        };
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        guards.file = Some(guard);
        let file_layer = fmt::Layer::default()
            .with_writer(file_writer)
            .with_ansi(false)
            .with_filter::<LevelFilter>(opts.file_level.into())
            .with_filter(exclude_external.clone())
            .boxed();
        layers.push(file_layer);
    }

    let journal_level: LevelFilter = opts.journal_level.into();
    if journal_level > LevelFilter::OFF {
        let journal_layer = tracing_journald::layer()
            .expect("Couldn't connect to journald")
            .with_filter::<LevelFilter>(opts.journal_level.into())
            .with_filter(exclude_external.clone())
            .boxed();
        layers.push(journal_layer);
    }

    tracing_subscriber::registry().with(layers).init();
    guards
}
