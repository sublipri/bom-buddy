use crate::location::{Location, State};
use crate::radar::{
    Radar, RadarId, RadarImageDataLayer, RadarImageFeature, RadarImageFeatureLayer,
    RadarImageLegend, RadarType,
};
use crate::station::WeatherStation;
use anyhow::{anyhow, Result};
use chrono::{TimeZone, Utc};
use rusqlite::{named_params, params, Connection, Row};
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

    pub fn insert_radars(&mut self, radars: &[Radar], legends: &[RadarImageLegend]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut stmt = tx.prepare(include_str!("../sql/insert_radar.sql"))?;

        for radar in radars {
            stmt.execute(named_params! {
                ":id": radar.id,
                ":name": radar.name,
                ":full_name": radar.full_name,
                ":latitude": radar.latitude,
                ":longitude": radar.longitude,
                ":state": radar.state,
                ":type_": radar.r#type,
                ":group_": radar.group,
            })?;
        }

        stmt.finalize()?;

        let mut stmt = tx.prepare("INSERT INTO radar_legend (id, image) VALUES (?, ?)")?;
        for legend in legends {
            stmt.execute(params![legend.r#type.id(), legend.png_buf])?;
        }
        stmt.finalize()?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_radars(&self) -> Result<Vec<Radar>> {
        let mut stmt = self.conn.prepare("SELECT * FROM radar")?;
        let mut radars = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            radars.push(self.row_to_radar(row)?);
        }
        Ok(radars)
    }

    fn row_to_radar(&self, row: &Row) -> Result<Radar> {
        Ok(Radar {
            id: row.get(0)?,
            name: row.get(1)?,
            full_name: row.get(2)?,
            latitude: row.get(3)?,
            longitude: row.get(4)?,
            state: row.get(5)?,
            r#type: row.get(6)?,
            group: row.get(7)?,
        })
    }

    pub fn get_radar(&self, id: u32) -> Result<Radar> {
        let mut stmt = self.conn.prepare("SELECT * FROM radar WHERE id = (?)")?;
        let mut binding = stmt.query(params![id])?;
        let row = binding.next()?.unwrap();
        self.row_to_radar(row)
    }

    pub fn get_radar_data_layers(
        &self,
        id: RadarId,
        type_: &RadarType,
        max_frames: Option<u64>,
    ) -> Result<Vec<RadarImageDataLayer>> {
        let max_frames = max_frames.map(|i| i as i32);
        let max_frames = max_frames.unwrap_or(-1);
        let params = params![id, type_.id() as u8, max_frames];
        let sql = include_str!("../sql/get_radar_data_layers.sql");
        let mut stmt = self.conn.prepare(sql)?;
        let mut layers = Vec::new();
        let mut rows = stmt.query(params)?;
        while let Some(row) = rows.next()? {
            let radar_type_id: u8 = row.get(1)?;
            let image = RadarImageDataLayer {
                radar_id: row.get(0)?,
                radar_type: RadarType::from_id(radar_type_id as char)?,
                png_buf: row.get(2)?,
                datetime: Utc.timestamp_opt(row.get(3)?, 0).unwrap(),
                filename: row.get(4)?,
            };
            layers.push(image);
        }
        if layers.is_empty() {
            return Err(anyhow!(
                "No data layers found for radar ID {id} type {type_}"
            ));
        }
        layers.reverse();
        Ok(layers)
    }

    pub fn get_radar_data_layer_names(&mut self, radar_type: &RadarType) -> Result<Vec<String>> {
        debug!(
            "Loading existing radar data layer names from {}",
            self.path.display()
        );
        let params = params![radar_type.id() as u8];
        let sql = "SELECT filename FROM radar_data_layer WHERE radar_type_id = (?)";
        let mut stmt = self.conn.prepare(sql)?;
        let mut names = Vec::new();
        let mut rows = stmt.query(params)?;
        while let Some(row) = rows.next()? {
            names.push(row.get(0)?);
        }
        Ok(names)
    }
    pub fn get_radar_feature_layers(
        &mut self,
        id: RadarId,
        type_: &RadarType,
    ) -> Result<Vec<RadarImageFeatureLayer>> {
        let params = params![id, type_.size().id() as u8];
        let sql = include_str!("../sql/get_radar_feature_layers.sql");
        let mut stmt = self.conn.prepare(sql)?;
        let mut layers = Vec::new();
        let mut rows = stmt.query(params)?;

        while let Some(row) = rows.next()? {
            let layer_type: String = row.get(1)?;
            let radar_type_id: u8 = row.get(2)?;
            let layer = RadarImageFeatureLayer {
                radar_id: row.get(0)?,
                feature: RadarImageFeature::from_str(&layer_type)?,
                size: RadarType::from_id(radar_type_id as char)?,
                png_buf: row.get(3)?,
                filename: row.get(4)?,
            };
            layers.push(layer);
        }
        if layers.is_empty() {
            return Err(anyhow!(
                "No feature layers found for radar ID {id} type {type_}"
            ));
        }
        Ok(layers)
    }

    pub fn get_radar_legend(&mut self, radar_type: &RadarType) -> Result<RadarImageLegend> {
        let legend_type = radar_type.legend_type();
        let sql = "SELECT image FROM radar_legend WHERE id = (?)";
        let mut stmt = self.conn.prepare(sql)?;
        let mut binding = stmt.query(params![legend_type.id()])?;
        let row = binding.next()?.unwrap();
        let legend = RadarImageLegend {
            r#type: legend_type,
            png_buf: row.get_unwrap(0),
        };
        Ok(legend)
    }

    pub fn insert_radar_data_layers(&mut self, layers: &[RadarImageDataLayer]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut stmt = tx.prepare(include_str!("../sql/insert_radar_data_layer.sql"))?;
        for layer in layers {
            debug!(
                "Inserting data layer {} into {}",
                layer.filename,
                self.path.display()
            );
            stmt.execute(named_params! {
                ":image": layer.png_buf,
                ":radar_id": layer.radar_id,
                ":radar_type_id": layer.radar_type.id() as u8,
                ":timestamp": layer.datetime.timestamp(),
                ":filename": layer.filename,
            })?;
        }
        stmt.finalize()?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_radar_feature_layers(&mut self, layers: &[RadarImageFeatureLayer]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut stmt = tx.prepare(include_str!("../sql/insert_radar_feature_layer.sql"))?;
        for layer in layers {
            debug!(
                "Inserting feature layer {} into {}",
                layer.filename,
                self.path.display()
            );
            stmt.execute(named_params! {
                ":image": layer.png_buf,
                ":radar_id": layer.radar_id,
                ":radar_type_id": layer.size.id() as u8,
                ":feature": layer.feature.to_string(),
                ":filename": layer.filename,
            })?;
        }
        stmt.finalize()?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_radar_data_layers(&mut self, layers: &[RadarImageDataLayer]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let sql = "DELETE FROM radar_data_layer WHERE filename = (?)";
        let mut stmt = tx.prepare(sql)?;
        for layer in layers {
            debug!(
                "Deleting data layer {} from {}",
                layer.filename,
                self.path.display()
            );
            stmt.execute(params![layer.filename])?;
        }
        stmt.finalize()?;
        tx.commit()?;
        Ok(())
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
