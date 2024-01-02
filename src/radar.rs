use crate::config::Config;
use crate::ftp::FtpClient;
use crate::persistence::Database;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use clap::Parser;
use image::codecs::png::PngDecoder;
use image::io::Reader as ImageReader;
use image::{imageops, DynamicImage, ImageOutputFormat, Rgba, RgbaImage};
use mpvipc::*;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::str::FromStr;
use std::thread::sleep;
use strum::{Display, EnumCount};
use strum_macros::{EnumIter, EnumString};
use suppaftp::list::File;
use tracing::{debug, warn};

pub type RadarId = u32;

#[derive(Clone, Debug)]
pub struct Radar {
    pub id: RadarId,
    pub name: String,
    pub longitude: f32,
    pub latitude: f32,
    pub full_name: String,
    pub state: String,
    pub r#type: String,
    pub group: bool,
}

impl From<RadarData> for Radar {
    fn from(data: RadarData) -> Self {
        Self {
            id: data.id as RadarId,
            name: data.name,
            longitude: data.longitude,
            latitude: data.latitude,
            full_name: data.full_name,
            state: data.state,
            r#type: data.r#type,
            group: match data.group.as_str() {
                "Yes" => true,
                "No" => false,
                _ => false,
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RadarData {
    pub name: String,
    pub longitude: f32,
    pub latitude: f32,
    pub id: f64,
    pub full_name: String,
    pub idrnn0name: String,
    pub idrnn1name: String,
    pub state: String,
    pub r#type: String,
    pub group: String,
    pub status: String,
    pub archive: String,
    pub location_id: f64,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    EnumIter,
    Ord,
    PartialEq,
    PartialOrd,
    Eq,
    Deserialize,
    Serialize,
    clap::ValueEnum,
)]
pub enum RadarType {
    #[serde(rename = "64km")]
    #[clap(name = "64km")]
    SixtyFourKm,
    #[clap(name = "128km")]
    #[serde(rename = "128km")]
    OneTwentyEightKm,
    #[serde(rename = "256km")]
    #[clap(name = "256km")]
    TwoFiftySixKm,
    #[serde(rename = "512km")]
    #[clap(name = "512km")]
    FiveTwelveKm,
    #[serde(rename = "doppler")]
    #[clap(name = "doppler")]
    DopplerWind,
    #[serde(rename = "5min")]
    #[clap(name = "5min")]
    AccumulatedFiveMin,
    #[serde(rename = "1hour")]
    #[clap(name = "1hour")]
    AccumulatedOneHour,
    #[serde(rename = "since9")]
    #[clap(name = "since9")]
    AccumulatedSinceNine,
    #[serde(rename = "24hour")]
    #[clap(name = "24hour")]
    AccumulatedPreviousTwentyFour,
}

impl RadarType {
    pub fn from_id(id: char) -> Result<Self> {
        let radar_type = match id {
            '1' => Self::FiveTwelveKm,
            '2' => Self::TwoFiftySixKm,
            '3' => Self::OneTwentyEightKm,
            '4' => Self::SixtyFourKm,
            'I' => Self::DopplerWind,
            'A' => Self::AccumulatedFiveMin,
            'B' => Self::AccumulatedOneHour,
            'C' => Self::AccumulatedSinceNine,
            'D' => Self::AccumulatedPreviousTwentyFour,
            _ => return Err(anyhow!("{id} is not a valid radar type ID")),
        };

        Ok(radar_type)
    }

    pub fn id(&self) -> char {
        match self {
            Self::FiveTwelveKm => '1',
            Self::TwoFiftySixKm => '2',
            Self::OneTwentyEightKm => '3',
            Self::SixtyFourKm => '4',
            Self::DopplerWind => 'I',
            Self::AccumulatedFiveMin => 'A',
            Self::AccumulatedOneHour => 'B',
            Self::AccumulatedSinceNine => 'C',
            Self::AccumulatedPreviousTwentyFour => 'D',
        }
    }

    pub fn size(&self) -> Self {
        match self {
            Self::DopplerWind
            | Self::AccumulatedFiveMin
            | Self::AccumulatedOneHour
            | Self::AccumulatedSinceNine
            | Self::AccumulatedPreviousTwentyFour => Self::OneTwentyEightKm,
            _ => *self,
        }
    }

    pub fn update_frequency(self) -> Duration {
        match self {
            Self::AccumulatedSinceNine => Duration::minutes(15),
            Self::AccumulatedPreviousTwentyFour => Duration::days(1),
            _ => Duration::minutes(5),
        }
    }

    // A delay to accommodate lag between image timestamp and when it appears on FTP
    pub fn check_after(self) -> Duration {
        match self {
            Self::AccumulatedSinceNine => Duration::minutes(15),
            Self::AccumulatedPreviousTwentyFour => Duration::minutes(10),
            _ => Duration::minutes(2),
        }
    }

    pub fn min_image_count(self) -> i32 {
        match self {
            Self::AccumulatedPreviousTwentyFour => 10,
            Self::AccumulatedSinceNine => 30,
            _ => 18,
        }
    }

    pub fn legend_type(&self) -> RadarLegendType {
        match self {
            Self::SixtyFourKm
            | Self::OneTwentyEightKm
            | Self::TwoFiftySixKm
            | Self::FiveTwelveKm => RadarLegendType::Rainfall,
            Self::DopplerWind => RadarLegendType::DopplerWind,
            _ => RadarLegendType::AccumulatedRainfall,
        }
    }
}

#[derive(Debug, Clone, Display, EnumIter)]
pub enum RadarLegendType {
    Rainfall,
    AccumulatedRainfall,
    DopplerWind,
}

impl RadarLegendType {
    pub fn id(&self) -> u8 {
        match self {
            Self::Rainfall => 0,
            Self::AccumulatedRainfall => 1,
            Self::DopplerWind => 2,
        }
    }
}

/// A static legend that serves as the base layer for a radar image.
#[derive(Debug, Clone)]
pub struct RadarImageLegend {
    pub r#type: RadarLegendType,
    pub png_buf: Vec<u8>,
}

/// A static layer that overlays geographical information onto a radar image
#[derive(Debug)]
pub struct RadarImageFeatureLayer {
    pub feature: RadarImageFeature,
    pub size: RadarType,
    pub radar_id: RadarId,
    pub png_buf: Vec<u8>,
    pub filename: String,
}

#[derive(
    Clone,
    Display,
    EnumString,
    EnumIter,
    EnumCount,
    Debug,
    PartialEq,
    Deserialize,
    Serialize,
    clap::ValueEnum,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum RadarImageFeature {
    // Ordered by how the layers stack on the BOM website's radar viewer
    Background,
    Topography,
    Range,
    Waterways,
    Roads,
    #[strum(serialize = "wthrDistricts")]
    #[serde(rename = "wthrDistricts")]
    #[clap(name = "wthrDistricts")]
    ForecastDistricts,
    Rail,
    Catchments,
    Locations,
}

impl RadarImageFeatureLayer {
    pub fn from_filename(name: &str) -> Result<Self> {
        // e.g. IDR023.catchments.png
        let (first, last) = get_dot_indices(name)?;
        let radar_type_id: char = name[first - 1..first].parse()?;
        let feature = &name[first + 1..last];

        Ok(Self {
            feature: RadarImageFeature::from_str(feature)?,
            size: RadarType::from_id(radar_type_id)?,
            radar_id: name[3..first - 1].parse()?,
            png_buf: Vec::new(),
            filename: name.to_string(),
        })
    }
}

/// A dynamic layer that changes each frame i.e. the actual data from the radar
#[derive(Debug, Clone)]
pub struct RadarImageDataLayer {
    pub radar_type: RadarType,
    pub png_buf: Vec<u8>,
    pub radar_id: RadarId,
    pub datetime: DateTime<Utc>,
    pub filename: String,
}

impl PartialEq for RadarImageDataLayer {
    fn eq(&self, other: &Self) -> bool {
        self.filename == other.filename
    }
}

impl RadarImageDataLayer {
    pub fn from_filename(name: &str) -> Result<Self> {
        // e.g. IDR023.T.202311130334.png
        let (first, last) = get_dot_indices(name)?;
        let radar_type_id: char = name[first - 1..first].parse()?;
        let timestamp = &name[first + 3..last];
        let utc = NaiveDateTime::parse_from_str(timestamp, "%Y%m%d%H%M")?;

        Ok(Self {
            radar_type: RadarType::from_id(radar_type_id)?,
            png_buf: Vec::new(),
            radar_id: name[3..first - 1].parse()?,
            datetime: DateTime::from_naive_utc_and_offset(utc, Utc),
            filename: name.to_string(),
        })
    }

    pub fn next_datetime(&self) -> DateTime<Utc> {
        self.datetime + self.radar_type.update_frequency()
    }

    pub fn expected_next(&self) -> Self {
        Self {
            radar_id: self.radar_id,
            radar_type: self.radar_type,
            png_buf: Vec::new(),
            datetime: self.next_datetime(),
            filename: self.next_filename(),
        }
    }

    pub fn next_filename(&self) -> String {
        format!(
            "IDR{:02}{1}.T.{2}.png",
            self.radar_id,
            self.radar_type.id(),
            self.next_datetime().format("%Y%m%d%H%M")
        )
    }
}

fn get_dot_indices(name: &str) -> Result<(usize, usize)> {
    let err = Err(anyhow!("{name} is not a valid radar image file"));
    let Some(first_dot) = name.find('.') else {
        return err;
    };
    let Some(last_dot) = name.rfind('.') else {
        return err;
    };
    if first_dot == last_dot {
        return err;
    };
    Ok((first_dot, last_dot))
}

#[derive(Clone, Parser, Debug, Deserialize, Serialize)]
pub struct RadarImageOptions {
    pub features: Vec<RadarImageFeature>,
    pub max_frames: Option<u64>,
    pub radar_types: Vec<RadarType>,
    pub remove_header: bool,
    pub create_png: bool,
    pub create_apng: bool,
    pub frame_delay_ms: u16,
    pub image_dir: PathBuf,
    pub force: bool,
    pub open_mpv: bool,
    pub mpv_args: Vec<String>,
}

impl Default for RadarImageOptions {
    fn default() -> Self {
        Self {
            features: vec![
                RadarImageFeature::Background,
                RadarImageFeature::Topography,
                RadarImageFeature::Range,
                RadarImageFeature::Locations,
            ],
            frame_delay_ms: 200,
            max_frames: Some(24),
            radar_types: vec![RadarType::OneTwentyEightKm],
            remove_header: false,
            create_png: true,
            create_apng: false,
            force: false,
            open_mpv: false,
            mpv_args: vec![
                "--stop-screensaver=no".into(),
                "--geometry=1024x1114".into(),
                "--auto-window-resize=no".into(),
                "--loop-playlist".into(),
            ],
            image_dir: [
                Config::default_dirs().state.as_path(),
                &Path::new("radar-images"),
            ]
            .iter()
            .collect(),
        }
    }
}

#[derive(Debug)]
pub struct RadarImageComponents {
    pub legend: RadarImageLegend,
    pub feature_layers: Vec<RadarImageFeatureLayer>,
    pub data_layers: Vec<RadarImageDataLayer>,
}

#[derive(Debug, Clone)]
pub struct RadarImageFrame {
    pub radar_type: RadarType,
    pub image: DynamicImage,
    pub radar_id: RadarId,
    pub datetime: DateTime<Utc>,
    pub path: PathBuf,
}

impl RadarImageFrame {
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let name = path.file_name().unwrap().to_string_lossy();
        let (first, last) = get_dot_indices(&name)?;
        let radar_type_id: char = name[first - 1..first].parse()?;
        let timestamp = &name[first + 3..last];
        let utc = NaiveDateTime::parse_from_str(timestamp, "%Y%m%d%H%M")?;
        let image = ImageReader::open(&path)?.decode()?;

        Ok(Self {
            radar_type: RadarType::from_id(radar_type_id)?,
            radar_id: name[3..first - 1].parse()?,
            datetime: DateTime::from_naive_utc_and_offset(utc, Utc),
            path,
            image,
        })
    }
}

impl PartialEq for RadarImageFrame {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

fn decode_png(png_buf: &[u8]) -> Result<DynamicImage> {
    let decoder = PngDecoder::new(png_buf)?;
    let img = DynamicImage::from_decoder(decoder)?;
    Ok(img)
}

pub fn get_radar_image_managers<'a>(
    id: RadarId,
    db: &'a mut Database,
    ftp: &'a mut FtpClient,
    opts: &'a RadarImageOptions,
) -> Result<Vec<RadarImageManager>> {
    let mut managers = Vec::new();

    for radar_type in &opts.radar_types {
        let feature_layers = if let Ok(layers) = db.get_radar_feature_layers(id, radar_type) {
            layers
        } else {
            let layers = ftp.get_radar_feature_layers(id, *radar_type)?;
            db.insert_radar_feature_layers(&layers)?;
            layers
        };

        let data_layers =
            if let Ok(layers) = db.get_radar_data_layers(id, radar_type, opts.max_frames) {
                layers
            } else {
                Vec::new()
            };

        let legend = db.get_radar_legend(radar_type)?;
        let manager = RadarImageManager::new(
            id,
            *radar_type,
            legend,
            data_layers,
            feature_layers,
            opts.clone(),
        );
        managers.push(manager);
    }
    Ok(managers)
}

pub fn fetch_new_data_layers(
    id: RadarId,
    db: &mut Database,
    radar_type: &RadarType,
    ftp: &mut FtpClient,
    ftp_files: &mut Vec<File>,
    opts: &RadarImageOptions,
) -> Result<Vec<RadarImageDataLayer>> {
    let now = Utc::now();

    let mut new_layers = Vec::new();
    let mut check_ftp_files = false;

    let existing_names = db.get_radar_data_layer_names(radar_type)?;
    if let Some(last_name) = existing_names.last() {
        let mut last_layer = &RadarImageDataLayer::from_filename(last_name).unwrap();
        loop {
            let time_since_last = now - last_layer.datetime;
            if time_since_last > radar_type.update_frequency() * radar_type.min_image_count() {
                check_ftp_files = true;
                break;
            }

            let mut next_layer = last_layer.expected_next();
            if now < next_layer.datetime + next_layer.radar_type.check_after() {
                ftp.keepalive()?;
                break;
            }

            // Try to download images by their expected filename to avoid listing the whole
            // directory. If an image is missing more than a minute later than expected,
            // re-check the FTP in case the duration between files has changed
            match ftp.get_radar_data_png(&next_layer.filename) {
                Ok(png_buf) => {
                    next_layer.png_buf = png_buf;
                    new_layers.push(next_layer);
                    last_layer = new_layers.last().unwrap();
                }
                Err(e) => {
                    debug!(
                        "Failed to download radar data layer {}. {e}",
                        next_layer.filename
                    );
                    if now
                        > next_layer.datetime
                            + next_layer.radar_type.check_after()
                            + Duration::minutes(1)
                    {
                        check_ftp_files = true;
                    }
                    break;
                }
            }
        }
    } else {
        check_ftp_files = true;
    }

    if check_ftp_files {
        if ftp_files.is_empty() {
            for file in ftp.list_radar_data_layers()? {
                ftp_files.push(file);
            }
        }
        let last_layer = existing_names
            .last()
            .map(|n| RadarImageDataLayer::from_filename(n).unwrap());
        let mut todo = Vec::new();
        let prefix = format!("IDR{id:02}{}", radar_type.id());
        let mut count = 0;
        for file in ftp_files.iter().filter(|f| f.name().starts_with(&prefix)) {
            count += 1;
            let layer = RadarImageDataLayer::from_filename(file.name())?;
            if existing_names.contains(&layer.filename) || new_layers.contains(&layer) {
                continue;
            }
            if let Some(ref last) = last_layer {
                if layer.datetime < last.datetime {
                    continue;
                }
            }
            todo.push(layer);
        }
        if let Some(max_frames) = opts.max_frames {
            if todo.len() > max_frames as usize {
                let idx = todo.len() - max_frames as usize;
                todo.drain(..idx);
            }
        }
        for layer in todo.iter_mut() {
            layer.png_buf = ftp.get_radar_data_png(&layer.filename)?;
        }
        new_layers.append(&mut todo);
        if count == 0 {
            warn!(
                "No images for {radar_type} found on FTP. \
                Some radars have limited data available. Consider adjusting your config"
            );
        }
    }
    Ok(new_layers)
}

pub fn update_radar_images(
    managers: &mut Vec<RadarImageManager>,
    db: &mut Database,
    ftp: &mut FtpClient,
) -> Result<DateTime<Utc>> {
    // Cache the FTP file list so we don't re-download it for each radar type
    let mut ftp_files = Vec::new();
    let mut next_datetimes = Vec::new();
    for m in managers {
        let new_data_layers =
            fetch_new_data_layers(m.radar_id, db, &m.radar_type, ftp, &mut ftp_files, &m.opts)?;
        if !new_data_layers.is_empty() {
            db.insert_radar_data_layers(&new_data_layers)?;
            m.add_data_layers(new_data_layers);
        }
        if let Some(last) = m.data_layers.last() {
            next_datetimes.push(last.next_datetime() + last.radar_type.check_after());
        }
    }
    let next_layer_due = next_datetimes.into_iter().min().unwrap();
    let next_check = if next_layer_due > Utc::now() {
        next_layer_due
    } else {
        Utc::now() + Duration::seconds(60)
    };
    Ok(next_check)
}

pub struct RadarImageManager {
    image_dir: PathBuf,
    radar_type: RadarType,
    radar_id: RadarId,
    legend: RadarImageLegend,
    feature_layers: Vec<RadarImageFeatureLayer>,
    data_layers: Vec<RadarImageDataLayer>,
    frames: Vec<RadarImageFrame>,
    pub opts: RadarImageOptions,
    mpv: Option<MpvRadarViewer>,
}

impl Display for RadarImageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IDR{:02}{}", self.radar_id, self.radar_type.id())
    }
}

impl RadarImageManager {
    pub fn new(
        radar_id: RadarId,
        radar_type: RadarType,
        legend: RadarImageLegend,
        data_layers: Vec<RadarImageDataLayer>,
        feature_layers: Vec<RadarImageFeatureLayer>,
        opts: RadarImageOptions,
    ) -> Self {
        let mut image_dir = opts.image_dir.clone();
        image_dir.push(format!("IDR{:02}{}", radar_id, radar_type.id()));

        let mpv = if opts.open_mpv {
            let socket_name = format!("IDR{:02}{}.sock", radar_id, radar_type.id());
            Some(MpvRadarViewer {
                radar_id,
                radar_type,
                socket_path: Config::default_dirs().run.join(socket_name),
                handle: None,
                frame_delay: opts.frame_delay_ms as f32 / 1000.0,
            })
        } else {
            None
        };

        let mut frames = Vec::new();
        if let Ok(entries) = fs::read_dir(&image_dir) {
            for entry in entries {
                if let Ok(frame) = RadarImageFrame::from_path(entry.unwrap().path()) {
                    frames.push(frame);
                }
            }
        }

        Self {
            mpv,
            radar_id,
            radar_type,
            legend,
            data_layers,
            feature_layers,
            frames,
            image_dir,
            opts,
        }
    }

    fn construct_frames(&mut self) -> Result<()> {
        let mut bottom_layer = decode_png(&self.legend.png_buf)?;
        let mut top_layer = DynamicImage::ImageRgba8(RgbaImage::new(512, 512));

        if self.opts.force {
            self.frames.clear();
        }

        let mut todo = Vec::new();
        for layer in &self.data_layers {
            if !self.frames.iter().any(|f| f.datetime == layer.datetime) {
                todo.push(layer);
            }
        }

        if todo.is_empty() {
            return Ok(());
        }

        for feature in &self.opts.features {
            match feature {
                RadarImageFeature::Background | RadarImageFeature::Topography => {
                    self.overlay_feature(&mut bottom_layer, feature)?
                }
                _ => self.overlay_feature(&mut top_layer, feature)?,
            }
        }

        for layer in todo {
            debug!("Constructing frame for {}", layer.filename);
            let mut data_layer = decode_png(&layer.png_buf)?;
            if self.opts.remove_header {
                self.remove_header(&mut data_layer);
            }
            let mut final_image = bottom_layer.clone();
            imageops::overlay(&mut final_image, &data_layer, 0, 0);
            imageops::overlay(&mut final_image, &top_layer, 0, 0);
            let frame = RadarImageFrame {
                radar_id: layer.radar_id,
                radar_type: layer.radar_type,
                path: self.image_dir.join(&layer.filename),
                datetime: layer.datetime,
                image: final_image,
            };
            self.frames.push(frame);
        }
        Ok(())
    }

    fn remove_header(&self, image: &mut DynamicImage) {
        let DynamicImage::ImageRgba8(ref mut rgba_image) = image else {
            return;
        };
        for y in 0..16 {
            for x in 0..512 {
                rgba_image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }

    fn overlay_feature(&self, base: &mut DynamicImage, feature: &RadarImageFeature) -> Result<()> {
        let Some(layer) = self.feature_layers.iter().find(|l| l.feature == *feature) else {
            warn!("Missing {feature} feature");
            return Ok(());
        };
        imageops::overlay(base, &decode_png(&layer.png_buf)?, 0, 0);
        Ok(())
    }

    fn sort_frames(&mut self) {
        self.frames
            .sort_by(|a, b| a.datetime.partial_cmp(&b.datetime).unwrap());
    }

    fn remove_images(&mut self, idx: usize) -> Result<Vec<RadarImageDataLayer>> {
        let removed = self.data_layers.drain(..idx).collect();
        for frame in self.frames.drain(..idx) {
            if frame.path.exists() {
                debug!("Deleting old radar image {}", frame.path.display());
                fs::remove_file(&frame.path)?;
            }
        }
        Ok(removed)
    }

    pub fn prune(&mut self) -> Result<Vec<RadarImageDataLayer>> {
        let mut removed = Vec::new();
        self.sort_frames();
        if let Some(max_frames) = self.opts.max_frames {
            if self.frames.len() > max_frames as usize {
                let idx = self.frames.len() - max_frames as usize;
                debug!("{self} frame limit of {max_frames} exceeded by {idx}");
                removed.extend(self.remove_images(idx)?);
            }
        }

        let mut iter = self.frames.iter().enumerate().peekable();
        while let Some((idx, frame)) = iter.next() {
            if let Some((_, next_frame)) = iter.peek() {
                let gap = next_frame.datetime - frame.datetime;
                let max_gap = frame.radar_type.update_frequency() * 4;
                if next_frame.datetime - frame.datetime > max_gap {
                    debug!(
                        "{self} has gap of {} minutes between images. Removing old images",
                        gap.num_minutes()
                    );
                    removed.extend(self.remove_images(idx + 1)?);
                    break;
                }
            }
        }
        Ok(removed)
    }
    pub fn write_pngs(&mut self) -> Result<()> {
        self.construct_frames()?;
        fs::create_dir_all(&self.image_dir)?;
        for frame in &self.frames {
            if frame.path.exists() && !self.opts.force {
                continue;
            }
            let file = fs::File::create(&frame.path)?;
            let mut writer = BufWriter::new(file);
            frame.image.write_to(&mut writer, ImageOutputFormat::Png)?;
        }
        Ok(())
    }

    pub fn add_data_layers(&mut self, mut layers: Vec<RadarImageDataLayer>) {
        self.data_layers.append(&mut layers);
    }

    pub fn open_images(&mut self) -> Result<()> {
        self.write_pngs()?;
        self.sort_frames();
        let paths: Vec<&Path> = self.frames.iter().map(|f| f.path.as_path()).collect();
        let mpv = self.mpv.as_mut().unwrap();
        mpv.open_images(&paths, &self.opts.mpv_args)?;
        Ok(())
    }

    pub fn create_apng(&mut self) -> Result<()> {
        self.construct_frames()?;
        let start = self.frames.first().unwrap().datetime.format("%Y%m%d%H%M");
        let end = self.frames.last().unwrap().datetime.format("%Y%m%d%H%M");
        let filename = format!("{}.T.{}-{}.png", self, start, end);
        let path = self.image_dir.join(&filename);
        let out_file = fs::File::create(path)?;
        let mut writer = BufWriter::new(out_file);
        let mut pngs = Vec::new();
        for frame in &self.frames {
            let png = apng::load_dynamic_image(frame.image.clone())?;
            pngs.push(png);
        }

        let config = apng::create_config(&pngs, None).unwrap();
        let mut encoder = apng::Encoder::new(&mut writer, config).unwrap();
        let apng_frame = apng::Frame {
            delay_num: Some(self.opts.frame_delay_ms),
            delay_den: Some(1000),
            ..Default::default()
        };
        encoder.encode_all(pngs, Some(&apng_frame)).unwrap();

        Ok(())
    }
}

struct MpvRadarViewer {
    handle: Option<Child>,
    socket_path: PathBuf,
    radar_id: RadarId,
    radar_type: RadarType,
    frame_delay: f32,
}

impl MpvRadarViewer {
    pub fn open_images(&mut self, paths: &[&Path], args: &[String]) -> Result<()> {
        let mut mpv_is_running = false;
        if let Some(ref mut handle) = self.handle {
            match handle.try_wait() {
                Ok(Some(status)) => {
                    warn!("MPV {} exited with: {}", self.socket_path.display(), status);
                }
                Ok(None) => mpv_is_running = true,
                Err(e) => warn!("Error waiting for MPV {} {e}", self.socket_path.display()),
            }
        }
        if !mpv_is_running {
            self.start(paths, args)?;
        }
        let mpv = self.connect()?;
        mpv.playlist_clear()?;
        let command = MpvCommand::LoadFile {
            file: paths[0].to_string_lossy().to_string(),
            option: PlaylistAddOptions::Replace,
        };
        mpv.run_command(command)?;
        for path in &paths[1..] {
            let command = MpvCommand::LoadFile {
                file: path.to_string_lossy().to_string(),
                option: PlaylistAddOptions::Append,
            };
            mpv.run_command(command).unwrap();
        }
        Ok(())
    }

    fn connect(&self) -> Result<Mpv> {
        let mut attempts = 0;
        let mpv = loop {
            match Mpv::connect(&self.socket_path.to_string_lossy()) {
                Ok(mpv) => break mpv,
                _ => {
                    sleep(Duration::milliseconds(20).to_std().unwrap());
                    attempts += 1;
                    if attempts > 10 {
                        return Err(anyhow!(
                            "Failed to connect to MPV at {}",
                            self.socket_path.display()
                        ));
                    }
                }
            }
        };
        Ok(mpv)
    }

    fn start(&mut self, image_paths: &[&Path], args: &[String]) -> Result<()> {
        let mut ipc_arg = OsString::from("--input-ipc-server=");
        ipc_arg.push(&self.socket_path);
        let app_id = format!("mpv-radar-IDR{}{}", self.radar_id, self.radar_type.id());
        fs::create_dir_all(self.socket_path.parent().unwrap())?;
        let child = std::process::Command::new("mpv")
            .arg(ipc_arg)
            .arg(format!("--wayland-app-id={app_id}"))
            .arg(format!("--x11-name={app_id}"))
            .arg(format!("--image-display-duration={}", self.frame_delay))
            .arg("--loop-file=no")
            .args(args)
            .args(image_paths)
            .spawn()
            .unwrap();

        self.handle = Some(child);
        Ok(())
    }
}
