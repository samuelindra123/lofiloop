//! Membaca properti media dengan ffprobe.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ffmpeg::Ffmpeg;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    pub path: String,
    pub duration_s: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_count: Option<u64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

impl MediaInfo {
    /// Audio yang benar-benar terdengar? Video bisu atau audio yang cuma noise lantai
    /// diperlakukan sebagai tanpa suara, sehingga audio user dipakai tanpa ragu.
    pub fn frame_duration_s(&self) -> f64 {
        if self.fps > 0.0 {
            1.0 / self.fps
        } else {
            0.04
        }
    }
}

/// Baca properti satu berkas media.
pub fn probe(ff: &Ffmpeg, path: &str) -> Result<MediaInfo> {
    let args: Vec<String> = vec![
        "-v".into(),
        "error".into(),
        "-print_format".into(),
        "json".into(),
        "-show_format".into(),
        "-show_streams".into(),
        path.into(),
    ];
    let json = ff.probe_json(&args)?;

    let streams = json
        .get("streams")
        .and_then(|s| s.as_array())
        .ok_or_else(|| Error::Probe(format!("tidak ada stream di {path}")))?;

    let mut info = MediaInfo {
        path: path.to_string(),
        duration_s: json
            .get("format")
            .and_then(|f| f.get("duration"))
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<f64>().ok())
            .unwrap_or(0.0),
        has_video: false,
        has_audio: false,
        width: 0,
        height: 0,
        fps: 0.0,
        frame_count: None,
        video_codec: None,
        audio_codec: None,
        sample_rate: None,
        channels: None,
    };

    for s in streams {
        let kind = s.get("codec_type").and_then(|c| c.as_str()).unwrap_or("");
        match kind {
            "video" => {
                info.has_video = true;
                info.width = s.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                info.height = s.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                info.video_codec = s
                    .get("codec_name")
                    .and_then(|c| c.as_str())
                    .map(str::to_string);
                info.fps = parse_fraction(
                    s.get("avg_frame_rate")
                        .or_else(|| s.get("r_frame_rate"))
                        .and_then(|r| r.as_str())
                        .unwrap_or("0/0"),
                );
                info.frame_count = s
                    .get("nb_frames")
                    .and_then(|n| n.as_str())
                    .and_then(|n| n.parse::<u64>().ok());
            }
            "audio" => {
                info.has_audio = true;
                info.audio_codec = s
                    .get("codec_name")
                    .and_then(|c| c.as_str())
                    .map(str::to_string);
                info.sample_rate = s
                    .get("sample_rate")
                    .and_then(|n| n.as_str())
                    .and_then(|n| n.parse::<u32>().ok());
                info.channels = s.get("channels").and_then(|n| n.as_u64()).map(|n| n as u32);
            }
            _ => {}
        }
    }

    if !info.has_video && !info.has_audio {
        return Err(Error::Probe(format!(
            "{path} tidak punya stream audio maupun video"
        )));
    }
    if info.duration_s <= 0.0 {
        return Err(Error::Probe(format!("durasi {path} tidak terbaca")));
    }
    Ok(info)
}

/// Puncak volume audio dalam dBFS, dipakai untuk memutuskan audio video "terdengar" atau tidak.
pub fn audio_peak_dbfs(ff: &Ffmpeg, path: &str) -> Result<f64> {
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostats".into(),
        "-i".into(),
        path.into(),
        "-vn".into(),
        "-af".into(),
        "volumedetect".into(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ];
    let stderr = ff.run(&args, "volumedetect")?;
    for line in stderr.lines() {
        if let Some(rest) = line.split("max_volume:").nth(1) {
            if let Some(num) = rest.split_whitespace().next() {
                if let Ok(v) = num.parse::<f64>() {
                    return Ok(v);
                }
            }
        }
    }
    Ok(f64::NEG_INFINITY)
}

fn parse_fraction(text: &str) -> f64 {
    let mut parts = text.split('/');
    let num = parts
        .next()
        .and_then(|n| n.parse::<f64>().ok())
        .unwrap_or(0.0);
    let den = parts
        .next()
        .and_then(|n| n.parse::<f64>().ok())
        .unwrap_or(1.0);
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}
