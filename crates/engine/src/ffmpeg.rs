//! Pemanggilan proses ffmpeg/ffprobe. Semua perintah lewat sini supaya progres dan
//! pembatalan bisa ditambahkan di satu tempat.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::error::{Error, Result};
use crate::proc::{command, command_str};

/// Lokasi ffmpeg dan ffprobe yang dipakai.
///
/// Urutan pencarian: `LOFILOOP_FFMPEG_DIR`, folder `assets/ffmpeg` di sebelah exe,
/// lalu PATH sistem.
#[derive(Debug, Clone)]
pub struct Ffmpeg {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Ffmpeg {
    /// Ffmpeg tiruan untuk pengujian: hanya menyusun argumen, tidak menjalankan apa pun.
    pub fn for_test() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
        }
    }

    pub fn discover() -> Result<Self> {
        if let Some(ff) = Self::from_config() {
            return Ok(ff);
        }
        if let Some(ff) = Self::from_env() {
            return Ok(ff);
        }
        if let Some(ff) = Self::from_app_dirs() {
            return Ok(ff);
        }
        if let Some(ff) = Self::from_known_locations() {
            return Ok(ff);
        }
        Ok(Self {
            ffmpeg: PathBuf::from(ffmpeg_exe()),
            ffprobe: PathBuf::from(ffprobe_exe()),
        })
    }

    /// Folder yang disimpan di konfigurasi pengguna, termasuk hasil unduhan otomatis.
    fn from_config() -> Option<Self> {
        let config = crate::config::Config::load();
        let dir = config.ffmpeg_dir?;
        Self::from_dir(std::path::Path::new(&dir)).ok()
    }

    fn from_env() -> Option<Self> {
        let dir = std::env::var_os("LOFILOOP_FFMPEG_DIR")?;
        Self::from_dir(std::path::Path::new(&dir)).ok()
    }

    /// Folder ffmpeg bawaan aplikasi: di sebelah exe, atau folder data hasil unduhan.
    fn from_app_dirs() -> Option<Self> {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(root) = exe.parent() {
                for candidate in [root.join("assets").join("ffmpeg"), root.to_path_buf()] {
                    if let Ok(ff) = Self::from_dir(&candidate) {
                        return Some(ff);
                    }
                }
            }
        }
        if let Ok(auto) = crate::config::auto_ffmpeg_dir() {
            if let Ok(ff) = Self::from_dir(&auto) {
                return Some(ff);
            }
            if let Ok(ff) = Self::from_dir(&auto.join("bin")) {
                return Some(ff);
            }
        }
        None
    }

    /// Lokasi umum pemasangan manual. Dipakai supaya aplikasi tetap bekerja walau
    /// dijalankan dari proses yang PATH-nya tidak memuat folder ffmpeg.
    fn from_known_locations() -> Option<Self> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(pf).join("ffmpeg").join("bin"));
        }
        if cfg!(windows) {
            candidates.push(PathBuf::from(r"C:\tools\ffmpeg\bin"));
            candidates.push(PathBuf::from(r"C:\ffmpeg\bin"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let root = PathBuf::from(local).join("LofiLoop").join("ffmpeg");
            candidates.push(root.clone());
            candidates.push(root.join("bin"));
        }
        // PATH sistem, dibaca langsung dari registry kalau proses tidak mewarisinya.
        candidates.extend(Self::registry_path_dirs());

        for dir in candidates {
            if let Ok(ff) = Self::from_dir(&dir) {
                return Some(ff);
            }
        }
        None
    }

    /// Baca PATH dari registry Windows. Berguna kalau aplikasi dijalankan dari proses
    /// yang PATH-nya sudah usang, misalnya setelah pemasangan baru.
    fn registry_path_dirs() -> Vec<PathBuf> {
        if !cfg!(windows) {
            return Vec::new();
        }
        let script = "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + \
                      [Environment]::GetEnvironmentVariable('Path','User')";
        let out = command_str("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output();
        let Ok(out) = out else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&out.stdout)
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    pub fn from_dir(dir: &Path) -> Result<Self> {
        let ffmpeg = dir.join(ffmpeg_exe());
        let ffprobe = dir.join(ffprobe_exe());
        if !ffmpeg.is_file() {
            return Err(Error::FfmpegMissing(ffmpeg));
        }
        if !ffprobe.is_file() {
            return Err(Error::FfmpegMissing(ffprobe));
        }
        Ok(Self { ffmpeg, ffprobe })
    }

    /// Jalankan ffmpeg. Gagal kalau exit code bukan 0.
    pub fn run(&self, args: &[String], label: &str) -> Result<String> {
        let out = command(&self.ffmpeg)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Spawn {
                program: self.ffmpeg.display().to_string(),
                source: e,
            })?;
        if !out.status.success() {
            let tail = tail_of(&out.stderr);
            return Err(Error::FfmpegFailed {
                label: label.to_string(),
                code: out.status.code(),
                tail,
            });
        }
        Ok(String::from_utf8_lossy(&out.stderr).into_owned())
    }

    /// Jalankan ffmpeg sambil melaporkan kemajuan nyata dari dalam proses.
    ///
    /// Memakai `-progress pipe:1` sehingga ffmpeg sendiri yang memberi tahu sudah sampai
    /// detik berapa. Tanpa ini, tahap terberat (penggabungan panjang) tidak bisa dilaporkan
    /// dan bar kemajuan akan diam di satu angka sampai prosesnya selesai.
    ///
    /// `expected_s` adalah perkiraan durasi total keluaran; `base` dan `span` menentukan
    /// bagian mana dari bar 0..1 yang diisi tahap ini.
    pub fn run_with_progress(
        &self,
        args: &[String],
        label: &str,
        expected_s: f64,
        base: f64,
        span: f64,
        report: &dyn Fn(f64),
    ) -> Result<()> {
        let mut full: Vec<String> = args.to_vec();
        // -progress harus ditulis sebelum output supaya berlaku untuk seluruh proses.
        if let Some(pos) = full.iter().position(|a| a == "-y") {
            full.insert(pos + 1, "-progress".into());
            full.insert(pos + 2, "pipe:1".into());
            full.insert(pos + 3, "-nostats".into());
        } else {
            full.insert(0, "-progress".into());
            full.insert(1, "pipe:1".into());
        }

        let mut child = command(&self.ffmpeg)
            .args(&full)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Spawn {
                program: self.ffmpeg.display().to_string(),
                source: e,
            })?;

        // stdout dibaca di thread terpisah. Kalau dibaca langsung, ffmpeg yang belum
        // mengirim apa pun akan membuat thread ini ikut menunggu, dan tidak ada denyut
        // waktu yang bisa dikirim ke UI. Di mesin lambat, jeda itu bisa puluhan detik.
        let (tx, rx) = std::sync::mpsc::channel::<f64>();
        let stdout = child.stdout.take();
        let reader = std::thread::spawn(move || {
            if let Some(stdout) = stdout {
                let reader = std::io::BufReader::new(stdout);
                for line in std::io::BufRead::lines(reader).map_while(|l| l.ok()) {
                    if let Some(value) = line.strip_prefix("out_time_ms=") {
                        if let Ok(micros) = value.trim().parse::<i64>() {
                            if tx.send(micros as f64 / 1_000_000.0).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        // Denyut berkala: pakai kemajuan terakhir yang diketahui, sehingga bar tetap
        // bergerak walau ffmpeg belum mengirim laporan baru.
        let started = std::time::Instant::now();
        let mut last_seen = 0.0f64;
        let mut last_value = -1.0f64;
        loop {
            // Tunggu laporan ffmpeg, tapi tidak lebih dari setengah detik supaya
            // denyut waktu tetap berjalan.
            match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(seconds) => last_seen = seconds,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
            if expected_s > 0.0 {
                // Kalau ffmpeg lama diam, majukan sendiri berdasarkan waktu berjalan
                // sebagai perkiraan sementara, tidak pernah melebihi batas tahap ini.
                let elapsed = started.elapsed().as_secs_f64();
                let by_time = if expected_s > 0.0 {
                    elapsed / expected_s
                } else {
                    0.0
                };
                let fraction = (last_seen / expected_s).max(by_time * 0.5).clamp(0.0, 1.0);
                let value = base + span * fraction;
                if value - last_value >= 0.01 {
                    last_value = value;
                    report(value);
                }
            }
        }
        let _ = reader.join();

        let status = child.wait().map_err(|e| Error::Spawn {
            program: self.ffmpeg.display().to_string(),
            source: e,
        })?;
        let stderr_text = child
            .stderr
            .take()
            .map(|mut s| {
                let mut buf = String::new();
                let _ = std::io::Read::read_to_string(&mut s, &mut buf);
                buf
            })
            .unwrap_or_default();
        if !status.success() {
            return Err(Error::FfmpegFailed {
                label: label.to_string(),
                code: status.code(),
                tail: tail_of(stderr_text.as_bytes()),
            });
        }
        report((base + span).clamp(0.0, 1.0));
        Ok(())
    }

    /// Jalankan ffprobe dan ambil stdout sebagai teks JSON.
    pub fn probe_json(&self, args: &[String]) -> Result<serde_json::Value> {
        let out = command(&self.ffprobe)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Spawn {
                program: self.ffprobe.display().to_string(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(Error::FfmpegFailed {
                label: "ffprobe".into(),
                code: out.status.code(),
                tail: tail_of(&out.stderr),
            });
        }
        serde_json::from_slice(&out.stdout).map_err(Error::Json)
    }

    /// Jalankan ffprobe dan ambil stdout sebagai teks mentah.
    pub fn probe_text(&self, args: &[String]) -> Result<String> {
        let out = command(&self.ffprobe)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Spawn {
                program: self.ffprobe.display().to_string(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(Error::FfmpegFailed {
                label: "ffprobe".into(),
                code: out.status.code(),
                tail: tail_of(&out.stderr),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Jalankan ffmpeg dan ambil stdout sebagai teks (untuk -encoders, -filters, dan sejenisnya).
    pub fn run_stdout(&self, args: &[String], label: &str) -> Result<String> {
        let out = command(&self.ffmpeg)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Spawn {
                program: self.ffmpeg.display().to_string(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(Error::FfmpegFailed {
                label: label.to_string(),
                code: out.status.code(),
                tail: tail_of(&out.stderr),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Apakah encoder tertentu ada di build ini. Daftar encoder keluar di stdout.
    /// Pilih encoder AAC tercepat yang tersedia di mesin ini.
    ///
    /// `aac_mf` memakai MediaFoundation Windows dan terukur dua kali lebih cepat daripada
    /// encoder AAC bawaan ffmpeg (49 dtk vs 105 dtk untuk 30 menit audio), dengan ukuran
    /// dan mutu hasil yang sama. Kalau tidak tersedia, pakai encoder bawaan.
    pub fn best_aac(&self) -> String {
        use std::sync::OnceLock;
        static CACHE: OnceLock<String> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                if self.has_encoder("aac_mf") {
                    "aac_mf".to_string()
                } else {
                    "aac".to_string()
                }
            })
            .clone()
    }

    pub fn has_encoder(&self, name: &str) -> bool {
        self.run_stdout(&["-hide_banner".into(), "-encoders".into()], "encoders")
            .map(|s| s.contains(name))
            .unwrap_or(false)
    }

    /// Nama encoder video terbaik yang tersedia: QSV kalau ada, kalau tidak libx264.
    ///
    /// Catatan penting: **jangan** pakai ini sebagai default. Pengukuran Fase 2 menunjukkan
    /// unit loop yang di-encode `h264_qsv` menghasilkan lompatan di sambungan (rasio metrik
    /// 1,73) sementara `libx264` dengan argumen sama menghasilkan 1,38–1,22. Encode
    /// hardware di UHD 605 tampaknya menaruh artefak di frame pertama tiap GOP, dan karena
    /// setiap pengulangan dimulai dari keyframe, artefak itu berulang tepat di sambungan.
    pub fn available_video_encoder(&self) -> VideoEncoder {
        if self.has_encoder("h264_qsv") {
            VideoEncoder::Qsv
        } else {
            VideoEncoder::X264
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncoder {
    Qsv,
    X264,
}

impl VideoEncoder {
    pub fn name(self) -> &'static str {
        match self {
            VideoEncoder::Qsv => "h264_qsv (hardware)",
            VideoEncoder::X264 => "libx264",
        }
    }

    /// Argumen encoder untuk kualitas yang layak pakai di bitrate sedang.
    ///
    /// `tune = seam` memakai pengaturan yang menjaga sambungan loop tetap halus: encoder
    /// yang sama untuk setiap unit, plus filter `deblock` ringan supaya blending crossfade
    /// tidak meninggalkan artefak blok di frame yang berulang.
    pub fn args(self, bitrate: &str, tune_seam: bool) -> Vec<String> {
        let mut args = match self {
            VideoEncoder::Qsv => vec![
                "-c:v".into(),
                "h264_qsv".into(),
                "-b:v".into(),
                bitrate.into(),
                "-preset".into(),
                "medium".into(),
            ],
            VideoEncoder::X264 => vec![
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "veryfast".into(),
                "-b:v".into(),
                bitrate.into(),
                "-maxrate".into(),
                bitrate.into(),
                "-bufsize".into(),
                "8M".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
            ],
        };
        if tune_seam && self == VideoEncoder::X264 {
            args.extend([
                "-tune".into(),
                "animation".into(),
                "-deblock".into(),
                "1:1".into(),
            ]);
        }
        args
    }
}

fn ffmpeg_exe() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn ffprobe_exe() -> &'static str {
    if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    }
}

/// Beberapa baris terakhir stderr, untuk pesan error yang berguna.
fn tail_of(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(4);
    lines[start..].join(" | ")
}
