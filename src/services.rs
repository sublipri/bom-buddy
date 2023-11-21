use crate::client::Client;
use crate::{
    location::{Location, SearchResult},
    persistence::Database,
};
use anyhow::{anyhow, Result};

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
