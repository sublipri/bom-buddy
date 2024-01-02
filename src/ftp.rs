use crate::radar::{
    Radar, RadarData, RadarId, RadarImageFeature, RadarImageFeatureLayer, RadarImageLegend,
    RadarLegendType, RadarType,
};
use anyhow::{anyhow, Result};
use chrono::Duration;
use std::str::FromStr;
use std::{io::Cursor, thread::sleep};
use strum::IntoEnumIterator;
use suppaftp::list::File;
use suppaftp::FtpStream;
use tracing::{debug, error};

pub struct FtpClient {
    // Store FtpStream as an Option to avoid login delay when FtpClient is constructed
    ftp_stream: Option<FtpStream>,
    root_url: String,
}

impl FtpClient {
    pub fn new() -> Result<Self> {
        let root_url = "ftp.bom.gov.au:21".to_string();
        Ok(FtpClient {
            ftp_stream: None,
            root_url,
        })
    }

    fn stream(&mut self) -> Result<&mut FtpStream> {
        if self.ftp_stream.is_none() {
            let mut stream = FtpStream::connect(&self.root_url)?;
            stream.login("anonymous", "guest")?;
            self.ftp_stream = Some(stream);
        }
        Ok(self.ftp_stream.as_mut().unwrap())
    }

    fn get_buf(&mut self, path: &str) -> Result<Cursor<Vec<u8>>> {
        debug!("Downloading {}{path}", self.root_url);
        Ok(self.stream()?.retr_as_buffer(path)?)
    }

    pub fn keepalive(&mut self) -> Result<()> {
        if self.ftp_stream.is_some() {
            self.stream()?.noop()?;
        }
        Ok(())
    }

    pub fn list_files(&mut self, path: &str) -> Result<impl Iterator<Item = File>> {
        let url = format!("{}{}", self.root_url, path);
        debug!("Listing directory {url}");
        let mut attempts = 0;
        let listing = loop {
            match self.stream()?.list(Some(path)) {
                Ok(l) => break l,
                Err(e) => {
                    error!("Error listing directory {url}. {e} Retry in 5 seconds");
                    attempts += 1;
                    if attempts > 5 {
                        return Err(anyhow!(
                            "Failed to list directory {url} after {attempts} attempts. {e}"
                        ));
                    }
                    sleep(Duration::seconds(5).to_std().unwrap());
                    continue;
                }
            };
        };
        Ok(listing.into_iter().map(|s| File::from_str(&s).unwrap()))
    }

    pub fn get_radar_data(&mut self) -> Result<Vec<RadarData>> {
        let buf = self.get_buf("/anon/home/adfd/spatial/IDR00007.dbf")?;
        let mut reader = dbase::Reader::new(buf)?;
        Ok(reader.read_as::<RadarData>()?)
    }

    pub fn get_public_radars(&mut self) -> Result<impl Iterator<Item = Radar>> {
        Ok(self
            .get_radar_data()?
            .into_iter()
            .filter(|r| r.status == "Public")
            .map(|r| r.into()))
    }

    pub fn get_radar_legends(&mut self) -> Result<Vec<RadarImageLegend>> {
        let mut legends = Vec::with_capacity(3);
        for t in RadarLegendType::iter() {
            let path = format!("/anon/gen/radar_transparencies/IDR.legend.{}.png", t.id());
            legends.push(RadarImageLegend {
                r#type: t,
                png_buf: self.get_buf(&path)?.into_inner(),
            })
        }
        Ok(legends)
    }

    pub fn get_radar_feature_layers(
        &mut self,
        id: RadarId,
        size: RadarType,
    ) -> Result<Vec<RadarImageFeatureLayer>> {
        let mut layers = Vec::new();
        for feature in RadarImageFeature::iter() {
            let layer = self.get_radar_feature_layer(id, size, feature)?;
            layers.push(layer);
        }
        Ok(layers)
    }

    pub fn get_radar_feature_layer(
        &mut self,
        id: RadarId,
        size: RadarType,
        feature: RadarImageFeature,
    ) -> Result<RadarImageFeatureLayer> {
        let filename = format!("IDR{id:02}{}.{feature}.png", size.id());
        let path = format!("/anon/gen/radar_transparencies/{filename}");
        let png_buf = self.get_buf(&path)?.into_inner();
        Ok(RadarImageFeatureLayer {
            feature,
            size,
            radar_id: id,
            png_buf,
            filename,
        })
    }

    pub fn list_radar_data_layers(&mut self) -> Result<impl Iterator<Item = File>> {
        Ok(self
            .list_files("/anon/gen/radar")?
            .filter(move |f| f.name().ends_with(".png")))
    }

    pub fn get_radar_data_png(&mut self, filename: &str) -> Result<Vec<u8>> {
        let buf = self.get_buf(&format!("/anon/gen/radar/{}", filename))?;
        Ok(buf.into_inner())
    }
}
