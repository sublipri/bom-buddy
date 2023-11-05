use crate::daily::DailyForecast;
use crate::hourly::HourlyForecast;
use crate::observation::Observation;
use crate::warning::Warning;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Serialize, Deserialize)]
pub struct Weather {
    pub past_observations: VecDeque<Observation>,
    pub observation: Observation,
    pub daily_forecast: DailyForecast,
    pub hourly_forecast: HourlyForecast,
    pub warnings: Vec<Warning>,
}
