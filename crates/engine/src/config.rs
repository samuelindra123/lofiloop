//! Konfigurasi aplikasi yang tersimpan di disk, plus penyelesaian lokasi ffmpeg.
//!
//! Urutan mencari ffmpeg: konfigurasi pengguna (termasuk hasil unduhan otomatis) ->
//! variabel LOFILOOP_FFMPEG_DIR -> folder `assets/ffmpeg` sebelah exe -> PATH sistem.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Folder berisi ffmpeg.exe dan ffprobe.exe. Kosong = cari otomatis.
    pub ffmpeg_dir: Option<String>,
    pub duration_minutes: u32,
    pub video_loop_s: f64,
    pub video_xfade_s: f64,
    pub audio_xfade_s: f64,
    pub policy: String,
    pub encoder: String,
    pub bitrate_video: String,
    pub target_lufs: Option<f64>,
    pub output_dir: Option<String>,
    /// Sudah pernah menyelesaikan wizard penyiapan.
    pub onboarded: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ffmpeg_dir: None,
            duration_minutes: 60,
            video_loop_s: 10.0,
            video_xfade_s: 2.0,
            audio_xfade_s: 2.0,
            policy: "replace".into(),
            encoder: "quality".into(),
            bitrate_video: "4M".into(),
            target_lufs: Some(-14.0),
            output_dir: None,
            onboarded: false,
        }
    }
}

impl Config {
    /// Lokasi berkas konfigurasi: %APPDATA%\LofiLoop\config.json
    pub fn path() -> Result<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .ok_or_else(|| Error::Invalid("folder konfigurasi tidak ditemukan".into()))?;
        Ok(base.join("LofiLoop").join("config.json"))
    }

    pub fn load() -> Self {
        let Ok(path) = Self::path() else {
            return Self::default();
        };
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        Ok(())
    }
}

/// Folder data aplikasi untuk ffmpeg hasil unduhan otomatis.
pub fn auto_ffmpeg_dir() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .ok_or_else(|| Error::Invalid("folder data lokal tidak ditemukan".into()))?;
    Ok(base.join("LofiLoop").join("ffmpeg"))
}

/// Cari folder ffmpeg yang siap dipakai, berdasarkan urutan prioritas.
pub fn resolve_ffmpeg_dir(config_dir: Option<&str>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(dir) = config_dir {
        candidates.push(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("LOFILOOP_FFMPEG_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            candidates.push(root.join("assets").join("ffmpeg"));
            candidates.push(root.to_path_buf());
        }
    }
    if let Ok(auto) = auto_ffmpeg_dir() {
        candidates.push(auto);
    }

    for dir in candidates {
        if has_ffmpeg(&dir) {
            return Some(dir);
        }
        // Hasil ekstraksi sering menaruh berkas di dalam satu subfolder.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && has_ffmpeg(&path) {
                    return Some(path);
                }
            }
        }
    }
    None
}

pub fn has_ffmpeg(dir: &Path) -> bool {
    let ffmpeg = dir.join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    let ffprobe = dir.join(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    ffmpeg.is_file() && ffprobe.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip() {
        let dir = std::env::temp_dir().join("lofiloop-config-test");
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.display().to_string();
        let cfg = Config {
            ffmpeg_dir: Some(dir_text.clone()),
            duration_minutes: 30,
            ..Default::default()
        };
        let text = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.duration_minutes, 30);
        assert_eq!(back.ffmpeg_dir.as_deref(), Some(dir_text.as_str()));
    }

    #[test]
    fn has_ffmpeg_false_for_empty_dir() {
        let dir = std::env::temp_dir().join("lofiloop-empty-test");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!has_ffmpeg(&dir));
    }
}
