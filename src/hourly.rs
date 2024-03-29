use crate::descriptor::IconDescriptor;
use chrono::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct HourlyForecast {
    pub issue_time: DateTime<Utc>,
    pub data: Vec<HourlyForecastData>,
}

impl From<HourlyResponse> for HourlyForecast {
    fn from(response: HourlyResponse) -> Self {
        HourlyForecast {
            issue_time: response.metadata.issue_time,
            data: response.data,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HourlyForecastData {
    pub rain: HourlyForecastRain,
    pub temp: f32,
    pub temp_feels_like: f32,
    pub wind: HourlyForecastWind,
    pub relative_humidity: u8,
    pub uv: u8,
    pub icon_descriptor: IconDescriptor,
    pub next_three_hourly_forecast_period: DateTime<Utc>,
    pub time: DateTime<Utc>,
    pub is_night: bool,
    pub next_forecast_period: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HourlyResponse {
    pub metadata: HourlyForecastMetadata,
    pub data: Vec<HourlyForecastData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HourlyForecastMetadata {
    pub issue_time: DateTime<Utc>,
    pub response_timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HourlyForecastRain {
    pub amount: HourlyForecastRainAmount,
    pub chance: u8,
    pub precipitation_amount_10_percent_chance: u8,
    pub precipitation_amount_25_percent_chance: u8,
    pub precipitation_amount_50_percent_chance: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HourlyForecastRainAmount {
    pub max: Option<u16>,
    pub min: u16,
    pub units: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HourlyForecastWind {
    pub direction: String,
    pub speed_kilometre: u8,
    pub speed_knot: u8,
    pub gust_speed_kilometre: u8,
    pub gust_speed_knot: u8,
}
