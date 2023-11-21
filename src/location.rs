use crate::station::WeatherStation;
use crate::weather::Weather;
use serde::{Deserialize, Serialize};
use std::fmt;
use strum_macros::{Display, EnumString};

#[derive(Debug, Deserialize, Serialize)]
pub struct Location {
    pub geohash: String,
    pub station: Option<WeatherStation>,
    pub has_wave: bool,
    pub id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub marine_area_id: Option<String>,
    pub name: String,
    pub state: State,
    pub postcode: String,
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
    pub state: State,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub geohash: String,
    pub id: String,
    pub name: String,
    pub postcode: String,
    pub state: State,
}

impl fmt::Display for SearchResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} {}", self.name, self.state, self.postcode)
    }
}

#[derive(Serialize, Deserialize)]
pub struct SearchMetadata {
    pub copyright: String,
    pub response_timestamp: String,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResponse {
    pub data: Vec<SearchResult>,
    pub metadata: SearchMetadata,
}

#[derive(Clone, Debug, Display, Deserialize, Serialize, Eq, PartialEq, EnumString)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum State {
    Act,
    Nsw,
    Vic,
    Qld,
    Tas,
    Sa,
    Nt,
    Wa,
}

impl State {
    pub fn get_product_code(&self, id: &str) -> String {
        let prefix = match self {
            State::Nt => "IDD",
            State::Nsw => "IDN",
            State::Act => "IDN",
            State::Qld => "IDQ",
            State::Sa => "IDS",
            State::Tas => "IDT",
            State::Vic => "IDV",
            State::Wa => "IDW",
        };

        format!("{prefix}{id}")
    }
}
