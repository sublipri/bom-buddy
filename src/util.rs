use anyhow::Result;
use chrono::Duration;
use std::{fs, path::Path};
use tracing::info;

pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.num_seconds();
    let minutes = total_seconds / 60 % 60;
    let hours = total_seconds / 60 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{}h{}m{:02}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m{:02}s", minutes, seconds)
    } else {
        format!("{:02}s", seconds)
    }
}

pub fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        info!("Deleting {}", path.display());
        fs::remove_file(path)?;
    }
    Ok(())
}
