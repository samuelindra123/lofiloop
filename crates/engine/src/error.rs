//! Jenis error engine. Semuanya membawa cukup konteks untuk ditampilkan di CLI maupun UI.

use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    FfmpegMissing(PathBuf),
    Spawn {
        program: String,
        source: std::io::Error,
    },
    FfmpegFailed {
        label: String,
        code: Option<i32>,
        tail: String,
    },
    Io(std::io::Error),
    Json(serde_json::Error),
    Probe(String),
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::FfmpegMissing(p) => write!(
                f,
                "ffmpeg/ffprobe tidak ditemukan di {}.\nPasang FFmpeg dan tambahkan ke PATH, \
                 atau set LOFILOOP_FFMPEG_DIR ke folder berisi ffmpeg.exe.",
                p.display()
            ),
            Error::Spawn { program, source } => write!(f, "gagal menjalankan {program}: {source}"),
            Error::FfmpegFailed { label, code, tail } => write!(
                f,
                "{label} gagal (exit code {}): {tail}",
                code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
            ),
            Error::Io(e) => write!(f, "kesalahan I/O: {e}"),
            Error::Json(e) => write!(f, "gagal membaca keluaran JSON: {e}"),
            Error::Probe(msg) => write!(f, "media tidak terbaca: {msg}"),
            Error::Invalid(msg) => write!(f, "input tidak valid: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
