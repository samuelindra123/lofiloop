//! Titik masuk engine. Semua pekerjaan berat ada di sini, tanpa ketergantungan ke UI.

pub mod audio;
pub mod config;
pub mod error;
pub mod ffmpeg;
pub mod machine;
pub mod merge;
pub mod probe;
pub mod proc;
pub mod render;
pub mod run;
pub mod setup;
pub mod video;

pub use config::Config;
pub use error::{Error, Result};
pub use ffmpeg::Ffmpeg;
pub use machine::{inspect, MachineReport};
pub use merge::{merge, MergeOptions, MergeReport};
pub use probe::{audio_peak_dbfs, probe, MediaInfo};
pub use render::{render, AudioPolicy, EncoderChoice, RenderOptions, RenderReport};
pub use run::{bersihkan_kerja, Ctx};
pub use setup::{download_and_install, InstallReport};
