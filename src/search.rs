use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Serialize, Deserialize)]
pub struct SearchResult {
    pub geohash: String,
    pub id: String,
    pub name: String,
    pub postcode: String,
    pub state: String,
}

impl fmt::Display for SearchResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} {}", self.name, self.state, self.postcode)
    }
}

#[derive(Serialize, Deserialize)]
pub struct SearchMetadata {
    pub copyright: String,
    pub response_timestamp: String,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResponse {
    pub data: Vec<SearchResult>,
    pub metadata: SearchMetadata,
}
