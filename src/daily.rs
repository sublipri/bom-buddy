use crate::descriptor::IconDescriptor;
use chrono::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyForecast {
    pub issue_time: DateTime<Utc>,
    pub next_issue_time: Option<DateTime<Utc>>,
    pub forecast_region: String,
    pub forecast_type: String,
    pub days: Vec<DailyForecastData>,
}

impl From<DailyResponse> for DailyForecast {
    fn from(response: DailyResponse) -> Self {
        DailyForecast {
            issue_time: response.metadata.issue_time,
            next_issue_time: response.metadata.next_issue_time,
            forecast_region: response.metadata.forecast_region,
            forecast_type: response.metadata.forecast_type,
            days: response.data,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DailyForecastData {
    pub rain: Rain,
    pub uv: Uv,
    pub astronomical: Astronomical,
    pub date: DateTime<Utc>,
    pub temp_max: f32,
    pub temp_min: Option<f32>,
    pub extended_text: Option<String>,
    pub icon_descriptor: IconDescriptor,
    pub short_text: Option<String>,
    pub surf_danger: Option<String>,
    pub fire_danger: Option<String>,
    pub fire_danger_category: FireDangerCategory,
    pub now: Option<Now>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DailyForecastMetadata {
    pub issue_time: DateTime<Utc>,
    pub response_timestamp: DateTime<Utc>,
    pub forecast_region: String,
    pub forecast_type: String,
    pub next_issue_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DailyResponse {
    pub metadata: DailyForecastMetadata,
    pub data: Vec<DailyForecastData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Now {
    pub is_night: bool,
    pub now_label: String,
    pub later_label: String,
    pub temp_now: f32,
    pub temp_later: f32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Uv {
    pub category: Option<String>,
    pub max_index: Option<u8>,
    pub end_time: Option<DateTime<Utc>>,
    pub start_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Astronomical {
    pub sunrise_time: DateTime<Utc>,
    pub sunset_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FireDangerCategory {
    pub dark_mode_colour: Option<String>,
    pub default_colour: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Rain {
    pub amount: RainAmount,
    pub chance: u8,
    pub chance_of_no_rain_category: String,
    pub precipitation_amount_25_percent_chance: u8,
    pub precipitation_amount_50_percent_chance: u8,
    pub precipitation_amount_75_percent_chance: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RainAmount {
    pub lower_range: u16,
    pub upper_range: u16,
    pub min: u16,
    pub max: Option<u16>,
    pub units: String,
}
