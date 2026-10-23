//! LofiLoop CLI — jalur utama untuk memakai dan menguji engine tanpa UI.
//!
//! Contoh:
//!   lofiloop-cli --video klip.mp4 --audio lagu.mp3 --duration 1h -o hasil.mp4
//!   lofiloop-cli --video klip.mp4 --duration 30m --loop 10 --video-xfade 1.5 -o hasil.mp4
//!   lofiloop-cli --video klip.mp4 --audio lagu.mp3 --duration 3h --policy duck -o hasil.mp4
//!   lofiloop-cli --check-deps

use std::process::ExitCode;

use lofiloop_engine::merge::{self, MergeOptions};
use lofiloop_engine::render::{AudioPolicy, EncoderChoice, RenderOptions};
use lofiloop_engine::{render, Ctx, Ffmpeg};

const USAGE: &str = "\
LofiLoop — loop video dan audio mulus, dengan auto-mix.

Pakai:
  lofiloop-cli --video <berkas> [--audio <berkas>] [opsi] -o <keluaran.mp4>

Wajib:
  --video <berkas>        klip video sumber yang akan diulang
  -o, --output <berkas>   berkas hasil (mp4)

Audio:
  --audio <berkas>        audio yang dipakai (mp3/wav/m4a/flac/...)
  --policy <mode>         replace (default) | keep | duck
                          replace: suara video dibuang, pakai audio user
                          keep   : pertahankan suara video, audio user diabaikan
                          duck   : campur keduanya, audio user turun saat video bersuara
  --lufs <angka>          normalisasi loudness audio user (default -14). 0 = jangan sentuh
  --audio-fade <dtk>      fade-out lembut di akhir audio (default 3)
  --audio-xfade <dtk>     crossfade di sambungan antar-putaran lagu (default 2)

Durasi dan loop:
  --duration <durasi>     durasi hasil: 30m, 1h, 3h, 45s, atau detik polos (default 1h)
  --loop <dtk>            panjang unit loop video (default 10)
  --video-xfade <dtk>     crossfade video (default 1)

Kualitas:
  --encoder <mode>        quality (default, libx264, sambungan paling halus)
                          | hardware (h264_qsv, lebih cepat tapi sambungan lebih kasar)
  --bitrate <nilai>       bitrate video (default 4M)
  --height <piksel>       tinggi hasil, lebar mengikuti rasio aspek sumber (mis. 720, 1080).
                          Semakin kecil, semakin hemat ruang. Kosong = ikut sumber.
  --fps <angka>           frame per detik hasil (mis. 24, 30). Kosong = ikut sumber.
  --audio-bitrate <nilai> bitrate audio (default 192k)

Lain-lain:
  --work <folder>         folder kerja untuk berkas antara (default: <keluaran>.work,
                          dihapus sendiri setelah pekerjaan selesai)
  --check-deps            periksa ffmpeg dan encoder, lalu keluar
  -h, --help              tampilkan bantuan ini

Gabung banyak lagu menjadi satu (tanpa video):
  lofiloop-cli --merge lagu1.mp3 lagu2.mp3 lagu3.mp3 -o gabungan.m4a
  lofiloop-cli --merge *.mp3 --merge-xfade 4 --bitrate-audio 256k -o gabungan.m4a

  --merge <berkas...>     daftar lagu yang digabung, sesuai urutan
  --merge-xfade <dtk>     transisi mulus antar lagu (default 3, 0 = potong langsung)
  --merge-no-normalize    jangan samakan volume antar lagu
";

/// Jalankan mode gabung lagu. Dipisah supaya alur render tetap sederhana.
/// Jalankan mode gabung lagu. Dipisah supaya alur render tetap sederhana.
fn run_merge(args: &[String], merge_at: usize) -> std::result::Result<(), String> {
    let mut opts = MergeOptions::default();
    let mut work_dir: Option<String> = None;
    let mut tracks: Vec<String> = Vec::new();

    // Kumpulkan posisi nilai yang bukan lagu, supaya berkas lagu tidak tertukar dengan opsi.
    let mut skip: Vec<usize> = vec![merge_at];
    let mut i = merge_at + 1;
    while i < args.len() {
        let key = args[i].as_str();
        match key {
            "--merge-xfade" | "--bitrate-audio" | "--work" | "-o" | "--output" => {
                let value = args
                    .get(i + 1)
                    .cloned()
                    .ok_or_else(|| format!("opsi {key} butuh nilai"))?;
                match key {
                    "--merge-xfade" => opts.crossfade_s = parse_num(&value, "--merge-xfade")?,
                    "--bitrate-audio" => opts.bitrate = value,
                    "--work" => work_dir = Some(value),
                    _ => opts.output = value,
                }
                skip.push(i);
                skip.push(i + 1);
                i += 2;
            }
            "--durations" => {
                // Durasi tiap lagu, dipisah spasi atau koma. Menghindari pembacaan ulang
                // semua berkas yang memakan waktu tanpa bisa dilaporkan.
                let raw = args.get(i + 1).cloned().unwrap_or_default();
                let list: Vec<f64> = raw
                    .split([' ', ','])
                    .filter(|v| !v.trim().is_empty())
                    .filter_map(|v| v.trim().parse::<f64>().ok())
                    .collect();
                if !list.is_empty() {
                    opts.durations = list;
                }
                skip.push(i);
                skip.push(i + 1);
                i += 2;
            }
            "--merge-no-normalize" => {
                opts.normalize = false;
                skip.push(i);
                i += 1;
            }
            other if other.starts_with("--") => {
                return Err(format!("opsi '{other}' tidak dikenal di mode --merge"));
            }
            _ => i += 1,
        }
    }

    for (index, value) in args.iter().enumerate() {
        if !skip.contains(&index) {
            tracks.push(value.clone());
        }
    }

    if tracks.len() < 2 {
        return Err(format!(
            "mode --merge butuh minimal dua berkas lagu, hanya {} yang terbaca. Pakai --help.",
            tracks.len()
        ));
    }
    if opts.crossfade_s < 0.0 {
        return Err("--merge-xfade tidak boleh negatif".into());
    }
    opts.tracks = tracks;

    let work = work_dir.unwrap_or_else(|| format!("{}.work", opts.output));
    let ctx = Ctx::for_cli(&work).map_err(|e| e.to_string())?;
    let ff = Ffmpeg::discover().map_err(|e| e.to_string())?;

    println!(
        "[lofiloop] menggabungkan {} lagu, transisi {:.1} dtk, hasil: {}",
        opts.tracks.len(),
        opts.crossfade_s,
        opts.output
    );

    let report = merge::merge(&ctx, &ff, &opts).map_err(|e| e.to_string())?;

    println!();
    println!("== selesai ==");
    println!("berkas        : {}", report.output);
    println!("jumlah lagu   : {}", report.track_count);
    println!(
        "ukuran        : {:.0} MB",
        report.size_bytes as f64 / 1_048_576.0
    );
    println!(
        "durasi hasil  : {:.1} menit (dari {:.1} menit materi)",
        report.duration_s / 60.0,
        report.source_total_s / 60.0
    );
    if report.overlap_s > 0.0 {
        println!(
            "tumpang tindih: {:.1} dtk hilang karena {} transisi {:.1} dtk (memang disengaja)",
            report.overlap_s,
            report.track_count - 1,
            report.crossfade_s
        );
    }
    println!(
        "format        : {} Hz, {} kanal",
        report.sample_rate, report.channels
    );
    println!(
        "normalisasi   : {}",
        if report.normalized { "ya" } else { "tidak" }
    );
    println!("waktu proses  : {:.1} dtk", report.elapsed_s);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> std::result::Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return Ok(());
    }

    // Mode gabung lagu: dipilih kalau --merge diberikan.
    if let Some(position) = args.iter().position(|a| a == "--merge") {
        return run_merge(&args, position);
    }

    let mut opts = RenderOptions::default();
    let mut work_dir: Option<String> = None;
    let mut duration_arg = mut_pair(&mut opts);

    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        let value = |i: usize| -> std::result::Result<String, String> {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("opsi {key} butuh nilai"))
        };
        match key {
            "--check-deps" => return check_deps(),
            "--video" => opts.video = value(i)?,
            "--audio" => opts.audio = Some(value(i)?),
            "-o" | "--output" => opts.output = value(i)?,
            "--duration" => duration_arg.duration = Some(value(i)?),
            "--loop" => opts.video_loop_s = parse_num(&value(i)?, "--loop")?,
            "--audio-fade" => opts.audio_fade_out_s = parse_num(&value(i)?, "--audio-fade")?,
            "--video-xfade" => opts.video_xfade_s = parse_num(&value(i)?, "--video-xfade")?,
            "--audio-xfade" => opts.audio_xfade_s = parse_num(&value(i)?, "--audio-xfade")?,
            "--policy" => {
                opts.policy = match value(i)?.as_str() {
                    "replace" => AudioPolicy::Replace,
                    "keep" => AudioPolicy::Keep,
                    "duck" => AudioPolicy::Duck,
                    other => return Err(format!("--policy '{other}' tidak dikenal")),
                }
            }
            "--lufs" => {
                let v = parse_num(&value(i)?, "--lufs")?;
                opts.target_lufs = if v == 0.0 { None } else { Some(v) };
            }
            "--encoder" => {
                opts.encoder = match value(i)?.as_str() {
                    "quality" => EncoderChoice::Quality,
                    "hardware" => EncoderChoice::Hardware,
                    "auto" => EncoderChoice::Auto,
                    other => return Err(format!("--encoder '{other}' tidak dikenal")),
                }
            }
            "--bitrate" => opts.bitrate_video = value(i)?,
            "--audio-bitrate" => opts.bitrate_audio = value(i)?,
            "--height" => {
                let h = parse_num(&value(i)?, "--height")?;
                if h < 16.0 || h > 4320.0 {
                    return Err("--height di luar rentang wajar (16 sampai 4320)".into());
                }
                opts.target.height = Some(h as u32);
            }
            "--fps" => {
                let f = parse_num(&value(i)?, "--fps")?;
                if f < 1.0 || f > 240.0 {
                    return Err("--fps di luar rentang wajar (1 sampai 240)".into());
                }
                opts.target.fps = Some(f);
            }
            "--work" => work_dir = Some(value(i)?),
            other => return Err(format!("opsi '{other}' tidak dikenal. Pakai --help.")),
        }
        i += if key == "--check-deps" { 1 } else { 2 };
    }

    if opts.video.is_empty() || opts.output.is_empty() {
        return Err("--video dan -o wajib diisi. Pakai --help.".into());
    }
    opts.duration_s = parse_duration(duration_arg.duration.as_deref().unwrap_or("1h"))?;

    if let Some(lufs) = opts.target_lufs {
        if !(-40.0..=-5.0).contains(&lufs) {
            return Err(format!(
                "--lufs {lufs} di luar rentang wajar (-40 sampai -5)"
            ));
        }
    }
    if opts.video_xfade_s <= 0.0 || opts.audio_xfade_s <= 0.0 {
        return Err("crossfade harus lebih dari 0 detik".into());
    }

    let work = work_dir.unwrap_or_else(|| format!("{}.work", opts.output));
    let ctx = Ctx::for_cli(&work).map_err(|e| e.to_string())?;

    println!(
        "[lofiloop] target: {} ({:.0} dtk), loop video {:.1} dtk",
        opts.output, opts.duration_s, opts.video_loop_s
    );

    let ffmpeg_dir = std::env::var("LOFILOOP_FFMPEG_DIR").ok();
    let report = render(&ctx, &opts, ffmpeg_dir.as_deref()).map_err(|e| e.to_string())?;

    println!();
    println!("== selesai ==");
    println!("berkas          : {}", report.output);
    println!(
        "ukuran          : {:.0} MB",
        report.size_bytes as f64 / 1_048_576.0
    );
    println!(
        "durasi video    : {:.3} dtk ({} x unit {:.3} dtk)",
        report.duration_video_s, report.video_repeats, report.video_unit_s
    );
    if let Some(a) = report.duration_audio_s {
        println!("durasi audio    : {a:.3} dtk");
    }
    if let Some(d) = report.av_delta_s {
        println!("selisih A/V     : {:.3} dtk", d);
    }
    if let Some(u) = report.audio_unit_s {
        println!(
            "unit audio      : {u:.3} dtk ({} x)",
            report.audio_repeats.unwrap_or(0)
        );
    }
    println!(
        "bentuk keluaran : {}x{} pada {:.3} fps",
        report.output_width, report.output_height, report.output_fps
    );
    println!("kebijakan audio : {}", report.policy_note);
    println!("waktu proses    : {:.1} dtk", report.elapsed_s);
    if ctx.work_dir().exists() {
        println!("folder kerja    : {work} (disisakan karena ada yang gagal dihapus)");
    }

    Ok(())
}

/// Pasangan sementara supaya --duration bisa diurai sekali di akhir.
struct DurationArg {
    duration: Option<String>,
}

fn mut_pair(_opts: &mut RenderOptions) -> DurationArg {
    DurationArg { duration: None }
}

fn parse_num(text: &str, flag: &str) -> std::result::Result<f64, String> {
    text.parse::<f64>()
        .map_err(|_| format!("nilai {flag} harus angka, dapat '{text}'"))
}

/// Terima 30m, 1h, 3h, 45s, 1h30m, atau detik polos.
fn parse_duration(text: &str) -> std::result::Result<f64, String> {
    let text = text.trim();
    if let Ok(plain) = text.parse::<f64>() {
        if plain <= 0.0 {
            return Err(format!("durasi '{text}' harus lebih dari 0"));
        }
        return Ok(plain);
    }
    let mut total = 0.0;
    let mut num = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            num.push(ch);
            continue;
        }
        let value: f64 = num
            .parse()
            .map_err(|_| format!("durasi '{text}' tidak terbaca"))?;
        num.clear();
        total += match ch {
            's' | 'S' => value,
            'm' | 'M' => value * 60.0,
            'h' | 'H' => value * 3600.0,
            other => return Err(format!("satuan '{other}' tidak dikenal di durasi '{text}'")),
        };
    }
    if !num.is_empty() {
        return Err(format!(
            "durasi '{text}' harus diakhiri satuan s, m, atau h"
        ));
    }
    if total <= 0.0 {
        return Err(format!("durasi '{text}' harus lebih dari 0"));
    }
    Ok(total)
}

fn check_deps() -> std::result::Result<(), String> {
    let ff = Ffmpeg::discover().map_err(|e| e.to_string())?;
    println!("ffmpeg : {}", ff.ffmpeg.display());
    println!("ffprobe: {}", ff.ffprobe.display());
    let encoders = [("libx264", true), ("aac", true), ("h264_qsv", false)];
    for (name, required) in encoders {
        let ok = ff.has_encoder(name);
        let tag = if ok {
            "OK"
        } else if required {
            "GAGAL"
        } else {
            "tidak ada"
        };
        println!("encoder {name:<10}: {tag}");
    }
    if !ff.has_encoder("libx264") && !ff.has_encoder("h264_qsv") {
        return Err("tidak ada encoder H.264 yang tersedia".into());
    }
    if !ff.has_encoder("aac") {
        return Err("encoder aac tidak tersedia".into());
    }
    println!(
        "encoder video yang tersedia: {}",
        ff.available_video_encoder().name()
    );
    println!(
        "encoder video yang dipakai  : {} (default; pakai --encoder hardware untuk QSV)",
        lofiloop_engine::ffmpeg::VideoEncoder::X264.name()
    );
    Ok(())
}
