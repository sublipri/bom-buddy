use chrono::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Observation {
    pub issue_time: DateTime<Utc>,
    pub observation_time: DateTime<Utc>,
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
    pub rain_since_9am: f32,
    pub humidity: u8,
    pub station: Station,
}

impl From<ObservationResponse> for Observation {
    fn from(response: ObservationResponse) -> Self {
        Observation {
            issue_time: response.metadata.issue_time,
            observation_time: response.metadata.observation_time,
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
    pub rain_since_9am: f32,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PastObservationData {
    #[serde(rename = "TDZ")]
    pub tdz: String,
    pub aifstime_local: String,
    pub aifstime_utc: String,
    pub air_temp: f64,
    pub apparent_t: f64,
    pub cloud: String,
    pub cloud_base_m: Option<u32>,
    pub cloud_oktas: Option<u32>,
    pub cloud_type: String,
    pub cloud_type_id: Option<String>, // Uncertain type
    pub delta_t: f64,
    pub dewpt: f64,
    pub duration_from_local_9am_date: i64,
    pub gust_kmh: i64,
    pub gust_kt: i64,
    pub history_product: String,
    pub lat: f64,
    pub local_9am_date_time: String,
    pub local_9am_date_time_utc: String,
    pub lon: f64,
    pub name: String,
    pub press: f64,
    pub press_msl: f64,
    pub press_qnh: f64,
    pub press_tend: String,
    pub rain_hour: f64,
    pub rain_ten: f64,
    pub rain_trace: String,
    pub rain_trace_time: String,
    pub rain_trace_time_utc: String,
    pub rel_hum: i64,
    pub sea_state: String,
    pub sort_order: i64,
    pub swell_dir_worded: String,
    // http://www.bom.gov.au/marine/knowledge-centre/reference/waves.shtml
    pub swell_height: Option<f64>, // Uncertain type
    pub swell_period: Option<i64>, // Uncertain type
    pub time_zone_name: String,
    pub vis_km: String,
    pub weather: String,
    pub wind_dir: String,
    pub wind_dir_deg: i64,
    pub wind_spd_kmh: i64,
    pub wind_spd_kt: i64,
    pub wind_src: String,
    pub wmo: i64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PastObservationsHeader {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Office")]
    pub office: String,
    #[serde(rename = "TDZ")]
    pub tdz: String,
    pub issue_time_local: String,
    pub issue_time_utc: String,
    #[serde(rename = "main_ID")]
    pub main_id: String,
    pub name: String,
    pub product_name: String,
    pub state: String,
    pub state_time_zone: String,
    pub time_zone: String,
    pub time_zone_name: String,
    pub wmo_id: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PastObservationsNotice {
    pub copyright: String,
    pub copyright_url: String,
    pub disclaimer_url: String,
    pub feedback_url: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PastObservations {
    pub data: Vec<PastObservationData>,
    pub header: Vec<PastObservationsHeader>,
    pub notice: Vec<PastObservationsNotice>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PastObservationsResponse {
    pub observations: PastObservations,
}
