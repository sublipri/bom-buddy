use crate::location::{Location, State};
use crate::station::WeatherStation;
use anyhow::{anyhow, Result};
use rusqlite::{named_params, params, Connection};
use std::path::PathBuf;
use std::str::FromStr;
use tracing::{debug, info};

pub struct Database {
    path: PathBuf,
    pub conn: Connection,
}
impl Database {
    pub fn from_path(path: PathBuf) -> Result<Database> {
        let connection = Connection::open(&path)?;
        Ok(Self {
            path,
            conn: connection,
        })
    }

    pub fn init(&self) -> Result<()> {
        info!("creating database at {}", self.path.display());
        self.conn.execute_batch(include_str!("../sql/schema.sql"))?;
        Ok(())
    }

    pub fn insert_stations(
        &mut self,
        stations: impl Iterator<Item = WeatherStation>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut stmt = tx.prepare(include_str!("../sql/insert_station.sql"))?;

        for station in stations {
            stmt.execute(named_params! {
                ":id": station.id,
                ":district_id": station.district_id,
                ":name": station.name,
                ":start": station.start,
                ":end": station.end,
                ":latitude": station.latitude,
                ":longitude": station.longitude,
                ":source": station.source,
                ":state": station.state,
                ":height": station.height,
                ":barometric_height": station.barometric_height,
                ":wmo_id": station.wmo_id,
            })?;
        }

        stmt.finalize()?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_station(&self, station: &WeatherStation) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare_cached(include_str!("../sql/insert_station.sql"))?;

        stmt.execute(named_params! {
            ":id": station.id,
            ":district_id": station.district_id,
            ":name": station.name,
            ":start": station.start,
            ":end": station.end,
            ":latitude": station.latitude,
            ":longitude": station.longitude,
            ":source": station.source,
            ":state": station.state,
            ":height": station.height,
            ":barometric_height": station.barometric_height,
            ":wmo_id": station.wmo_id,
        })?;

        Ok(())
    }

    pub fn get_station(&self, bom_id: u32) -> Result<WeatherStation> {
        let mut stmt = self.conn.prepare("SELECT * FROM station WHERE id = (?)")?;
        let mut binding = stmt.query(params![bom_id])?;
        let row = binding.next()?.unwrap();

        Ok(WeatherStation {
            id: row.get(0)?,
            district_id: row.get(1)?,
            name: row.get(2)?,
            start: row.get(3)?,
            end: row.get(4)?,
            latitude: row.get(5)?,
            longitude: row.get(6)?,
            source: row.get(7)?,
            state: row.get(8)?,
            height: row.get(9)?,
            barometric_height: row.get(10)?,
            wmo_id: row.get(11)?,
        })
    }

    pub fn insert_location(&self, location: &Location) -> Result<()> {
        debug!(
            "Inserting location {} into {}",
            location.id,
            self.path.display()
        );
        let mut stmt = self
            .conn
            .prepare_cached(include_str!("../sql/insert_location.sql"))?;

        stmt.execute(named_params! {
            ":id": location.id,
            ":geohash": location.geohash,
            ":station_id": location.station.as_ref().map(|s| s.id),
            ":has_wave": location.has_wave,
            ":latitude": location.latitude,
            ":longitude": location.longitude,
            ":marine_area_id": location.marine_area_id,
            ":name": location.name,
            ":state": location.state.to_string(),
            ":postcode": location.postcode,
            ":tidal_point": location.tidal_point,
            ":timezone": location.timezone,
            ":weather": serde_json::to_string(&location.weather)?,
        })?;

        Ok(())
    }

    pub fn update_weather(&self, location: &Location) -> Result<()> {
        debug!(
            "Updating {}'s weather in {}",
            location.id,
            self.path.display()
        );
        let mut stmt = self
            .conn
            .prepare("UPDATE location SET weather = (?) WHERE id = (?)")?;
        let weather = serde_json::to_string(&location.weather)?;
        stmt.execute(params![weather, location.id])?;
        Ok(())
    }
    pub fn get_location(&self, id: &str) -> Result<Location> {
        let mut stmt = self.conn.prepare("SELECT * FROM location WHERE id = (?)")?;
        let mut binding = stmt.query(params![id])?;
        let Some(row) = binding.next()? else {
            return Err(anyhow!(
                "No record of Location {} in {}",
                id,
                self.path.display()
            ));
        };

        let station = if let Some(station_id) = row.get(2)? {
            Some(self.get_station(station_id)?)
        } else {
            None
        };
        let state_name: String = row.get(8)?;
        let state = State::from_str(&state_name).unwrap();
        let weather_json: String = row.get(12)?;
        let weather = serde_json::from_str(&weather_json).unwrap();

        Ok(Location {
            id: row.get(0)?,
            geohash: row.get(1)?,
            station,
            has_wave: row.get(3)?,
            latitude: row.get(4)?,
            longitude: row.get(5)?,
            marine_area_id: row.get(6)?,
            name: row.get(7)?,
            state,
            postcode: row.get(9)?,
            tidal_point: row.get(10)?,
            timezone: row.get(11)?,
            weather,
        })
    }

    pub fn get_locations(&self, location_ids: &Vec<String>) -> Result<Vec<Location>> {
        let mut locations = Vec::new();
        for id in location_ids {
            let location = self.get_location(id)?;
            locations.push(location);
        }
        Ok(locations)
    }
}
