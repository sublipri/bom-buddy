use crate::client::Client;
use crate::radar::{Radar, RadarId};
use crate::{
    location::{Location, SearchResult},
    persistence::Database,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use geo::{HaversineDistance, Point, RhumbBearing};
use std::fmt::{self, Display};

pub fn create_location(
    result: SearchResult,
    client: &Client,
    database: &Database,
) -> Result<Location> {
    let location_data = client.get_location(&result.geohash)?;
    let weather = client.get_weather(&result.geohash)?;
    let station = if let Some(obs) = weather.observation() {
        Some(database.get_station(obs.station.bom_id.parse()?)?)
    } else {
        None
    };

    let location = Location {
        geohash: result.geohash,
        station,
        has_wave: location_data.has_wave,
        id: result.id,
        name: result.name,
        state: result.state,
        postcode: result.postcode,
        latitude: location_data.latitude,
        longitude: location_data.longitude,
        marine_area_id: location_data.marine_area_id,
        tidal_point: location_data.tidal_point,
        timezone: location_data.timezone,
        weather,
    };

    database.insert_location(&location)?;

    Ok(location)
}

#[derive(Debug)]
pub struct NearbyRadar {
    pub id: RadarId,
    pub name: String,
    pub distance: i32,
    pub direction: String,
}

impl Display for NearbyRadar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} - {}km {}", self.name, self.distance, self.direction)
    }
}

pub fn get_nearby_radars(location: &Location, radars: &[Radar]) -> Vec<NearbyRadar> {
    let location_point = Point::new(location.longitude, location.latitude);

    let mut nearby_radars = Vec::new();
    for radar in radars {
        let radar_point = Point::new(radar.longitude as f64, radar.latitude as f64);
        let distance = location_point.haversine_distance(&radar_point);
        let distance = (distance / 1000.0) as i32;
        let bearing = location_point.rhumb_bearing(radar_point);

        let direction = match bearing {
            x if x < 22.5 => "N",
            x if x < 67.5 => "NE",
            x if x < 112.5 => "E",
            x if x < 157.5 => "SE",
            x if x < 202.5 => "S",
            x if x < 247.5 => "SW",
            x if x < 292.5 => "W",
            x if x < 337.5 => "NW",
            _ => "N",
        };

        nearby_radars.push(NearbyRadar {
            id: radar.id,
            name: radar.full_name.clone(),
            distance,
            direction: direction.into(),
        })
    }

    nearby_radars.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
    let new_len = nearby_radars
        .iter()
        .position(|r| r.distance > 200)
        .unwrap_or(1)
        .max(1);
    nearby_radars.truncate(new_len);
    nearby_radars
}

pub fn ids_to_locations(
    location_ids: &Vec<String>,
    client: &Client,
    database: &Database,
) -> Result<Vec<Location>> {
    if let Ok(locations) = database.get_locations(location_ids) {
        return Ok(locations);
    }
    let mut locations = Vec::new();

    for id in location_ids {
        let Some((name, geohash)) = id.split_once('-') else {
            return Err(anyhow!("{} is not a valid Location ID", id));
        };

        let results = client.search(name)?;
        if results.is_empty() {
            return Err(anyhow!("No results found for {}", id));
        }

        let Some(result) = results
            .iter()
            .find(|r| r.name == name && r.geohash == geohash)
        else {
            let mut ids = results[0].id.to_owned();
            for r in results.iter().skip(1) {
                ids.push('\n');
                ids.push_str(&r.id);
            }
            return Err(anyhow!(
                "No matches found for {}. Perhaps you meant:\n{}",
                id,
                ids
            ));
        };

        let location = create_location(result.clone(), client, database)?;
        locations.push(location);
    }
    Ok(locations)
}

pub fn update_if_due(
    locations: &mut Vec<Location>,
    client: &Client,
    database: &Database,
) -> Result<DateTime<Utc>> {
    let mut next_datetimes = Vec::with_capacity(locations.len());
    for location in locations {
        let (was_updated, next_check) = location.weather.update_if_due(client)?;
        if was_updated {
            database.update_weather(location)?;
        }
        next_datetimes.push(next_check);
    }
    Ok(*next_datetimes.iter().min().unwrap())
}
