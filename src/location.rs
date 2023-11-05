use serde::{Deserialize, Serialize};

use crate::weather::Weather;

#[derive(Debug, Deserialize, Serialize)]
pub struct Location {
    pub geohash: String,
    pub has_wave: bool,
    pub id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub marine_area_id: Option<String>,
    pub name: String,
    pub state: String,
    pub tidal_point: Option<String>,
    pub timezone: String,
    pub weather: Weather,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LocationData {
    pub geohash: String,
    pub has_wave: bool,
    pub id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub marine_area_id: Option<String>,
    pub name: String,
    pub state: String,
    pub tidal_point: Option<String>,
    pub timezone: String,
}

#[derive(Serialize, Deserialize)]
pub struct LocationMetadata {
    pub copyright: String,
    pub response_timestamp: String,
}

#[derive(Serialize, Deserialize)]
pub struct LocationResponse {
    pub data: LocationData,
    pub metadata: LocationMetadata,
}
