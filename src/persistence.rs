use crate::location::Location;
use anyhow::Result;
use std::fs::{self, File};
use std::path::PathBuf;
use std::time::SystemTime;
use tracing::debug;

pub struct StateFile {
    path: PathBuf,
    mtime: Option<SystemTime>,
}

impl StateFile {
    pub fn from_path(path: PathBuf) -> Result<StateFile> {
        let mtime = if path.exists() {
            path.metadata()?.modified().ok()
        } else {
            None
        };
        Ok(Self { path, mtime })
    }

    pub fn load(&self) -> Result<Location> {
        debug!("Reading state from {}", self.path.display());
        let file = File::open(&self.path)?;
        Ok(serde_json::from_reader(file)?)
    }

    pub fn write(&self, location: &Location) -> Result<()> {
        let json = serde_json::to_string(&location)?;
        let tmp = self.path.with_file_name("tmp_state.json");
        debug!("Writing Location to {}", tmp.display());
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        debug!("Renamed {} to {}", tmp.display(), self.path.display());
        Ok(())
    }

    pub fn has_changed(&self) -> Result<bool> {
        let mut has_changed = false;
        let mtime = self.path.metadata()?.modified()?;
        if let Some(last_mtime) = self.mtime {
            if mtime > last_mtime {
                has_changed = true
            }
        }
        Ok(has_changed)
    }
}
