use crate::daily::{DailyForecast, DailyResponse};
use crate::hourly::{HourlyForecast, HourlyResponse};
use crate::location::{Location, LocationData, LocationResponse, SearchResponse, SearchResult};
use crate::observation::{
    Observation, ObservationResponse, PastObservationData, PastObservationsResponse,
};
use crate::warning::{Warning, WarningResponse};
use crate::weather::{Weather, WeatherOptions};
use anyhow::anyhow;
use anyhow::Result;
use chrono::Duration;
use chrono::Utc;
use std::collections::VecDeque;
use std::thread::sleep;
use tracing::{debug, error, trace};
use ureq::{Agent, AgentBuilder, Error, Response};

const URL_BASE: &str = "https://api.weather.bom.gov.au/v1/locations";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Safari/537.36";

#[derive(Debug)]
pub struct ClientOptions {
    pub retry_limit: u64,
    pub retry_delay: Duration,
    pub timeout: Duration,
    pub timeout_connect: Duration,
    pub user_agent: String,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            retry_limit: 5,
            retry_delay: Duration::seconds(7),
            timeout: Duration::seconds(7),
            timeout_connect: Duration::seconds(30),
            user_agent: USER_AGENT.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Client {
    client: Agent,
    opts: ClientOptions,
}

impl Client {
    pub fn new(opts: ClientOptions) -> Client {
        let client = AgentBuilder::new()
            .timeout_read(opts.timeout.to_std().unwrap())
            .timeout_write(opts.timeout.to_std().unwrap())
            .timeout_connect(opts.timeout_connect.to_std().unwrap())
            .user_agent(&opts.user_agent)
            .build();
        Client { client, opts }
    }

    fn get(&self, url: &str) -> Result<Response> {
        debug!("Fetching {url}");
        let mut attemps = 0;
        while attemps < self.opts.retry_limit {
            let mut retry_delay = self.opts.retry_delay;
            match self.client.get(url).call() {
                Ok(response) => {
                    return Ok(response);
                }
                Err(Error::Status(code, response)) => match code {
                    503 | 429 | 408 => {
                        if let Some(header) = response.header("retry-after") {
                            retry_delay = Duration::seconds(header.parse()?);
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
            debug!("Retrying in {} seconds", retry_delay.num_seconds());
            sleep(retry_delay.to_std()?);
        }
        Err(anyhow!("Retry limit exceeded"))
    }

    fn get_string(&self, url: &str) -> Result<String> {
        let response = self.get(url)?;
        let string = response.into_string()?;
        trace!("{}", &string);
        Ok(string)
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
    pub fn get_observation(&self, geohash: &str) -> Result<Option<Observation>> {
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
        let now = Utc::now();
        let opts = WeatherOptions::default();

        let daily_forecast = self.get_daily(geohash)?;
        let mut next_daily_due = if let Some(next) = daily_forecast.next_issue_time {
            next + opts.update_delay
        } else {
            daily_forecast.issue_time + opts.daily_update_frequency + opts.update_delay
        };
        if now > next_daily_due {
            next_daily_due = now + opts.daily_overdue_delay;
        }

        let hourly_forecast = self.get_hourly(geohash)?;
        let mut next_hourly_due =
            hourly_forecast.issue_time + opts.hourly_update_frequency + opts.update_delay;
        if now > next_hourly_due {
            next_hourly_due = now + opts.hourly_overdue_delay;
        }

        let mut observations = VecDeque::new();
        let mut next_observation_due = now + opts.observation_missing_delay;
        if let Some(observation) = self.get_observation(geohash)? {
            next_observation_due =
                observation.issue_time + opts.observation_update_frequency + opts.update_delay;
            if now > next_observation_due {
                next_observation_due = now + opts.observation_overdue_delay;
            }
            observations.push_front(observation);
        }

        let warnings = self.get_warnings(geohash)?;
        let next_warning_due = now + opts.warning_update_frequency;

        Ok(Weather {
            geohash: geohash.to_string(),
            observations,
            daily_forecast,
            hourly_forecast,
            warnings,
            next_observation_due,
            next_daily_due,
            next_hourly_due,
            next_warning_due,
            opts,
        })
    }

    pub fn get_location(&self, geohash: &str) -> Result<LocationData> {
        let url = format!("{URL_BASE}/{}", &geohash[..6]);
        let response: LocationResponse = serde_json::from_value(self.get_json(&url)?)?;
        Ok(response.data)
    }

    pub fn get_past_observations(&self, location: &Location) -> Result<Vec<PastObservationData>> {
        let Some(station) = &location.station else {
            return Err(anyhow!("{} doesn't have a weather station", location.id));
        };
        let Some(wmo_id) = station.wmo_id else {
            return Err(anyhow!("{} doesn't have a WMO ID", station.name));
        };
        let code = location.state.get_product_code("60910");
        let url = format!("https://reg.bom.gov.au/fwo/{code}/{code}.{wmo_id}.json");
        let response: PastObservationsResponse = serde_json::from_value(self.get_json(&url)?)?;
        Ok(response.observations.data)
    }

    pub fn get_station_list(&self) -> Result<String> {
        let url = "https://reg.bom.gov.au/climate/data/lists_by_element/stations.txt";
        self.get_string(url)
    }
}

impl Default for Client {
    fn default() -> Self {
        let opts = ClientOptions::default();
        Self::new(opts)
    }
}
