use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Warning {
    pub area_id: String,
    pub expiry_time: String,
    pub id: String,
    pub issue_time: String,
    pub phase: String,
    pub short_title: String,
    pub state: String,
    pub title: String,
    pub r#type: String,
    pub warning_group_type: String,
}

#[derive(Serialize, Deserialize)]
pub struct WarningMetadata {
    pub copyright: String,
    pub response_timestamp: String,
}

#[derive(Serialize, Deserialize)]
pub struct WarningResponse {
    pub data: Vec<Warning>,
    pub metadata: WarningMetadata,
}
