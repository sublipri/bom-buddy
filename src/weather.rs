use crate::client::Client;
use crate::daily::DailyForecast;
use crate::descriptor::IconDescriptor;
use crate::hourly::HourlyForecast;
use crate::observation::Observation;
use crate::util::format_duration;
use crate::warning::Warning;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Local, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::DurationSeconds;
use std::{collections::VecDeque, str::FromStr};
use strum_macros::{AsRefStr, EnumIter, EnumString};
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
    pub check_observations: bool,
    /// A delay to account for lag between issue time and appearance in API
    #[serde_as(as = "DurationSeconds<i64>")]
    pub update_delay: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub observation_update_frequency: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub observation_overdue_delay: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub observation_missing_delay: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub hourly_update_frequency: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub hourly_overdue_delay: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub daily_update_frequency: Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub daily_overdue_delay: Duration,
    // The BOM will sometimes issue a new forecast before the advertised next_issue_time
    // so we provide the option to ignore that and just check at a regular interval
    pub use_daily_next_issue_time: bool,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub warning_update_frequency: Duration,
}

impl Default for WeatherOptions {
    fn default() -> Self {
        Self {
            past_observation_amount: 6 * 24 * 2,
            check_observations: true,
            update_delay: Duration::minutes(2),
            observation_update_frequency: Duration::minutes(10),
            observation_overdue_delay: Duration::minutes(2),
            observation_missing_delay: Duration::hours(1),
            hourly_update_frequency: Duration::hours(3),
            hourly_overdue_delay: Duration::hours(1),
            daily_update_frequency: Duration::hours(1),
            use_daily_next_issue_time: false,
            daily_overdue_delay: Duration::minutes(30),
            warning_update_frequency: Duration::minutes(30),
        }
    }
}

impl Weather {
    pub fn observation(&self) -> Option<&Observation> {
        self.observations.front()
    }
    pub fn update_if_due(&mut self, client: &Client) -> Result<(bool, DateTime<Utc>)> {
        let now = Utc::now();
        let mut was_updated = false;

        if self.opts.check_observations && now > self.next_observation_due {
            if let Some(observation) = client.get_observation(&self.geohash)? {
                self.update_observation(now, observation);
            } else {
                self.next_observation_due = now + self.opts.observation_missing_delay;
            }
            was_updated = true;
        }

        if now > self.next_hourly_due {
            let hourly = client.get_hourly(&self.geohash)?;
            self.update_hourly(now, hourly);
            was_updated = true;
        }

        if now > self.next_daily_due {
            let daily = client.get_daily(&self.geohash)?;
            self.update_daily(now, daily);
            was_updated = true;
        }

        if now > self.next_warning_due {
            self.warnings = client.get_warnings(&self.geohash)?;
            self.next_warning_due = now + self.opts.warning_update_frequency;
            was_updated = true;
        }

        let next_datetimes = [
            self.next_observation_due,
            self.next_hourly_due,
            self.next_daily_due,
            self.next_warning_due,
        ];
        let next_check = next_datetimes.iter().min().unwrap();

        Ok((was_updated, *next_check))
    }

    pub fn update_observation(&mut self, now: DateTime<Utc>, observation: Observation) {
        if let Some(last) = self.observation() {
            if observation.issue_time == last.issue_time {
                debug!(
                    "{} observation overdue. Next check in {}",
                    &self.geohash,
                    format_duration(self.opts.observation_overdue_delay)
                );
                self.next_observation_due = now + self.opts.observation_overdue_delay;
                return;
            }
        }

        self.next_observation_due = observation.issue_time
            + self.opts.observation_update_frequency
            + self.opts.update_delay;
        if now > self.next_observation_due {
            self.next_observation_due = now + self.opts.observation_overdue_delay;
        }

        debug!(
            "{} new observation received. Next check in {}",
            &self.geohash,
            format_duration(self.next_observation_due - now)
        );
        self.observations.push_front(observation);
        if self.observations.len() > self.opts.past_observation_amount {
            self.observations.pop_back();
        }
    }

    pub fn update_hourly(&mut self, now: DateTime<Utc>, hourly: HourlyForecast) {
        let last = &self.hourly_forecast;
        if hourly.issue_time == last.issue_time {
            debug!(
                "{} hourly forecast overdue. Next check in {}",
                &self.geohash,
                format_duration(self.opts.hourly_overdue_delay)
            );
            self.next_hourly_due = now + self.opts.hourly_overdue_delay;
            // Previous hours will be removed in the API response even if the issue time is the same
            self.hourly_forecast = hourly;
            return;
        }

        self.next_hourly_due =
            hourly.issue_time + self.opts.hourly_update_frequency + self.opts.update_delay;
        debug!(
            "{} new hourly forecast received. Next check in {}",
            &self.geohash,
            format_duration(self.next_hourly_due - now)
        );
        self.hourly_forecast = hourly;
    }

    pub fn update_daily(&mut self, now: DateTime<Utc>, new_daily: DailyForecast) {
        let last = &self.daily_forecast;
        if new_daily.issue_time == last.issue_time {
            self.next_daily_due = if self.opts.use_daily_next_issue_time {
                debug!(
                    "{} daily forecast overdue. Next check in {}",
                    &self.geohash,
                    format_duration(self.opts.daily_overdue_delay)
                );
                now + self.opts.daily_overdue_delay
            } else {
                debug!(
                    "{} No new daily forecast. Next check in {}",
                    &self.geohash,
                    format_duration(self.opts.daily_update_frequency)
                );
                now + self.opts.daily_update_frequency
            };
            return;
        }

        self.next_daily_due = if self.opts.use_daily_next_issue_time {
            if let Some(next) = new_daily.next_issue_time {
                next + self.opts.update_delay
            } else {
                new_daily.issue_time + self.opts.daily_update_frequency + self.opts.update_delay
            }
        } else {
            now + self.opts.daily_update_frequency
        };
        debug!(
            "{} new daily forecast received. Next check in {}",
            &self.geohash,
            format_duration(self.next_daily_due - now)
        );
        self.daily_forecast = new_daily;
    }

    pub fn current(&self) -> CurrentWeather {
        let now = Utc::now();
        let observation = self.observation();
        let hourly = self
            .hourly_forecast
            .data
            .iter()
            .find(|h| now > h.time)
            .unwrap();
        let mut days = self.daily_forecast.days.iter();
        let today = days.next().unwrap();
        let tomorrow = days.next().unwrap();
        // temp_max should only ever be None on the last day of the forecast
        // provide an obviously wrong value rather than crashing if it is None
        let today_max = today.temp_max.unwrap_or(-9999.0);
        let overnight_min = tomorrow.temp_min.unwrap_or(-9999.0);
        let tomorrow_max = tomorrow.temp_max.unwrap_or(-9999.0);

        let (temp, temp_feels_like, max_temp, wind_speed, wind_direction, gust) =
            if let Some(obs) = observation {
                let wind_direction = if let Some(dir) = &obs.wind.direction {
                    dir
                } else {
                    &hourly.wind.direction
                };
                (
                    obs.temp,
                    obs.temp_feels_like,
                    f32::max(obs.max_temp.value, today_max),
                    obs.wind.speed_kilometre,
                    wind_direction,
                    obs.gust.speed_kilometre,
                )
            } else {
                (
                    hourly.temp,
                    hourly.temp_feels_like,
                    today_max,
                    hourly.wind.speed_kilometre,
                    &hourly.wind.direction,
                    hourly.wind.gust_speed_kilometre,
                )
            };

        let current_time = now.with_timezone(&Local).time();
        let start = NaiveTime::from_hms_opt(6, 0, 0).unwrap();
        let end = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let next_is_max = current_time > start && current_time < end;

        let (next_temp, next_label, later_temp, later_label) = if next_is_max {
            (max_temp, "Max", overnight_min, "Overnight min")
        } else {
            (overnight_min, "Overnight min", tomorrow_max, "Tomorrow max")
        };

        CurrentWeather {
            temp,
            temp_feels_like,
            max_temp,
            next_temp,
            next_label,
            later_temp,
            later_label,
            overnight_min, // TODO: check what happens after midnight
            tomorrow_max,
            rain_since_9am: observation.and_then(|obs| obs.rain_since_9am),
            extended_text: &today.extended_text,
            short_text: &today.short_text,
            humidity: observation.as_ref().map(|obs| obs.humidity),
            hourly_rain_chance: hourly.rain.chance,
            hourly_rain_min: hourly.rain.amount.min,
            hourly_rain_max: hourly.rain.amount.max.unwrap_or(0),
            today_rain_chance: today.rain.chance.unwrap_or(0),
            today_rain_min: today.rain.amount.min.unwrap_or(0),
            today_rain_max: today.rain.amount.max.unwrap_or(0),
            wind_speed,
            wind_direction,
            gust,
            relative_humidity: hourly.relative_humidity,
            uv: hourly.uv,
            icon: hourly.icon_descriptor.get_icon_emoji(hourly.is_night),
            icon_descriptor: &hourly.icon_descriptor,
            is_night: hourly.is_night,
        }
    }
}

pub struct CurrentWeather<'a> {
    pub temp: f32,
    pub temp_feels_like: f32,
    pub max_temp: f32,
    pub next_temp: f32,
    pub later_temp: f32,
    pub next_label: &'a str,
    pub later_label: &'a str,
    pub overnight_min: f32,
    pub tomorrow_max: f32,
    pub rain_since_9am: Option<f32>,
    pub today_rain_chance: u8,
    pub today_rain_min: u16,
    pub today_rain_max: u16,
    pub hourly_rain_chance: u8,
    pub hourly_rain_min: u16,
    pub hourly_rain_max: u16,
    pub humidity: Option<u8>,
    pub relative_humidity: u8,
    pub uv: u8,
    pub icon: &'a str,
    pub short_text: &'a Option<String>,
    pub extended_text: &'a Option<String>,
    pub icon_descriptor: &'a IconDescriptor,
    pub is_night: bool,
    pub wind_speed: u8,
    pub wind_direction: &'a str,
    pub gust: u8,
}

impl<'a> CurrentWeather<'a> {
    /// Process a user-provided format string e.g. "{icon} {temp} ({temp_feels_like})".
    /// Just a basic implementation that doesn't handle mismatched curly brackets
    pub fn process_fstring(&self, fstring: &str) -> Result<String> {
        let mut pos = 0;
        let mut remainder = fstring;
        let mut output = String::new();
        while !remainder.is_empty() {
            if let Some(next) = remainder.find('{') {
                output.push_str(&remainder[..next]);
                let start = next + 1;
                let Some(end) = remainder.find('}') else {
                    return Err(anyhow!("{fstring} is not a valid format string"));
                };
                let key = &remainder[start..end];
                let Ok(fstring_key) = FstringKey::from_str(key) else {
                    return Err(anyhow!("{} is not a valid key", key));
                };
                fstring_key.push_value(&mut output, self);
                pos = pos + end + 1;
                remainder = &fstring[pos..];
            } else {
                output.push_str(remainder);
                break;
            }
        }

        Ok(output)
    }
}

#[derive(AsRefStr, EnumString, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum FstringKey {
    Temp,
    TempFeelsLike,
    Icon,
    NextTemp,
    NextLabel,
    LaterTemp,
    LaterLabel,
    MaxTemp,
    OvernightMin,
    TomorrowMax,
    #[strum(serialize = "rain_since_9am")]
    RainSince9am,
    HourlyRainChance,
    HourlyRainMin,
    HourlyRainMax,
    TodayRainChance,
    TodayRainMin,
    TodayRainMax,
    ShortText,
    ExtendedText,
    WindSpeed,
    WindDirection,
    WindGust,
}

impl FstringKey {
    fn push_value(&self, s: &mut String, w: &CurrentWeather) {
        match self {
            Self::Temp => s.push_str(&w.temp.to_string()),
            Self::TempFeelsLike => s.push_str(&w.temp_feels_like.to_string()),
            Self::Icon => s.push_str(w.icon),
            Self::NextTemp => s.push_str(&w.next_temp.to_string()),
            Self::NextLabel => s.push_str(w.next_label),
            Self::LaterTemp => s.push_str(&w.later_temp.to_string()),
            Self::LaterLabel => s.push_str(w.later_label),
            Self::MaxTemp => s.push_str(&w.max_temp.to_string()),
            Self::OvernightMin => s.push_str(&w.overnight_min.to_string()),
            Self::TomorrowMax => s.push_str(&w.tomorrow_max.to_string()),
            Self::RainSince9am => {
                // API usually returns 0 if there hasn't been rain
                // so take None to mean data unavailable
                if let Some(rain) = w.rain_since_9am {
                    s.push_str(&rain.to_string())
                } else {
                    s.push_str("??")
                }
            }
            Self::HourlyRainChance => s.push_str(&w.hourly_rain_chance.to_string()),
            Self::HourlyRainMin => s.push_str(&w.hourly_rain_min.to_string()),
            Self::HourlyRainMax => s.push_str(&w.hourly_rain_max.to_string()),
            Self::TodayRainChance => s.push_str(&w.today_rain_chance.to_string()),
            Self::TodayRainMin => s.push_str(&w.today_rain_min.to_string()),
            Self::TodayRainMax => s.push_str(&w.today_rain_max.to_string()),
            Self::ShortText => s.push_str(w.short_text.as_ref().unwrap_or(&String::new())),
            Self::ExtendedText => s.push_str(w.extended_text.as_ref().unwrap_or(&String::new())),
            Self::WindSpeed => s.push_str(&w.wind_speed.to_string()),
            Self::WindDirection => s.push_str(w.wind_direction),
            Self::WindGust => s.push_str(&w.gust.to_string()),
        }
    }
}
