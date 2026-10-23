//! Render akhir: gabungkan loop video dan loop audio sampai durasi target.
//!
//! Prinsipnya: setelah unit jadi, seluruh pengulangan memakai stream copy.
//! Biayanya didominasi kecepatan tulis disk, bukan CPU: 1 jam video 1080p = 104 dtk,
//! RAM puncak 29 MB.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::audio::{self, AudioUnit};
use crate::error::{Error, Result};
use crate::ffmpeg::{Ffmpeg, VideoEncoder};
use crate::probe::{self, MediaInfo};
use crate::run::Ctx;
use crate::video::{self, VideoUnit};

/// Apa yang dilakukan pada audio asli video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioPolicy {
    /// Buang suara video, pakai audio user. Perilaku default.
    #[default]
    Replace,
    /// Pertahankan suara video, abaikan audio user.
    Keep,
    /// Campur keduanya: audio user diturunkan saat suara video terdengar.
    Duck,
}

/// Pilihan encoder video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EncoderChoice {
    /// libx264. Sambungan paling halus, dipakai default.
    #[default]
    Quality,
    /// h264_qsv. Lebih cepat, tapi sambungan loop lebih kasar (terukur 1,73 vs 1,38).
    Hardware,
    /// Pilih otomatis.
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOptions {
    pub video: String,
    pub audio: Option<String>,
    pub output: String,
    /// Durasi target dalam detik.
    pub duration_s: f64,
    /// Panjang tiap pengulangan video, dalam detik.
    pub video_loop_s: f64,
    /// Fade-out lembut di akhir audio, dalam detik.
    pub audio_fade_out_s: f64,
    /// Durasi crossfade video, detik.
    pub video_xfade_s: f64,
    /// Durasi crossfade audio, detik.
    pub audio_xfade_s: f64,
    pub policy: AudioPolicy,
    pub encoder: EncoderChoice,
    pub bitrate_video: String,
    pub bitrate_audio: String,
    /// Normalisasi loudness audio user ke LUFS ini. `None` = jangan sentuh.
    pub target_lufs: Option<f64>,
    /// Ambang dBFS di bawah mana audio video dianggap senyap.
    pub silence_dbfs: f64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            video: String::new(),
            audio: None,
            output: "output.mp4".into(),
            duration_s: 3600.0,
            video_loop_s: 10.0,
            audio_fade_out_s: 3.0,
            video_xfade_s: 2.0,
            audio_xfade_s: 2.0,
            policy: AudioPolicy::Replace,
            encoder: EncoderChoice::Quality,
            bitrate_video: "4M".into(),
            bitrate_audio: "192k".into(),
            target_lufs: Some(-14.0),
            silence_dbfs: -50.0,
        }
    }
}

/// Laporan hasil render, dipakai CLI dan UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderReport {
    pub output: String,
    pub duration_video_s: f64,
    pub duration_audio_s: Option<f64>,
    pub av_delta_s: Option<f64>,
    pub size_bytes: u64,
    pub video_unit_frames: u64,
    pub video_unit_s: f64,
    pub video_repeats: u64,
    pub audio_unit_s: Option<f64>,
    pub audio_repeats: Option<u64>,
    pub video_has_sound: bool,
    pub policy: AudioPolicy,
    pub policy_note: String,
    pub elapsed_s: f64,
}

/// Jalankan seluruh pipeline.
///
/// `ffmpeg_dir` menunjuk folder berisi ffmpeg.exe; kalau `None`, lokasi dicari otomatis
/// lewat konfigurasi, variabel lingkungan, folder aplikasi, lalu PATH.
pub fn render(ctx: &Ctx, opts: &RenderOptions, ffmpeg_dir: Option<&str>) -> Result<RenderReport> {
    let started = std::time::Instant::now();
    let ff = match crate::config::resolve_ffmpeg_dir(ffmpeg_dir) {
        Some(dir) => Ffmpeg::from_dir(&dir)?,
        None => Ffmpeg::discover()?,
    };
    let encoder = match opts.encoder {
        EncoderChoice::Quality => VideoEncoder::X264,
        EncoderChoice::Hardware => VideoEncoder::Qsv,
        EncoderChoice::Auto => {
            // Default: libx264. Lihat catatan di Ffmpeg::available_video_encoder.
            VideoEncoder::X264
        }
    };
    if encoder == VideoEncoder::Qsv && !ff.has_encoder("h264_qsv") {
        return Err(Error::Invalid(
            "encoder hardware h264_qsv tidak tersedia di build ffmpeg ini".into(),
        ));
    }
    ctx.step(&format!(
        "ffmpeg: {} (encoder video: {})",
        ff.ffmpeg.display(),
        encoder.name()
    ));

    let video_info = probe::probe(&ff, &opts.video)?;
    if !video_info.has_video {
        return Err(Error::Invalid(format!(
            "{} tidak punya stream video",
            opts.video
        )));
    }

    // Deteksi suara di video: dipakai untuk memutuskan kebijakan audio.
    let video_peak = if video_info.has_audio {
        probe::audio_peak_dbfs(&ff, &opts.video).unwrap_or(f64::NEG_INFINITY)
    } else {
        f64::NEG_INFINITY
    };
    let video_has_sound = video_peak > opts.silence_dbfs;
    ctx.step(&format!(
        "video: {:.2} dtk, {}x{}, {:.3} fps, audio asli: {}",
        video_info.duration_s,
        video_info.width,
        video_info.height,
        video_info.fps,
        if !video_info.has_audio {
            "tidak ada".to_string()
        } else if video_has_sound {
            format!("ada (puncak {video_peak:.1} dBFS), akan dimatikan")
        } else {
            format!("senyap (puncak {video_peak:.1} dBFS)")
        }
    ));

    let audio_info = match &opts.audio {
        Some(path) => {
            let info = probe::probe(&ff, path)?;
            if !info.has_audio {
                return Err(Error::Invalid(format!("{path} tidak punya stream audio")));
            }
            ctx.step(&format!(
                "audio: {:.2} dtk, {} Hz, {} kanal",
                info.duration_s,
                info.sample_rate.unwrap_or(44100),
                info.channels.unwrap_or(2)
            ));
            Some(info)
        }
        None => None,
    };

    // Kalau audio user tidak ada, suara video dipertahankan apa pun kebijakannya.
    let (policy, policy_note) = if audio_info.is_none() {
        if video_info.has_audio {
            (
                AudioPolicy::Keep,
                "audio user tidak diberikan, suara video dipertahankan".to_string(),
            )
        } else {
            (
                AudioPolicy::Replace,
                "tidak ada audio sama sekali, hasilnya video bisu".to_string(),
            )
        }
    } else if !video_info.has_audio || !video_has_sound {
        (
            AudioPolicy::Replace,
            "video tanpa suara terdengar, audio user dipakai langsung".to_string(),
        )
    } else {
        match opts.policy {
            AudioPolicy::Keep => (
                AudioPolicy::Keep,
                "suara video dipertahankan sesuai pilihan".to_string(),
            ),
            AudioPolicy::Duck => (
                AudioPolicy::Duck,
                "suara video dan audio user dicampur, audio user diturunkan saat video bersuara"
                    .to_string(),
            ),
            AudioPolicy::Replace => (
                AudioPolicy::Replace,
                "suara video dimatikan, diganti audio user".to_string(),
            ),
        }
    };
    ctx.step(&format!("kebijakan audio: {policy_note}"));

    ctx.check_cancel()?;
    let unit = video::build_unit(
        ctx,
        &ff,
        &video_info,
        ctx.work_dir(),
        opts.video_loop_s,
        opts.video_xfade_s,
        encoder,
        &opts.bitrate_video,
    )?;
    let repeats = repeat_count(opts.duration_s, unit.duration_s);
    ctx.step(&format!(
        "unit video: {:.3} dtk, diulang {repeats} kali untuk {:.1} dtk",
        unit.duration_s, opts.duration_s
    ));

    ctx.check_cancel()?;
    let looped_video = ctx.work_file("loop_video.mp4");
    loop_video(ctx, &ff, &unit, opts.duration_s, &looped_video)?;

    // Audio: lagu diputar utuh. Kalau lebih pendek dari video, diulang; kalau lebih panjang,
    // dipotong dengan fade-out lembut. Panjang loop video tidak memengaruhi audio.
    let mut audio_unit: Option<AudioUnit> = None;
    let mut audio_repeats: Option<u64> = None;
    let mut clean_audio: Option<String> = None;

    match &audio_info {
        Some(info) if policy != AudioPolicy::Keep => {
            ctx.check_cancel()?;
            let plan = audio::plan(
                info.duration_s,
                opts.duration_s,
                opts.audio_xfade_s,
                opts.audio_fade_out_s,
            )?;
            ctx.step(&format!("rencana audio: {}", plan.describe()));

            let sample_rate = info.sample_rate.unwrap_or(44100);
            let channels = info.channels.unwrap_or(2);

            let mut unit_audio = audio::build_unit(
                ctx,
                &ff,
                &info.path,
                info.duration_s,
                ctx.work_dir(),
                plan.crossfade_s,
                sample_rate,
                channels,
            )?;
            if let Some(lufs) = opts.target_lufs {
                ctx.check_cancel()?;
                unit_audio =
                    audio::normalize_unit(ctx, &ff, &unit_audio, ctx.work_dir(), lufs, -1.0)?;
            }
            ctx.check_cancel()?;
            let encoded =
                audio::encode_unit(ctx, &ff, &unit_audio, ctx.work_dir(), &opts.bitrate_audio)?;

            ctx.check_cancel()?;
            let final_audio = audio::assemble(
                ctx,
                &ff,
                &unit_audio,
                &encoded,
                ctx.work_dir(),
                opts.duration_s,
                plan.fade_out_s,
                &opts.bitrate_audio,
            )?;

            audio_repeats = Some(plan.repeats);
            audio_unit = Some(unit_audio);
            clean_audio = Some(final_audio);
        }
        _ => {}
    }

    ctx.check_cancel()?;
    let extras = ExtraAudio {
        policy,
        user_audio: clean_audio,
    };
    let final_path = Path::new(&opts.output);
    if let Some(parent) = final_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    mux(
        ctx,
        &ff,
        &video_info,
        &looped_video,
        &extras,
        &opts.output,
        opts.duration_s,
    )?;

    let out_info = probe::probe(&ff, &opts.output)?;
    let size = std::fs::metadata(&opts.output)?.len();
    let duration_audio = out_info.has_audio.then_some(out_info.duration_s);
    let av_delta = duration_audio.map(|a| (out_info.duration_s - a).abs());

    // Hasil sudah utuh dan sudah diukur di atas, jadi folder kerjanya tidak dibutuhkan lagi.
    // Kalau langkah mana pun sebelum ini gagal, baris ini tidak tercapai dan isi folder
    // kerja tetap ada untuk diperiksa.
    crate::run::bersihkan_kerja(ctx);

    Ok(RenderReport {
        output: opts.output.clone(),
        duration_video_s: unit.duration_s * repeats as f64,
        duration_audio_s: duration_audio,
        av_delta_s: av_delta,
        size_bytes: size,
        video_unit_frames: unit.frames,
        video_unit_s: unit.duration_s,
        video_repeats: repeats,
        audio_unit_s: audio_unit.as_ref().map(|a| a.duration_s),
        audio_repeats,
        video_has_sound,
        policy,
        policy_note,
        elapsed_s: started.elapsed().as_secs_f64(),
    })
}

struct ExtraAudio {
    policy: AudioPolicy,
    user_audio: Option<String>,
}

/// Ulangi unit video dengan stream copy sampai durasi target.
fn loop_video(ctx: &Ctx, ff: &Ffmpeg, unit: &VideoUnit, duration_s: f64, out: &Path) -> Result<()> {
    let repeats = repeat_count(duration_s, unit.duration_s);
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-stream_loop".into(),
        (repeats - 1).to_string(),
        "-i".into(),
        unit.path.clone(),
        "-c".into(),
        "copy".into(),
        "-t".into(),
        format!("{duration_s}"),
        out.display().to_string(),
    ];
    ctx.step(&format!(
        "mengulang video: {repeats} kali x {:.3} dtk, stream copy (biaya didominasi tulis disk)",
        unit.duration_s
    ));
    ff.run(&args, "loop video").map(|_| ())
}

/// Gabungkan video hasil loop dengan audio sesuai kebijakan.
fn mux(
    ctx: &Ctx,
    ff: &Ffmpeg,
    video_info: &MediaInfo,
    looped_video: &Path,
    extras: &ExtraAudio,
    output: &str,
    duration_s: f64,
) -> Result<()> {
    let video_arg = looped_video.display().to_string();
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        video_arg.clone(),
    ];

    match (&extras.user_audio, extras.policy) {
        (Some(user_audio), AudioPolicy::Replace) => {
            ctx.step("menggabungkan video hasil loop dengan audio user (tanpa encode ulang video)");
            args.extend([
                "-i".into(),
                user_audio.clone(),
                "-map".into(),
                "0:v".into(),
                "-map".into(),
                "1:a".into(),
                "-c:v".into(),
                "copy".into(),
                "-c:a".into(),
                "copy".into(),
                "-shortest".into(),
            ]);
        }
        (Some(user_audio), AudioPolicy::Duck) => {
            ctx.step("mencampur suara video dengan audio user, audio user diturunkan saat video bersuara");
            args.extend([
                "-i".into(),
                user_audio.clone(),
                "-filter_complex".into(),
                "[0:a]volume=0.9[v];[1:a]volume=0.8,sidechaincompress=threshold=0.05:ratio=8:attack=20:release=400[u];[v][u]amix=inputs=2:duration=shortest[a]".into(),
                "-map".into(),
                "0:v".into(),
                "-map".into(),
                "[a]".into(),
                "-c:v".into(),
                "copy".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "192k".into(),
            ]);
        }
        _ => {
            // Keep, atau tidak ada audio user, atau video memang bisu.
            if video_info.has_audio {
                ctx.step("mempertahankan suara asli video");
                args.extend([
                    "-map".into(),
                    "0:v".into(),
                    "-map".into(),
                    "0:a?".into(),
                    "-c:v".into(),
                    "copy".into(),
                    "-c:a".into(),
                    "copy".into(),
                ]);
            } else {
                ctx.step("hasil tanpa audio (video bisu dan tidak ada audio user)");
                args.extend([
                    "-map".into(),
                    "0:v".into(),
                    "-an".into(),
                    "-c:v".into(),
                    "copy".into(),
                ]);
            }
        }
    }

    args.extend([
        "-t".into(),
        format!("{duration_s}"),
        "-movflags".into(),
        "+faststart".into(),
        output.into(),
    ]);
    ff.run(&args, "mux").map(|_| ())
}

/// Berapa kali unit harus diulang untuk mencapai (atau melewati) durasi target.
fn repeat_count(duration_s: f64, unit_s: f64) -> u64 {
    if unit_s <= 0.0 {
        return 1;
    }
    ((duration_s / unit_s).ceil() as u64).max(1)
}
