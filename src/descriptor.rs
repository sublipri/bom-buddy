use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;

// https://reg.bom.gov.au/info/forecast_icons.shtml
#[derive(Debug, Serialize, Deserialize, AsRefStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "title_case")]
pub enum IconDescriptor {
    Sunny,
    Clear,
    MostlySunny,
    PartlyCloudy,
    Cloudy,
    Hazy,
    LightRain,
    Windy,
    Fog,
    Shower,
    Rain,
    Dusty,
    Frost,
    Snow,
    Storm,
    LightShower,
    HeavyShower,
    Cyclone,
}

impl IconDescriptor {
    // TODO: Add option to use nerd font weather icons. They don't look as nice but have proper
    // icons for all descriptors at night time unlike emojis
    // https://erikflowers.github.io/weather-icons/
    pub fn get_icon_emoji(&self, is_night: bool) -> &str {
        match self {
            Self::Sunny if is_night => "🌙",
            Self::Sunny => "☀️",
            Self::Clear => "🌙",
            Self::MostlySunny => "🌤️",
            Self::PartlyCloudy => "⛅",
            Self::Cloudy => "☁️",
            Self::Hazy => "🌅",
            Self::Windy => "🌬️",
            Self::Fog => "🌫️",
            Self::Shower => "🌦️",
            Self::LightShower => "🌦️",
            Self::LightRain => "🌦️",
            Self::HeavyShower => "🌧️",
            Self::Rain => "🌧️",
            Self::Dusty => "🐪",
            Self::Frost => "❄️",
            Self::Snow => "🌨️",
            Self::Storm => "⛈️",
            Self::Cyclone => "🌀",
        }
    }

    pub fn get_description(&self, is_night: bool) -> &str {
        match self {
            Self::Sunny if is_night => "Clear",
            Self::MostlySunny if is_night => "Mostly Clear",
            _ => self.as_ref(),
        }
    }
}
