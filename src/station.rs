use serde::{Deserialize, Serialize};
use std::{mem::take, str::Lines};

#[derive(Debug, Serialize, Deserialize)]
pub struct WeatherStation {
    pub id: u32,
    pub district_id: String,
    pub name: String,
    pub start: u16,
    pub end: Option<u16>,
    pub latitude: f64,
    pub longitude: f64,
    pub source: Option<String>,
    pub state: String,
    pub height: Option<f64>,
    pub barometric_height: Option<f64>,
    pub wmo_id: Option<u32>,
}

/// A textual table containing all past and present weather stations
/// https://reg.bom.gov.au/climate/data/lists_by_element/stations.txt
pub struct StationsTable<'a> {
    lines: Lines<'a>,
    widths: [usize; 12],
}

impl<'a> StationsTable<'a> {
    pub fn new(station_list: &'a str) -> Self {
        let mut lines = station_list.lines();
        lines.nth(4);
        let widths = [8, 6, 41, 8, 8, 9, 10, 15, 4, 11, 9, 6];
        Self { lines, widths }
    }
}

impl<'a> Iterator for StationsTable<'a> {
    type Item = WeatherStation;

    fn next(&mut self) -> Option<Self::Item> {
        let line = self.lines.next()?;

        if line.is_empty() {
            return None;
        }

        let mut start = 0;
        let mut columns = Vec::new();
        for &width in self.widths.iter() {
            let end = start + width;
            let column = &line[start..end].trim();
            columns.push(column.to_string());
            start = end;
        }

        let source = take(&mut columns[7]);
        let source = if source.starts_with('.') {
            None
        } else {
            Some(source)
        };

        Some(WeatherStation {
            id: take(&mut columns[0]).parse().unwrap(),
            district_id: take(&mut columns[1]),
            name: take(&mut columns[2]).to_string(),
            start: take(&mut columns[3]).parse().unwrap(),
            end: take(&mut columns[4]).parse().ok(),
            latitude: take(&mut columns[5]).parse().unwrap(),
            longitude: take(&mut columns[6]).parse().unwrap(),
            source,
            state: take(&mut columns[8]).to_string(),
            height: take(&mut columns[9]).parse().ok(),
            barometric_height: take(&mut columns[10]).parse().ok(),
            wmo_id: take(&mut columns[11]).parse().ok(),
        })
    }
}
