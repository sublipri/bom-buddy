use crate::daily::{DailyForecast, DailyResponse};
use crate::hourly::{HourlyForecast, HourlyResponse};
use crate::location::{Location, LocationResponse};
use crate::observation::{Observation, ObservationResponse};
use crate::search::{SearchResponse, SearchResult};
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
use ureq::{Agent, AgentBuilder, Error};

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

    fn get(&self, url: &str) -> Result<serde_json::Value> {
        debug!("Fetching {url}");
        let mut attemps = 0;
        while attemps < self.retry_limit {
            match self.client.get(url).call() {
                Ok(response) => {
                    let json = response.into_json()?;
                    return Ok(json);
                }
                Err(Error::Status(code, response)) => match code {
                    503 | 429 | 408 => {
                        let retry = if let Some(header) = response.header("retry-after") {
                            header.parse()?
                        } else {
                            self.retry_delay
                        };
                        error!("{} for {}, retry in {}", code, url, retry);
                        attemps += 1;
                        sleep(Duration::from_secs(retry));
                    }
                    _ => {
                        let error = response.into_string()?;
                        error!("{code}: {error}");
                        return Err(anyhow!("{error}"));
                    }
                },
                Err(err) => {
                    let err = err.into_transport().unwrap();
                    let error = err.to_string();
                    error!("{error}");
                    if let Some(message) = err.message() {
                        error!("{message}");
                    }
                    return Err(anyhow!("{error}"));
                }
            }
        }
        Err(anyhow!("Retry limit exceeded"))
    }

    pub fn search(&self, term: &str) -> Result<Vec<SearchResult>> {
        let url = format!("{URL_BASE}?search={term}");
        let response: SearchResponse = serde_json::from_value(self.get(&url)?)?;
        Ok(response.data)
    }

    // Search results contain a 7 character geohash but endpoints expect 6.
    pub fn get_observation(&self, geohash: &str) -> Result<Observation> {
        let url = format!("{URL_BASE}/{}/observations", &geohash[..6]);
        let response: ObservationResponse = serde_json::from_value(self.get(&url)?)?;
        Ok(response.into())
    }

    pub fn get_daily(&self, geohash: &str) -> Result<DailyForecast> {
        let url = format!("{URL_BASE}/{}/forecasts/daily", &geohash[..6]);
        let response: DailyResponse = serde_json::from_value(self.get(&url)?)?;
        Ok(response.into())
    }

    pub fn get_hourly(&self, geohash: &str) -> Result<HourlyForecast> {
        let url = format!("{URL_BASE}/{}/forecasts/hourly", &geohash[..6]);
        let response: HourlyResponse = serde_json::from_value(self.get(&url)?)?;
        Ok(response.into())
    }

    pub fn get_warnings(&self, geohash: &str) -> Result<Vec<Warning>> {
        let url = format!("{URL_BASE}/{}/warnings", &geohash[..6]);
        let response: WarningResponse = serde_json::from_value(self.get(&url)?)?;
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

    pub fn get_location(&self, geohash: &str) -> Result<Location> {
        let url = format!("{URL_BASE}/{}", &geohash[..6]);
        let response: LocationResponse = serde_json::from_value(self.get(&url)?)?;
        let weather = self.get_weather(geohash)?;
        let location = Location {
            geohash: response.data.geohash,
            weather,
            has_wave: response.data.has_wave,
            id: response.data.id,
            latitude: response.data.latitude,
            longitude: response.data.longitude,
            marine_area_id: response.data.marine_area_id,
            name: response.data.name,
            state: response.data.state,
            tidal_point: response.data.tidal_point,
            timezone: response.data.timezone,
        };

        Ok(location)
    }

    pub fn update_if_due(&self, geohash: &str, weather: &mut Weather) -> Result<bool> {
        let now = Utc::now();
        let mut was_updated = false;
        if now > weather.observation.next_issue_time {
            let observation = self.get_observation(geohash)?;
            if observation.issue_time != weather.observation.issue_time {
                let past = take(&mut weather.observation);
                weather.observation = observation;
                weather.past_observations.push_front(past);
                if weather.past_observations.len() > 72 {
                    weather.past_observations.pop_back();
                }
                was_updated = true;
            }
            let warnings = self.get_warnings(geohash)?;
            if warnings != weather.warnings {
                was_updated = true;
            }
        }

        if now > weather.hourly_forecast.next_issue_time {
            let hourly = self.get_hourly(geohash)?;
            weather.hourly_forecast = hourly;
            was_updated = true;
        }

        if now > weather.daily_forecast.next_issue_time {
            let daily = self.get_daily(geohash)?;
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
