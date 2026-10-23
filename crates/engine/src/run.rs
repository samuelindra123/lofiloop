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

/// Hapus folder kerja setelah pekerjaan selesai.
///
/// Dipanggil hanya setelah semua hasil jadi, dan sengaja mengembalikan `()`: kegagalan
/// menghapus tidak boleh menjatuhkan pekerjaan yang sebenarnya sudah berhasil. Berkas
/// yang masih terkunci proses lain akan terhapus pada pembersihan berikutnya, dan kalau
/// tidak pernah, folder itu hanya tersisa — bukan hasil yang rusak.
///
/// Folder dihapus lewat `Ctx` sendiri, bukan lewat jalur keluaran, supaya kalau suatu saat
/// lokasi folder kerja berubah, pembersihannya ikut berubah tanpa perlu dicari ke mana-mana.
pub fn bersihkan_kerja(ctx: &Ctx) {
    let dir = ctx.work_dir();
    if dir.as_os_str().is_empty() {
        return;
    }
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!(
            "[lofiloop] folder kerja belum bisa dihapus: {} ({e})",
            dir.display()
        ),
    }
}

/// Bersihkan folder kerja **hanya kalau isinya kosong atau cuma berkas sisa yang tidak berguna**.
///
/// Dipanggil setelah pekerjaan gagal. Folder yang berisi berkas antara sengaja dibiarkan,
/// karena isinya itulah yang berguna untuk mencari tahu apa yang salah. Sebaliknya, folder
/// kosong tidak memberi petunjuk apa pun dan hanya meninggalkan sampah, jadi dihapus.
///
/// Pemeriksaannya tidak menelusuri subfolder: cukup melihat apakah ada entri di tingkat
/// teratas, karena berkas antara selalu ditulis langsung di sana. Kegagalan membaca folder
/// diperlakukan sebagai "jangan hapus", karena membiarkan folder tersisa jauh lebih ringan
/// akibatnya daripada menghapus sesuatu yang masih dibutuhkan.
pub fn bersihkan_kerja_kalau_kosong(ctx: &Ctx) {
    let dir = ctx.work_dir();
    if dir.as_os_str().is_empty() {
        return;
    }
    let kosong = match std::fs::read_dir(dir) {
        Ok(mut isi) => isi.next().is_none(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            eprintln!(
                "[lofiloop] folder kerja tidak bisa dibaca, dibiarkan: {} ({e})",
                dir.display()
            );
            return;
        }
    };
    if kosong {
        let _ = std::fs::remove_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bersihkan_kerja_menghapus_folder_beserta_isinya() {
        let dir = std::env::temp_dir().join("lofiloop-bersih-kerja");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("unit_video.mp4"), b"x").unwrap();
        let ctx = Ctx::for_cli(&dir).unwrap();

        bersihkan_kerja(&ctx);

        assert!(!dir.exists(), "folder kerja harus sudah tidak ada");
    }

    #[test]
    fn bersihkan_kerja_aman_kalau_folder_belum_ada() {
        let dir = std::env::temp_dir().join("lofiloop-bersih-kerja-kosong");
        let _ = std::fs::remove_dir_all(&dir);
        let ctx = Ctx {
            work_dir: dir.clone(),
            ..Ctx::for_cli(std::env::temp_dir()).unwrap()
        };

        bersihkan_kerja(&ctx);

        assert!(!dir.exists());
    }

    #[test]
    fn folder_kerja_kosong_dihapus_setelah_gagal() {
        let dir = std::env::temp_dir().join("lofiloop-kerja-kosong-setelah-gagal");
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = Ctx::for_cli(&dir).unwrap();

        bersihkan_kerja_kalau_kosong(&ctx);

        assert!(
            !dir.exists(),
            "folder kosong tidak memberi petunjuk apa pun"
        );
    }

    #[test]
    fn folder_kerja_berisi_disimpan_agar_bisa_diperiksa() {
        let dir = std::env::temp_dir().join("lofiloop-kerja-berisi-setelah-gagal");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("unit_video.mp4"), b"x").unwrap();
        let ctx = Ctx::for_cli(&dir).unwrap();

        bersihkan_kerja_kalau_kosong(&ctx);

        assert!(
            dir.exists(),
            "isi folder kerja harus tetap ada untuk diperiksa"
        );
        assert!(dir.join("unit_video.mp4").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
