use crate::daily::{DailyForecast, DailyResponse};
use crate::hourly::{HourlyForecast, HourlyResponse};
use crate::location::{Location, LocationData, LocationResponse, SearchResponse, SearchResult};
use crate::observation::{Observation, ObservationResponse};
use crate::warning::{Warning, WarningResponse};
use crate::weather::Weather;
use anyhow::anyhow;
use anyhow::Result;
use chrono::Utc;
use std::collections::VecDeque;
use std::mem::take;
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, error};
use ureq::{Agent, AgentBuilder, Error, Response};

const URL_BASE: &str = "https://api.weather.bom.gov.au/v1/locations";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Safari/537.36";

#[derive(Debug)]
pub struct Client {
    client: Agent,
    retry_limit: u64,
    retry_delay: u64,
}

impl Client {
    pub fn new() -> Client {
        let client = AgentBuilder::new()
            .timeout_read(Duration::from_secs(5))
            .timeout_write(Duration::from_secs(5))
            .user_agent(USER_AGENT)
            .build();
        Client {
            client,
            retry_limit: 5,
            retry_delay: 7,
        }
    }

    fn get(&self, url: &str) -> Result<Response> {
        debug!("Fetching {url}");
        let mut attemps = 0;
        while attemps < self.retry_limit {
            let mut retry_delay = self.retry_delay;
            match self.client.get(url).call() {
                Ok(response) => {
                    return Ok(response);
                }
                Err(Error::Status(code, response)) => match code {
                    503 | 429 | 408 => {
                        if let Some(header) = response.header("retry-after") {
                            retry_delay = header.parse()?;
                        }
                        error!("{} for {}", code, url);
                        attemps += 1;
                    }
                    _ => {
                        let error = response.into_string()?;
                        error!("{code}: {error}");
                        return Err(anyhow!("{error}"));
                    }
                },
                Err(err) => {
                    let message = err.into_transport().unwrap().to_string();
                    error!("{message}");
                    attemps += 1;
                }
            }
            debug!("Retrying in {} seconds", retry_delay);
            sleep(Duration::from_secs(retry_delay));
        }
        Err(anyhow!("Retry limit exceeded"))
    }

    fn get_string(&self, url: &str) -> Result<String> {
        let response = self.get(url)?;
        Ok(response.into_string()?)
    }

    fn get_json(&self, url: &str) -> Result<serde_json::Value> {
        let string = self.get_string(url)?;
        let value = match serde_json::from_str(&string) {
            Ok(json) => json,
            Err(e) => {
                debug!("{:?}", &string);
                return Err(anyhow!("Unable to decode JSON. {e}"));
            }
        };
        Ok(value)
    }

    pub fn search(&self, term: &str) -> Result<Vec<SearchResult>> {
        let url = format!("{URL_BASE}?search={term}");
        let response: SearchResponse = serde_json::from_value(self.get_json(&url)?)?;
        debug!(
            "Search term {} returned {} results.",
            term,
            response.data.len()
        );
        for result in &response.data {
            debug!("{:#?}", result);
        }
        Ok(response.data)
    }

    // Search results contain a 7 character geohash but other endpoints expect 6.
    pub fn get_observation(&self, geohash: &str) -> Result<Observation> {
        let url = format!("{URL_BASE}/{}/observations", &geohash[..6]);
        let response: ObservationResponse = serde_json::from_value(self.get_json(&url)?)?;
        Ok(response.into())
    }

    pub fn get_daily(&self, geohash: &str) -> Result<DailyForecast> {
        let url = format!("{URL_BASE}/{}/forecasts/daily", &geohash[..6]);
        let response: DailyResponse = serde_json::from_value(self.get_json(&url)?)?;
        Ok(response.into())
    }

    pub fn get_hourly(&self, geohash: &str) -> Result<HourlyForecast> {
        let url = format!("{URL_BASE}/{}/forecasts/hourly", &geohash[..6]);
        let response: HourlyResponse = serde_json::from_value(self.get_json(&url)?)?;
        Ok(response.into())
    }

    pub fn get_warnings(&self, geohash: &str) -> Result<Vec<Warning>> {
        let url = format!("{URL_BASE}/{}/warnings", &geohash[..6]);
        let response: WarningResponse = serde_json::from_value(self.get_json(&url)?)?;
        Ok(response.data)
    }

    pub fn get_weather(&self, geohash: &str) -> Result<Weather> {
        Ok(Weather {
            daily_forecast: self.get_daily(geohash)?,
            hourly_forecast: self.get_hourly(geohash)?,
            observation: self.get_observation(geohash)?,
            past_observations: VecDeque::new(),
            warnings: self.get_warnings(geohash)?,
        })
    }

    pub fn get_location(&self, geohash: &str) -> Result<LocationData> {
        let url = format!("{URL_BASE}/{}", &geohash[..6]);
        let response: LocationResponse = serde_json::from_value(self.get_json(&url)?)?;
        Ok(response.data)
    }

    pub fn get_station_list(&self) -> Result<String> {
        let url = "https://reg.bom.gov.au/climate/data/lists_by_element/stations.txt";
        self.get_string(url)
    }

    pub fn update_if_due(&self, location: &mut Location) -> Result<bool> {
        let now = Utc::now();
        let mut was_updated = false;
        let weather = &mut location.weather;
        if now > weather.observation.next_issue_time {
            let observation = self.get_observation(&location.geohash)?;
            if observation.issue_time != weather.observation.issue_time {
                let past = take(&mut weather.observation);
                weather.observation = observation;
                weather.past_observations.push_front(past);
                if weather.past_observations.len() > 72 {
                    weather.past_observations.pop_back();
                }
                was_updated = true;
            }
            let warnings = self.get_warnings(&location.geohash)?;
            if warnings != weather.warnings {
                was_updated = true;
            }
        }

        if now > weather.hourly_forecast.next_issue_time {
            let hourly = self.get_hourly(&location.geohash)?;
            weather.hourly_forecast = hourly;
            was_updated = true;
        }

        if now > weather.daily_forecast.next_issue_time {
            let daily = self.get_daily(&location.geohash)?;
            weather.daily_forecast = daily;
            was_updated = true;
        }

        Ok(was_updated)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
