use crate::client::Client;
use crate::hourly::HourlyForecast;
use crate::observation::Observation;
use crate::warning::Warning;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_with::DurationSeconds;
use std::collections::VecDeque;
use tracing::debug;

#[derive(Debug, Serialize, Deserialize)]
pub struct Weather {
    pub geohash: String,
    pub observations: VecDeque<Observation>,
    pub daily_forecast: DailyForecast,
    pub hourly_forecast: HourlyForecast,
    pub warnings: Vec<Warning>,
    pub next_observation_due: DateTime<Utc>,
    pub next_daily_due: DateTime<Utc>,
    pub next_hourly_due: DateTime<Utc>,
    pub next_warning_due: DateTime<Utc>,
    pub opts: WeatherOptions,
}

#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct WeatherOptions {
    pub past_observation_amount: usize,
    /// A delay to account for lag between issue time and appearance in API
    #[serde_as(as = "DurationSeconds<i64>")]
    pub update_delay: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub observation_update_frequency: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub observation_overdue_delay: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub hourly_update_frequency: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub hourly_overdue_delay: Duration,
    /// Only used when a DailyForecast has no next_issue_time
    #[serde_as(as = "DurationSeconds<i64>")]
    pub daily_update_frequency: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub daily_overdue_delay: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub warning_update_frequency: Duration,
}

impl Default for WeatherOptions {
    fn default() -> Self {
        Self {
            past_observation_amount: 6 * 24 * 2,
            update_delay: Duration::minutes(2),
            observation_update_frequency: Duration::minutes(10),
            observation_overdue_delay: Duration::minutes(2),
            hourly_update_frequency: Duration::hours(3),
            hourly_overdue_delay: Duration::minutes(30),
            daily_update_frequency: Duration::hours(1),
            daily_overdue_delay: Duration::minutes(30),
            warning_update_frequency: Duration::minutes(30),
        }
    }
}

impl Weather {
    pub fn observation(&self) -> Option<&Observation> {
        self.observations.front()
    }
    pub fn update_if_due(&mut self, client: &Client) -> Result<bool> {
        let now = Utc::now();
        let mut was_updated = false;

        if now > self.next_observation_due {
            let observation = client.get_observation(&self.geohash)?;
            was_updated = self.update_observation(now, observation);
        }

        if now > self.next_hourly_due {
            let hourly = client.get_hourly(&self.geohash)?;
            was_updated = self.update_hourly(now, hourly);
        }

        if now > self.next_daily_due {
            let daily = client.get_daily(&self.geohash)?;
            was_updated = self.update_daily(now, daily);
        }

        if now > self.next_warning_due {
            let warnings = client.get_warnings(&self.geohash)?;
            if warnings != self.warnings {
                was_updated = true;
            }
            self.next_warning_due = now + self.opts.warning_update_frequency;
        }

        Ok(was_updated)
    }

    fn update_observation(&mut self, now: DateTime<Utc>, observation: Observation) -> bool {
        if let Some(last) = self.observation() {
            if observation.issue_time == last.issue_time {
                debug!(
                    "{} observation overdue. Next check in {} minutes",
                    &self.geohash,
                    &self.opts.observation_overdue_delay.num_minutes()
                );
                self.next_observation_due = now + self.opts.observation_overdue_delay;
                return false;
            }
        }

        self.next_observation_due = observation.issue_time
            + self.opts.observation_update_frequency
            + self.opts.update_delay;
        if now > self.next_observation_due {
            self.next_observation_due = now + self.opts.observation_overdue_delay;
        }

        self.observations.push_front(observation);
        if self.observations.len() > self.opts.past_observation_amount {
            self.observations.pop_back();
        }

        true
    }

    fn update_hourly(&mut self, now: DateTime<Utc>, hourly: HourlyForecast) -> bool {
        let last = &self.hourly_forecast;
        if hourly.issue_time == last.issue_time {
            debug!(
                "{} hourly forecast overdue. Next check in {} minutes",
                &self.geohash,
                &self.opts.hourly_overdue_delay.num_minutes()
            );
            self.next_hourly_due = now + self.opts.hourly_overdue_delay;
            return false;
        }

        self.next_hourly_due =
            hourly.issue_time + self.opts.hourly_update_frequency + self.opts.update_delay;
        self.hourly_forecast = hourly;
        true
    }

    fn update_daily(&mut self, now: DateTime<Utc>, new_daily: DailyForecast) -> bool {
        let last = &self.daily_forecast;
        if new_daily.issue_time == last.issue_time {
            debug!(
                "{} daily forecast overdue. Next check in {} minutes",
                &self.geohash,
                &self.opts.daily_overdue_delay.num_minutes()
            );
            self.next_daily_due = now + self.opts.daily_overdue_delay;
            return false;
        }

        self.next_daily_due = if let Some(next) = new_daily.next_issue_time {
            next + self.opts.update_delay
        } else {
            new_daily.issue_time + self.opts.daily_update_frequency + self.opts.update_delay
        };
        self.daily_forecast = new_daily;
        true
    }
}
