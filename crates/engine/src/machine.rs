//! Pemeriksaan kesiapan mesin: ffmpeg, encoder, dan dukungan GPU.
//!
//! Hasilnya dipakai wizard penyiapan di UI untuk memberi tahu user apa yang kurang
//! dan apa yang bisa diperbaiki otomatis.

use serde::{Deserialize, Serialize};

use crate::ffmpeg::Ffmpeg;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderSupport {
    pub name: String,
    pub available: bool,
    /// Terbukti jalan lewat uji encode pendek, bukan cuma terdaftar.
    pub tested: Option<bool>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineReport {
    pub ffmpeg_found: bool,
    pub ffmpeg_dir: Option<String>,
    pub ffmpeg_version: Option<String>,
    pub ffprobe_found: bool,
    pub encoders: Vec<EncoderSupport>,
    /// Encoder pilihan default, dengan alasan yang bisa dibaca user.
    pub recommended_encoder: String,
    pub recommendation_note: String,
    pub cpu: String,
    pub logical_cores: u32,
    pub gpu: Option<String>,
    /// Perkiraan kecepatan: rasio waktu nyata dari uji encode pendek.
    pub encode_speed_realtime: Option<f64>,
    pub disk_free_gb: Option<f64>,
    pub problems: Vec<String>,
    pub ready: bool,
}

/// Periksa seluruh mesin dan kembalikan laporan siap-tampil.
pub fn inspect(ffmpeg_dir: Option<&str>) -> MachineReport {
    let mut problems: Vec<String> = Vec::new();

    let resolved = crate::config::resolve_ffmpeg_dir(ffmpeg_dir);
    let ffmpeg_dir_text = resolved.as_ref().map(|p| p.display().to_string());

    let ff = match resolved.as_ref().map(|dir| Ffmpeg::from_dir(dir)) {
        Some(Ok(ff)) => Some(ff),
        Some(Err(e)) => {
            problems.push(e.to_string());
            None
        }
        None => Ffmpeg::discover()
            .ok()
            .filter(|ff| ff.has_encoder("libx264") || ff.has_encoder("h264_qsv")),
    };

    let (cpu, cores, gpu) = machine_info();

    let Some(ff) = ff else {
        problems.push(
            "FFmpeg belum ditemukan. Unduh otomatis dari dalam aplikasi, atau tunjuk folder yang berisi ffmpeg.exe.".into(),
        );
        return MachineReport {
            ffmpeg_found: false,
            ffmpeg_dir: ffmpeg_dir_text,
            ffmpeg_version: None,
            ffprobe_found: false,
            encoders: Vec::new(),
            recommended_encoder: "libx264".into(),
            recommendation_note: "belum bisa dinilai sebelum ffmpeg ada".into(),
            cpu,
            logical_cores: cores,
            gpu,
            encode_speed_realtime: None,
            disk_free_gb: disk_free_gb(),
            problems,
            ready: false,
        };
    };

    let version = ff
        .run(&["-hide_banner".into(), "-version".into()], "version")
        .ok()
        .and_then(|_| {
            ff.ffmpeg
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        });

    let mut encoders = vec![
        EncoderSupport {
            name: "libx264".into(),
            available: ff.has_encoder("libx264"),
            tested: None,
            note: "CPU. Sambungan loop paling halus, dipakai default.".into(),
        },
        EncoderSupport {
            name: "h264_qsv".into(),
            available: ff.has_encoder("h264_qsv"),
            tested: None,
            note: "GPU Intel. Lebih cepat, tapi sambungan loop lebih kasar.".into(),
        },
    ];

    for enc in encoders.iter_mut() {
        if !enc.available {
            continue;
        }
        // Uji nyata: encode satu detik, lihat apakah benar-benar jalan.
        let tested = test_encoder(&ff, &enc.name).is_some();
        enc.tested = Some(tested);
    }

    let x264_ok = encoders
        .iter()
        .any(|e| e.name == "libx264" && e.available && e.tested == Some(true));
    let qsv_ok = encoders
        .iter()
        .any(|e| e.name == "h264_qsv" && e.available && e.tested == Some(true));

    let (recommended, note) = if x264_ok {
        (
            "quality".to_string(),
            "libx264 tersedia dan sudah diuji. Ini pilihan dengan sambungan paling halus."
                .to_string(),
        )
    } else if qsv_ok {
        (
            "hardware".to_string(),
            "libx264 tidak ada, jadi dipakai encoder GPU. Hasil sambungan lebih kasar.".to_string(),
        )
    } else {
        problems.push("Tidak ada encoder H.264 yang bisa dipakai.".into());
        (
            "quality".to_string(),
            "tidak ada encoder yang lulus uji".to_string(),
        )
    };

    if !ff.has_encoder("aac") {
        problems.push("Encoder AAC tidak ada, audio hasil tidak bisa dibuat.".into());
    }

    let speed = if x264_ok {
        Some(measure_speed(&ff))
    } else {
        None
    };
    let ready = problems.is_empty();

    MachineReport {
        ffmpeg_found: true,
        ffmpeg_dir: ff
            .ffmpeg
            .parent()
            .map(|p| p.display().to_string())
            .or(ffmpeg_dir_text),
        ffmpeg_version: version,
        ffprobe_found: true,
        encoders,
        recommended_encoder: recommended,
        recommendation_note: note,
        cpu,
        logical_cores: cores,
        gpu,
        encode_speed_realtime: speed,
        disk_free_gb: disk_free_gb(),
        problems,
        ready,
    }
}

/// Encode satu detik video uji. `None` berarti encoder tidak jalan.
fn test_encoder(ff: &Ffmpeg, name: &str) -> Option<()> {
    let out = std::env::temp_dir().join(format!("lofiloop-test-{name}.mp4"));
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "testsrc=d=1:s=320x180:r=25".into(),
        "-c:v".into(),
        name.into(),
        out.display().to_string(),
    ];
    let ok = ff.run(&args, "uji encoder").is_ok();
    let _ = std::fs::remove_file(&out);
    ok.then_some(())
}

/// Ukur kecepatan encode relatif terhadap waktu nyata, dari klip uji pendek.
fn measure_speed(ff: &Ffmpeg) -> f64 {
    let out = std::env::temp_dir().join("lofiloop-speed-test.mp4");
    let start = std::time::Instant::now();
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "testsrc=s=1280x720:r=25:d=4".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-b:v".into(),
        "4M".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        out.display().to_string(),
    ];
    let ok = ff.run(&args, "uji kecepatan").is_ok();
    let elapsed = start.elapsed().as_secs_f64();
    let _ = std::fs::remove_file(&out);
    if !ok || elapsed <= 0.0 {
        return 0.0;
    }
    // 4 detik video dibagi waktu encode = berapa kali lebih cepat dari waktu nyata.
    4.0 / elapsed
}

fn machine_info() -> (String, u32, Option<String>) {
    let name = std::env::var("PROCESSOR_IDENTIFIER")
        .ok()
        .or_else(|| std::env::var("PROCESSOR_ARCHITECTURE").ok())
        .unwrap_or_else(|| "CPU tidak terbaca".into());
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(0);
    let gpu = gpu_name();
    (name, cores, gpu)
}

/// Baca nama GPU dari wmic/PowerShell. Kalau gagal, kembalikan None.
fn gpu_name() -> Option<String> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_VideoController | Select-Object -First 1 -ExpandProperty Name)",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Ruang disk bebas di drive tempat konfigurasi disimpan, dalam GB.
fn disk_free_gb() -> Option<f64> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[math]::Round((Get-PSDrive ($env:SystemDrive -replace ':','')).Free / 1GB, 1)",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .ok()
}
