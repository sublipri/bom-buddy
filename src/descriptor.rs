use serde::{Deserialize, Serialize};

// https://reg.bom.gov.au/info/forecast_icons.shtml
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    pub fn get_icon_emoji(&self, is_night: bool) -> &str {
        match self {
            Self::Sunny => {
                if is_night {
                    "🌙"
                } else {
                    "☀️"
                }
            }
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
}
