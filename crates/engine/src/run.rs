//! Konteks satu pekerjaan: folder kerja, pelaporan langkah, dan pembatalan.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{Error, Result};

/// Pelapor kemajuan: pesan untuk dibaca, dan pecahan 0..1 kalau tahapnya bisa diukur.
pub type ProgressFn = Arc<dyn Fn(&str, Option<f64>) + Send + Sync>;

#[derive(Clone)]
pub struct Ctx {
    pub work_dir: PathBuf,
    cancel: Arc<AtomicBool>,
    progress: ProgressFn,
}

impl Ctx {
    /// Ctx untuk CLI: melaporkan ke stdout dan membuat folder kerja sendiri.
    pub fn for_cli(work_dir: impl Into<PathBuf>) -> Result<Self> {
        let work_dir = work_dir.into();
        std::fs::create_dir_all(&work_dir)?;
        Ok(Self {
            work_dir,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|msg: &str, _fraction: Option<f64>| println!("[lofiloop] {msg}")),
        })
    }

    /// Ctx dengan pelapor sendiri, dipakai UI.
    pub fn with_progress(
        work_dir: impl Into<PathBuf>,
        progress: ProgressFn,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self> {
        let work_dir = work_dir.into();
        std::fs::create_dir_all(&work_dir)?;
        Ok(Self {
            work_dir,
            cancel,
            progress,
        })
    }

    pub fn step(&self, msg: &str) {
        (self.progress)(msg, None);
    }

    /// Laporkan langkah beserta kemajuan 0..1, dipakai tahap yang jumlahnya bisa dihitung.
    pub fn step_at(&self, msg: &str, fraction: f64) {
        (self.progress)(msg, Some(fraction.clamp(0.0, 1.0)));
    }

    pub fn work_file(&self, name: &str) -> PathBuf {
        self.work_dir.join(name)
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Dipanggil di antara langkah-langkah panjang.
    pub fn check_cancel(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Invalid("pekerjaan dibatalkan".into()))
        } else {
            Ok(())
        }
    }
}
