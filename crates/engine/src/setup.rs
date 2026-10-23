//! Unduh dan pasang ffmpeg otomatis ke folder data aplikasi.
//!
//! Sumber: build Windows resmi dari gyan.dev (LGPL/GPL, tergantung build).
//! Alur: unduh zip -> periksa SHA256 dari berkas .sha256 di sebelahnya -> ekstrak -> cari
//! folder berisi ffmpeg.exe -> kembalikan lokasinya.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::has_ffmpeg;
use crate::error::{Error, Result};

/// Berkas unduhan utama.
const ZIP_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
const SHA_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip.sha256";

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub phase: String,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub fraction: Option<f64>,
}

/// Laporan hasil pemasangan ffmpeg.
#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    pub dir: String,
    pub version: String,
    pub verified_sha256: bool,
    pub size_bytes: u64,
}

/// Unduh, verifikasi, lalu pasang ffmpeg. `progress` dipanggil berkala.
pub fn download_and_install<F>(progress: F) -> Result<InstallReport>
where
    F: Fn(DownloadProgress),
{
    let dir = crate::config::auto_ffmpeg_dir()?;
    std::fs::create_dir_all(&dir)?;
    let zip_path = dir.join("ffmpeg-download.zip");

    progress(DownloadProgress {
        phase: "menyiapkan unduhan".into(),
        received_bytes: 0,
        total_bytes: None,
        fraction: Some(0.0),
    });

    let expected = fetch_text(SHA_URL)
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_string));

    download_zip(ZIP_URL, &zip_path, &progress)?;

    let mut verified = false;
    if let Some(expected) = expected {
        let actual = sha256_file(&zip_path)?;
        if !actual.eq_ignore_ascii_case(&expected) {
            let _ = std::fs::remove_file(&zip_path);
            return Err(Error::Invalid(format!(
                "berkas unduhan rusak: SHA256 tidak cocok (harapan {expected}, dapat {actual})"
            )));
        }
        verified = true;
    }

    progress(DownloadProgress {
        phase: "mengekstrak".into(),
        received_bytes: 0,
        total_bytes: None,
        fraction: None,
    });

    let extract_dir = dir.join("extract");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir)?;
    extract_zip(&zip_path, &extract_dir)?;

    let found = find_ffmpeg_dir(&extract_dir).ok_or_else(|| {
        Error::Invalid("ffmpeg.exe tidak ditemukan di dalam berkas unduhan".into())
    })?;

    // Pindahkan ke folder akhir supaya lokasinya stabil.
    let final_dir = dir.join("bin");
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::create_dir_all(&final_dir)?;
    for entry in std::fs::read_dir(&found)? {
        let entry = entry?;
        if entry.path().is_file() {
            let target = final_dir.join(entry.file_name());
            std::fs::copy(entry.path(), &target)?;
        }
    }

    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_dir_all(&extract_dir);

    if !has_ffmpeg(&final_dir) {
        return Err(Error::Invalid(
            "pemasangan gagal: ffmpeg.exe tidak ada di folder tujuan".into(),
        ));
    }

    let version = read_version(&final_dir).unwrap_or_else(|| "tidak terbaca".into());
    let size = std::fs::metadata(final_dir.join(exe_name("ffmpeg")))
        .map(|m| m.len())
        .unwrap_or(0);

    progress(DownloadProgress {
        phase: "selesai".into(),
        received_bytes: 0,
        total_bytes: None,
        fraction: Some(1.0),
    });

    Ok(InstallReport {
        dir: final_dir.display().to_string(),
        version,
        verified_sha256: verified,
        size_bytes: size,
    })
}

fn download_zip<F>(url: &str, dest: &Path, progress: &F) -> Result<()>
where
    F: Fn(DownloadProgress),
{
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; \
         Invoke-WebRequest -UseBasicParsing -Uri '{url}' -OutFile '{out}'",
        url = url,
        out = dest.display()
    );
    let child = crate::proc::command_str("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .spawn()
        .map_err(|e| Error::Invalid(format!("gagal menjalankan pengunduh: {e}")))?;

    // Pantau ukuran berkas selama unduhan supaya UI bisa menampilkan kemajuan.
    let mut last: u64 = 0;
    let mut child = child;
    // Tunggu proses unduh selesai sambil melaporkan ukuran berkas yang bertambah.
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let size = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
                if size != last {
                    last = size;
                    progress(DownloadProgress {
                        phase: "mengunduh ffmpeg".into(),
                        received_bytes: size,
                        // Ukuran akhir tidak diketahui sebelum selesai; UI menampilkan MB saja.
                        total_bytes: None,
                        fraction: None,
                    });
                }
            }
            Err(e) => return Err(Error::Invalid(format!("unduhan gagal: {e}"))),
        }
    };

    if !status.success() || !dest.is_file() {
        let _ = std::fs::remove_file(dest);
        return Err(Error::Invalid(
            "unduhan ffmpeg gagal. Periksa koneksi internet lalu coba lagi.".into(),
        ));
    }
    progress(DownloadProgress {
        phase: "unduhan selesai".into(),
        received_bytes: last,
        total_bytes: Some(last),
        fraction: Some(1.0),
    });
    Ok(())
}

fn fetch_text(url: &str) -> Result<String> {
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; \
         (Invoke-WebRequest -UseBasicParsing -Uri '{url}').Content",
        url = url
    );
    let out = crate::proc::command_str("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| Error::Invalid(format!("gagal mengambil {url}: {e}")))?;
    if !out.status.success() {
        return Err(Error::Invalid(format!("gagal mengambil {url}")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn sha256_file(path: &Path) -> Result<String> {
    // Get-FileHash tersedia di semua Windows modern.
    let script = format!("(Get-FileHash -Algorithm SHA256 '{}').Hash", path.display());
    let out = crate::proc::command_str("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| Error::Invalid(format!("gagal menghitung hash: {e}")))?;
    if !out.status.success() {
        return Err(Error::Invalid(
            "gagal menghitung SHA256 berkas unduhan".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_lowercase())
}

fn extract_zip(zip: &Path, dest: &Path) -> Result<()> {
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; \
         Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        zip.display(),
        dest.display()
    );
    let status = crate::proc::command_str("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|e| Error::Invalid(format!("gagal mengekstrak: {e}")))?;
    if !status.success() {
        return Err(Error::Invalid("gagal mengekstrak berkas unduhan".into()));
    }
    Ok(())
}

/// Cari folder berisi ffmpeg.exe di dalam hasil ekstraksi (sering ada satu subfolder).
fn find_ffmpeg_dir(root: &Path) -> Option<PathBuf> {
    if has_ffmpeg(root) {
        return Some(root.to_path_buf());
    }
    // Kedalaman dua sudah cukup untuk struktur build gyan.dev (folder/bin/).
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if has_ffmpeg(&path) {
            return Some(path);
        }
        if let Ok(inner) = std::fs::read_dir(&path) {
            for sub in inner.flatten() {
                let sub_path = sub.path();
                if sub_path.is_dir() && has_ffmpeg(&sub_path) {
                    return Some(sub_path);
                }
            }
        }
    }
    None
}

fn read_version(dir: &Path) -> Option<String> {
    let exe = dir.join(exe_name("ffmpeg"));
    let out = crate::proc::command(Path::new(exe))
        .arg("-version")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string())
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}
