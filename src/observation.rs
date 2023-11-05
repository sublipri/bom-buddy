use chrono::{prelude::*, Duration};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Observation {
    pub issue_time: DateTime<Utc>,
    pub observation_time: DateTime<Utc>,
    pub next_issue_time: DateTime<Utc>,
    pub temp: f32,
    pub temp_feels_like: f32,
    pub wind_speed: u8,
    pub wind_direction: String,
    pub gust: u8,
    pub max_gust: u8,
    pub max_gust_time: DateTime<Utc>,
    pub max_temp: f32,
    pub max_temp_time: DateTime<Utc>,
    pub min_temp: f32,
    pub min_temp_time: DateTime<Utc>,
    pub rain_since_9am: u32,
    pub humidity: u8,
    pub station: Station,
}

impl From<ObservationResponse> for Observation {
    fn from(response: ObservationResponse) -> Self {
        let now = Utc::now();
        let mut next_issue_time = response.metadata.issue_time + Duration::minutes(31);
        if now > next_issue_time {
            next_issue_time = now + Duration::minutes(1);
        }

        Observation {
            issue_time: response.metadata.issue_time,
            observation_time: response.metadata.observation_time,
            next_issue_time,
            temp: response.data.temp,
            temp_feels_like: response.data.temp_feels_like,
            wind_speed: response.data.wind.speed_kilometre,
            wind_direction: response.data.wind.direction,
            gust: response.data.gust.speed_kilometre,
            max_gust: response.data.max_gust.speed_kilometre,
            max_gust_time: response.data.max_gust.time,
            max_temp: response.data.max_temp.value,
            max_temp_time: response.data.max_temp.time,
            min_temp: response.data.min_temp.value,
            min_temp_time: response.data.min_temp.time,
            rain_since_9am: response.data.rain_since_9am,
            humidity: response.data.humidity,
            station: response.data.station,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ObservationResponse {
    pub data: ObservationData,
    pub metadata: ObservationMetadata,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ObservationData {
    pub temp: f32,
    pub temp_feels_like: f32,
    pub wind: Wind,
    pub gust: Gust,
    pub max_gust: MaxGust,
    pub max_temp: Temperature,
    pub min_temp: Temperature,
    pub rain_since_9am: u32,
    pub humidity: u8,
    pub station: Station,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ObservationMetadata {
    pub issue_time: DateTime<Utc>,
    pub observation_time: DateTime<Utc>,
    pub response_timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Gust {
    pub speed_kilometre: u8,
    pub speed_knot: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MaxGust {
    pub speed_kilometre: u8,
    pub speed_knot: u8,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Wind {
    pub direction: String,
    pub speed_kilometre: u8,
    pub speed_knot: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Temperature {
    pub time: DateTime<Utc>,
    pub value: f32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Station {
    pub bom_id: String,
    pub distance: f64,
    pub name: String,
}
