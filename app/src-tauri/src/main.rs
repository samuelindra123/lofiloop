//! Shell Tauri LofiLoop. Tipis: seluruh kerja ada di `lofiloop-engine`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod update;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use lofiloop_engine::machine::MachineReport;
use lofiloop_engine::merge::{self, MergeOptions};
use lofiloop_engine::render::{render as run_render, RenderOptions, RenderReport};
use lofiloop_engine::video;
use lofiloop_engine::{config, machine, probe, setup, Config, Ctx, Ffmpeg};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Crossfade video bawaan, dipakai untuk menghitung batas panjang loop saat berkas dipilih.
///
/// Nilainya harus sama dengan bawaan di `RenderOptions`. Angka ini hanya dipakai untuk
/// memberi tahu batas sebelum render; nilai yang benar-benar dipakai selalu yang dikirim UI.
const XFADE_VIDEO_DEFAULT: f64 = 2.0;

#[derive(Default)]
struct Session {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    running: Mutex<bool>,
    setup_running: Mutex<bool>,
}

#[derive(Serialize)]
struct EngineStatus {
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
struct MediaSummary {
    duration_s: f64,
    has_audio: bool,
    audible: bool,
    width: u32,
    height: u32,
    fps: f64,
    sample_rate: Option<u32>,
    channels: Option<u32>,
    /// Jumlah frame video, kalau terbaca.
    frame_count: Option<u64>,
    /// Panjang putaran loop terbesar yang muat untuk klip ini, detik, dihitung pada fps
    /// sumber. `None` kalau klipnya tidak bisa dipakai sebagai sumber loop.
    loop_maks_s: Option<f64>,
    /// fps sumber, dipakai UI untuk menghitung batas pada fps keluaran yang dipilih.
    fps_sumber: f64,
    /// Kalimat penjelas tentang batas itu, dalam satuan detik.
    loop_catatan: Option<String>,
}

#[derive(Serialize)]
struct RenderSummary {
    output: String,
    size_bytes: u64,
    duration_video_s: f64,
    duration_audio_s: Option<f64>,
    av_delta_s: f64,
    video_repeats: u64,
    elapsed_s: f64,
    policy_note: String,
    output_width: u32,
    output_height: u32,
    output_fps: f64,
}

impl From<RenderReport> for RenderSummary {
    fn from(r: RenderReport) -> Self {
        Self {
            output: r.output,
            size_bytes: r.size_bytes,
            duration_video_s: r.duration_video_s,
            duration_audio_s: r.duration_audio_s,
            av_delta_s: r.av_delta_s.unwrap_or(0.0),
            video_repeats: r.video_repeats,
            elapsed_s: r.elapsed_s,
            policy_note: r.policy_note,
            output_width: r.output_width,
            output_height: r.output_height,
            output_fps: r.output_fps,
        }
    }
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    message: String,
    fraction: Option<f64>,
}

/// Ambil folder ffmpeg dari konfigurasi pengguna, kalau ada.
fn configured_ffmpeg_dir() -> Option<String> {
    Config::load().ffmpeg_dir
}

fn discover_ffmpeg() -> Result<Ffmpeg, String> {
    let dir = configured_ffmpeg_dir();
    if let Some(path) = config::resolve_ffmpeg_dir(dir.as_deref()) {
        return Ffmpeg::from_dir(&path).map_err(|e| e.to_string());
    }
    Ffmpeg::discover().map_err(|e| e.to_string())
}

#[tauri::command]
fn check_engine() -> EngineStatus {
    match discover_ffmpeg() {
        Ok(ff) => {
            if !ff.has_encoder("libx264") && !ff.has_encoder("h264_qsv") {
                return EngineStatus {
                    ok: false,
                    detail: "FFmpeg tidak punya encoder H.264".into(),
                };
            }
            if !ff.has_encoder("aac") {
                return EngineStatus {
                    ok: false,
                    detail: "FFmpeg tidak punya encoder AAC".into(),
                };
            }
            EngineStatus {
                ok: true,
                detail: "siap".into(),
            }
        }
        Err(e) => EngineStatus {
            ok: false,
            detail: e,
        },
    }
}

/// Periksa mesin: ffmpeg, encoder (diuji nyata), GPU, disk, dan kecepatan encode.
#[tauri::command]
async fn inspect_machine() -> Result<MachineReport, String> {
    let dir = configured_ffmpeg_dir();
    tauri::async_runtime::spawn_blocking(move || machine::inspect(dir.as_deref()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn load_config() -> Config {
    Config::load()
}

#[tauri::command]
fn save_config(config: Config) -> Result<Config, String> {
    config.save().map_err(|e| e.to_string())?;
    Ok(config)
}

/// Unduh dan pasang FFmpeg otomatis, lalu simpan lokasinya di konfigurasi.
#[tauri::command]
async fn install_ffmpeg(
    app: AppHandle,
    state: State<'_, Session>,
) -> Result<setup::InstallReport, String> {
    if *state.setup_running.lock().unwrap() {
        return Err("pemasangan sedang berjalan".into());
    }
    *state.setup_running.lock().unwrap() = true;

    let app_for_task = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        setup::download_and_install(|p| {
            let _ = app_for_task.emit("download-progress", p);
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string());

    *state.setup_running.lock().unwrap() = false;

    let report: setup::InstallReport = result??;
    let mut cfg = Config::load();
    cfg.ffmpeg_dir = Some(report.dir.clone());
    cfg.save().map_err(|e| e.to_string())?;
    Ok(report)
}

#[tauri::command]
fn describe(path: String) -> Result<MediaSummary, String> {
    let ff = discover_ffmpeg()?;
    let info = probe::probe(&ff, &path).map_err(|e| e.to_string())?;
    let audible = if info.has_audio {
        probe::audio_peak_dbfs(&ff, &path)
            .map(|p| p > -50.0)
            .unwrap_or(false)
    } else {
        false
    };
    // Batas loop dihitung di sini, bukan di UI. Mesin yang tahu fps dan jumlah frame yang
    // sebenarnya, jadi hanya mesin yang bisa menghitungnya dengan benar. UI cukup memakai
    // angka ini untuk mengunci penggeser.
    let batas = video::batas_loop(&info, XFADE_VIDEO_DEFAULT, video::TargetVideo::asal());

    Ok(MediaSummary {
        duration_s: info.duration_s,
        has_audio: info.has_audio,
        audible,
        width: info.width,
        height: info.height,
        fps: info.fps,
        sample_rate: info.sample_rate,
        channels: info.channels,
        frame_count: info.frame_count,
        loop_maks_s: batas.maks_s,
        fps_sumber: batas.fps_sumber,
        loop_catatan: Some(batas.penjelasan()),
    })
}

#[derive(Serialize)]
struct MergeSummary {
    output: String,
    track_count: usize,
    duration_s: f64,
    source_total_s: f64,
    overlap_s: f64,
    size_bytes: u64,
    crossfade_s: f64,
    sample_rate: u32,
    channels: u32,
    normalized: bool,
    elapsed_s: f64,
}

impl From<lofiloop_engine::MergeReport> for MergeSummary {
    fn from(r: lofiloop_engine::MergeReport) -> Self {
        Self {
            output: r.output,
            track_count: r.track_count,
            duration_s: r.duration_s,
            source_total_s: r.source_total_s,
            overlap_s: r.overlap_s,
            size_bytes: r.size_bytes,
            crossfade_s: r.crossfade_s,
            sample_rate: r.sample_rate,
            channels: r.channels,
            normalized: r.normalized,
            elapsed_s: r.elapsed_s,
        }
    }
}

/// Dialog pilih folder. Dipakai halaman gabung supaya tujuan hasil jelas sejak awal.
#[tauri::command]
async fn pick_folder(app: AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut builder = app.dialog().file();
    if let Some(t) = title {
        builder = builder.set_title(&t);
    }
    builder.pick_folder(move |path| {
        let _ = tx.send(path);
    });
    let picked = rx.recv().map_err(|e| e.to_string())?;
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
async fn merge_songs(
    app: AppHandle,
    state: State<'_, Session>,
    opts: MergeOptions,
) -> Result<MergeSummary, String> {
    // Nilai mutex diambil di dalam blok tersendiri, supaya kuncinya benar-benar lepas
    // sebelum baris berikutnya. Menulis `if *state.running.lock().unwrap() { return }`
    // menahan kunci selama `return`, dan itu bisa membuat permintaan batal dari UI
    // menunggu di mutex yang sama.
    {
        let mut running = state.running.lock().unwrap();
        if *running {
            return Err("masih ada pekerjaan yang berjalan".into());
        }
        *running = true;
    }

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = state.cancel.lock().unwrap();
        *slot = Some(cancel.clone());
    }

    // Catatan: status `running` dibersihkan di bawah tanpa operator `?`, supaya tetap
    // dijalankan walau pekerjaan panik.
    let app_for_task = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg_dir = configured_ffmpeg_dir();
        let work_dir = std::path::Path::new(&opts.output).with_extension("work");
        let progress_app = app_for_task.clone();
        let reporter = Arc::new(move |msg: &str, fraction: Option<f64>| {
            let _ = progress_app.emit(
                "progress",
                ProgressEvent {
                    message: msg.to_string(),
                    fraction,
                },
            );
        });
        let ctx = Ctx::with_progress(&work_dir, reporter, cancel).map_err(|e| e.to_string())?;
        let ff = match config::resolve_ffmpeg_dir(ffmpeg_dir.as_deref()) {
            Some(dir) => Ffmpeg::from_dir(&dir).map_err(|e| e.to_string())?,
            None => Ffmpeg::discover().map_err(|e| e.to_string())?,
        };
        merge::merge(&ctx, &ff, &opts).map_err(|e| e.to_string())
    })
    .await;
    // Status dibersihkan tanpa operator `?`, sehingga tetap dijalankan walau pekerjaan
    // berhenti karena panik. Kalau baris ini terlewat, penanda "sedang berjalan" tidak
    // pernah dilepas dan aplikasi menolak semua pekerjaan berikutnya sampai ditutup.
    {
        let mut running = state.running.lock().unwrap();
        *running = false;
    }
    {
        let mut slot = state.cancel.lock().unwrap();
        *slot = None;
    }

    let result = result.map_err(|e| {
        format!("pekerjaan berhenti tak terduga: {e}. Status sudah direset, silakan coba lagi.")
    })?;

    result.map(MergeSummary::from)
}

#[tauri::command]
async fn render(
    app: AppHandle,
    state: State<'_, Session>,
    opts: RenderOptions,
) -> Result<RenderSummary, String> {
    if *state.running.lock().unwrap() {
        return Err("masih ada render yang berjalan".into());
    }
    *state.running.lock().unwrap() = true;

    let cancel = Arc::new(AtomicBool::new(false));
    *state.cancel.lock().unwrap() = Some(cancel.clone());

    let app_for_task = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg_dir = configured_ffmpeg_dir();
        let work_dir = std::path::Path::new(&opts.output).with_extension("work");

        let progress_app = app_for_task.clone();
        let reporter = Arc::new(move |msg: &str, fraction: Option<f64>| {
            let _ = progress_app.emit(
                "progress",
                ProgressEvent {
                    message: msg.to_string(),
                    fraction,
                },
            );
        });

        let ctx = Ctx::with_progress(&work_dir, reporter, cancel).map_err(|e| e.to_string())?;
        run_render(&ctx, &opts, ffmpeg_dir.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    *state.running.lock().unwrap() = false;
    *state.cancel.lock().unwrap() = None;

    let _ = app.emit(
        "progress",
        ProgressEvent {
            message: if result.is_ok() {
                "selesai".into()
            } else {
                "berhenti".into()
            },
            fraction: None,
        },
    );

    result.map(RenderSummary::from)
}

/// Apakah ada pekerjaan yang tercatat masih berjalan.
///
/// Dipakai UI untuk memberi tahu kalau status pernah terkunci, alih-alih membiarkan
/// pengguna menunggu tanpa kabar.
#[tauri::command]
async fn job_state(state: State<'_, Session>) -> Result<bool, String> {
    // WAJIB `async`. Perintah Tauri yang bukan async berjalan di thread utama, dan thread
    // itulah yang melayani seluruh UI: memblokirnya membuat jendela berhenti merespons
    // dan pemanggilnya menunggu selamanya.
    let running = {
        let guard = state.running.lock().unwrap();
        *guard
    };
    Ok(running)
}

#[tauri::command]
async fn cancel_job(state: State<'_, Session>) -> Result<(), String> {
    // Nilai mutex diambil dan dilepas di dalam blok, supaya kuncinya tidak tertahan
    // selama balasan dikirim ke UI.
    let ada_pekerjaan = {
        let slot = state.cancel.lock().unwrap();
        match slot.as_ref() {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    };

    if ada_pekerjaan {
        Ok(())
    } else {
        Err("tidak ada render yang berjalan".into())
    }
}

/// Periksa apakah ada versi yang lebih baru di GitHub Releases.
///
/// Dijalankan di luar thread utama karena menunggu jaringan bisa memakan detik-an, dan
/// thread utama adalah yang melayani seluruh UI.
///
/// Kegagalan di sini bukan kesalahan yang perlu ditampilkan ke pengguna: tidak ada internet,
/// atau GitHub sedang tidak bisa dihubungi, bukan alasan untuk mengganggu orang yang sedang
/// bekerja. Karena itu kegagalannya dikembalikan sebagai `None`, bukan sebagai error.
/// Hasil pemeriksaan versi, dikembalikan apa adanya termasuk saat gagal.
///
/// Kegagalan tidak dibiarkan tanpa kabar. Pemeriksaan yang diam-diam gagal membuat pengguna
/// mengira fiturnya rusak, padahal penyebabnya bisa jadi tidak ada internet atau jatah
/// permintaan GitHub sudah habis. Pesannya tetap singkat supaya tidak mengganggu.
#[derive(Serialize)]
struct UpdateHasil {
    info: Option<update::UpdateInfo>,
    /// Alasan singkat kalau pemeriksaan tidak berhasil. `None` berarti pemeriksaan berhasil.
    galat: Option<String>,
}

/// Periksa apakah ada versi yang lebih baru di GitHub Releases.
///
/// Dijalankan di luar thread utama karena menunggu jaringan bisa memakan detik-an, dan
/// thread utama adalah yang melayani seluruh UI.
///
/// `paksa` melewati hasil yang tersimpan. Tanpa paksaan, hasil yang masih berlaku dipakai
/// kembali sehingga GitHub hanya dihubungi sekali sehari walau aplikasi dibuka berkali-kali.
/// Batas itu perlu karena GitHub membatasi jumlah permintaan per alamat IP, dan terlampauinya
/// akan menolak seluruh permintaan dari alamat itu — termasuk dari aplikasi lain.
#[tauri::command]
async fn check_update(app: AppHandle, paksa: Option<bool>) -> UpdateHasil {
    let versi = app.package_info().version.to_string();
    let paksa = paksa.unwrap_or(false);
    let hasil =
        tauri::async_runtime::spawn_blocking(move || update::periksa_terjadwal(&versi, paksa))
            .await;

    match hasil {
        Ok(Ok(info)) => UpdateHasil {
            info: Some(info),
            galat: None,
        },
        Ok(Err(e)) => UpdateHasil {
            info: None,
            galat: Some(e),
        },
        Err(e) => UpdateHasil {
            info: None,
            galat: Some(format!("pemeriksaan gagal dijalankan: {e}")),
        },
    }
}

/// Buka sebuah alamat di peramban bawaan sistem.
///
/// Dipakai tombol unduh pada notifikasi pembaruan. Alamat yang diterima dibatasi hanya
/// `http` dan `https`, supaya perintah ini tidak bisa dipakai membuka berkas lokal atau
/// skema lain yang bisa menjalankan sesuatu.
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("alamat harus diawali https:// atau http://".into());
    }
    if url.contains(char::is_whitespace) || url.contains('"') || url.contains('\'') {
        return Err("alamat mengandung karakter yang tidak diizinkan".into());
    }

    // PowerShell dipanggil dengan daftar argumen, bukan lewat penyusunan satu baris perintah.
    // Dengan begitu isi `url` tidak pernah diurai ulang oleh shell, jadi tidak ada cara
    // menyisipkan perintah lewat alamat.
    tauri::async_runtime::spawn_blocking(move || {
        lofiloop_engine::proc::command_str("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Process",
                "-FilePath",
                &url,
            ])
            .status()
            .map_err(|e| format!("tidak bisa membuka peramban: {e}"))?
            .success()
            .then_some(())
            .ok_or_else(|| "peramban tidak bisa dibuka".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Session::default())
        .invoke_handler(tauri::generate_handler![
            check_engine,
            inspect_machine,
            load_config,
            save_config,
            install_ffmpeg,
            describe,
            merge_songs,
            pick_folder,
            render,
            cancel_job,
            job_state,
            check_update,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("gagal menjalankan LofiLoop");
}
