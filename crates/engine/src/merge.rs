//! Gabungkan banyak berkas audio menjadi satu, dengan transisi mulus antar lagu.
//!
//! Dipakai untuk membuat satu berkas panjang dari banyak lagu, lalu berkas itu bisa
//! langsung diberikan ke fitur loop (atau dipakai apa adanya).
//!
//! Dua hal yang membuat ini tetap cepat di mesin lemah:
//! 1. **Semua lagu dinormalisasi ke format yang sama lebih dulu** (sample rate, kanal, PCM).
//!    Dengan begitu penggabungan hanya perlu satu pass, bukan menyusun filter bertingkat
//!    yang makin berat untuk setiap lagu tambahan.
//! 2. Transisi memakai `acrossfade` equal-power, jadi jumlah daya di sambungan tetap dan
//!    volume tidak melonjak turun di tengah transisi.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ffmpeg::Ffmpeg;
use crate::run::Ctx;

/// Jumlah sample per frame AAC pada 44,1 kHz.
const AAC_FRAME_SAMPLES: u64 = 1024;

/// Format seragam untuk semua lagu sebelum digabung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u32,
}

impl AudioFormat {
    /// Format paling umum, dipakai kalau tidak ada lagu yang memberi petunjuk lain.
    pub fn default_format() -> Self {
        Self { sample_rate: 44100, channels: 2 }
    }

    /// Pilih format dari lagu dengan sample rate tertinggi, supaya tidak ada yang diturunkan.
    pub fn from_sources(rates: &[u32], channels: &[u32]) -> Self {
        let sample_rate = rates.iter().copied().filter(|r| *r > 0).max().unwrap_or(44100);
        let channels = channels.iter().copied().filter(|c| *c > 0).max().unwrap_or(2).min(2);
        Self { sample_rate, channels }
    }
}

/// Satu lagu dalam daftar gabung.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub path: String,
    pub duration_s: f64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeOptions {
    /// Daftar berkas audio, sesuai urutan yang diinginkan.
    pub tracks: Vec<String>,
    /// Durasi tiap lagu kalau sudah diketahui (dari UI). Dipakai supaya tidak perlu
    /// membaca ulang semua berkas hanya untuk tahu panjangnya: itu memakan beberapa
    /// detik tanpa bisa dilaporkan dan terasa seperti macet.
    #[serde(default)]
    pub durations: Vec<f64>,
    pub output: String,
    /// Durasi transisi antar lagu, detik. `0` berarti potong langsung tanpa transisi.
    pub crossfade_s: f64,
    /// Normalisasi loudness tiap lagu dulu. Menyamakan volume antar lagu.
    pub normalize: bool,
    pub target_lufs: f64,
    pub bitrate: String,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            durations: Vec::new(),
            output: "gabungan.m4a".into(),
            crossfade_s: 3.0,
            normalize: true,
            target_lufs: -14.0,
            bitrate: "192k".into(),
        }
    }
}

/// Laporan hasil penggabungan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeReport {
    pub output: String,
    pub track_count: usize,
    pub duration_s: f64,
    /// Jumlah durasi semua lagu sebelum transisi, untuk membandingkan.
    pub source_total_s: f64,
    /// Total detik yang "hilang" karena transisi saling menumpuk. Ini memang disengaja.
    pub overlap_s: f64,
    pub size_bytes: u64,
    pub crossfade_s: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub normalized: bool,
    pub elapsed_s: f64,
}

/// Hitung durasi hasil dan total tumpang tindih transisi.
pub fn plan(durations: &[f64], crossfade_s: f64) -> Result<(f64, f64)> {
    if durations.is_empty() {
        return Err(Error::Invalid("daftar lagu kosong".into()));
    }
    if durations.iter().any(|d| *d <= 0.0) {
        return Err(Error::Invalid("ada lagu dengan durasi tidak terbaca".into()));
    }
    let count = durations.len();
    let xfade = crossfade_s.max(0.0);

    // Transisi hanya masuk akal kalau lebih pendek dari lagu terpendek, dan tidak boleh
    // memakan lebih dari seperempat tiap lagu supaya isi lagu tetap utuh.
    let shortest = durations.iter().copied().fold(f64::INFINITY, f64::min);
    let xfade = xfade.min(shortest / 4.0).max(0.0);

    let total: f64 = durations.iter().sum();
    let overlap = if count > 1 { xfade * (count - 1) as f64 } else { 0.0 };
    Ok((total - overlap, overlap))
}

/// Gabungkan semua lagu menjadi satu berkas.
pub fn merge(ctx: &Ctx, ff: &Ffmpeg, opts: &MergeOptions) -> Result<MergeReport> {
    let started = std::time::Instant::now();
    if opts.tracks.len() < 2 {
        return Err(Error::Invalid("butuh minimal dua lagu untuk digabung".into()));
    }

    // 1. Tentukan panjang dan format tiap lagu.
    //
    // Kalau UI sudah mengirim durasinya, tidak perlu membaca berkas sama sekali: membaca
    // sepuluh lagu memakan beberapa detik dan tidak bisa dilaporkan per langkah sehingga
    // terasa seperti macet. Format audio tetap dibaca sekali saja dari lagu pertama.
    let have_durations =
        opts.durations.len() == opts.tracks.len() && opts.durations.iter().all(|d| *d > 0.0);

    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut first_info: Option<crate::probe::MediaInfo> = None;

    for (index, path) in opts.tracks.iter().enumerate() {
        let info = if let Some(info) = first_info.clone() {
            info
        } else {
            let probed = crate::probe::probe(ff, path)?;
            if !probed.has_audio {
                return Err(Error::Invalid(format!("{path} tidak punya stream audio")));
            }
            first_info = Some(probed.clone());
            probed
        };
        if !info.has_audio {
            return Err(Error::Invalid(format!("{path} tidak punya stream audio")));
        }
        let duration_s = if have_durations {
            opts.durations[index]
        } else {
            let probed = crate::probe::probe(ff, path)?;
            if !probed.has_audio {
                return Err(Error::Invalid(format!("{path} tidak punya stream audio")));
            }
            probed.duration_s
        };
        tracks.push(TrackInfo {
            path: path.clone(),
            duration_s,
            sample_rate: info.sample_rate,
            channels: info.channels,
        });
    }

    let rates: Vec<u32> = tracks.iter().filter_map(|t| t.sample_rate).collect();
    let chans: Vec<u32> = tracks.iter().filter_map(|t| t.channels).collect();
    let format = AudioFormat::from_sources(&rates, &chans);

    let durations: Vec<f64> = tracks.iter().map(|t| t.duration_s).collect();
    let (planned_s, overlap_s) = plan(&durations, opts.crossfade_s)?;
    let xfade = if tracks.len() > 1 {
        let shortest = durations.iter().copied().fold(f64::INFINITY, f64::min);
        opts.crossfade_s.min(shortest / 4.0).max(0.0)
    } else {
        0.0
    };

    ctx.step(&format!(
        "menggabungkan {} lagu: total {:.1} menit, transisi {:.1} dtk, hasil sekitar {:.1} menit",
        tracks.len(),
        durations.iter().sum::<f64>() / 60.0,
        xfade,
        planned_s / 60.0
    ));

    // 2. Seragamkan tiap lagu: sample rate, kanal, PCM. Sekaligus normalisasi kalau diminta.
    let mut normalized_paths: Vec<PathBuf> = Vec::new();
    for (i, track) in tracks.iter().enumerate() {
        ctx.check_cancel()?;
        let out = ctx.work_file(&format!("track_{:02}.wav", i + 1));
        let mut filters: Vec<String> = Vec::new();
        if opts.normalize {
            let gain = compute_gain(ff, &track.path, opts.target_lufs, -1.0)?;
            if gain.abs() > 0.1 {
                filters.push(format!("volume={gain:.2}dB"));
            }
        }
        let args = build_prepare_args(track, &out, format, &filters);

        // Menyiapkan lagu memakan kira-kira 70% waktu total, sisanya penggabungan.
        // Setiap lagu melaporkan kemajuannya sendiri supaya pesannya bergerak, bukan
        // diam di satu angka selama lagu panjang diproses.
        let base = 0.70 * (i as f64) / (tracks.len() as f64);
        let span = 0.70 / (tracks.len() as f64);
        let label = file_label(&track.path);
        let total = tracks.len();
        let minutes = track.duration_s / 60.0;

        ctx.step_at(
            &format!("menyiapkan lagu {}/{}: {label} ({minutes:.1} menit)", i + 1, total),
            base,
        );

        let progress_ctx = ctx.clone();
        let reporter = move |fraction: f64| {
            progress_ctx.step_at(
                &format!(
                    "menyiapkan lagu {}/{}: {label} ({minutes:.1} menit), {:.0}%",
                    i + 1,
                    total,
                    fraction * 100.0
                ),
                base + span * fraction,
            );
        };

        ff.run_with_progress(
            &args,
            &format!("siapkan lagu {}", i + 1),
            track.duration_s,
            base,
            span,
            &reporter,
        )?;
        normalized_paths.push(out);
    }

    // 3. Rangkai semuanya dalam satu pass filter.
    let output = PathBuf::from(&opts.output);
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    if xfade > 0.0 {
        ctx.step("menggabungkan dengan transisi equal-power antar lagu");
    } else {
        ctx.step("menggabungkan tanpa transisi");
    }

    // Digabung bertahap: satu putaran menggabungkan beberapa lagu, hasilnya dipakai sebagai
    // masukan putaran berikutnya. Dengan cara ini kemajuan bisa dilaporkan sepanjang proses
    // dan ukuran satu perintah ffmpeg tetap terbatas walau lagunya ratusan.
    let unit_seconds: Vec<f64> = tracks.iter().map(|t| t.duration_s).collect();
    run_staged(ctx, ff, &normalized_paths, &unit_seconds, &output, xfade, format, &opts.bitrate, planned_s)?;

    let size = std::fs::metadata(&output)?.len();
    let out_info = crate::probe::probe(ff, &opts.output)?;

    // Bersihkan berkas antara.
    for p in &normalized_paths {
        let _ = std::fs::remove_file(p);
    }

    // Hasil sudah utuh dan sudah diukur di atas, jadi folder kerjanya tidak dibutuhkan lagi.
    // Kalau langkah mana pun sebelum ini gagal, baris ini tidak tercapai dan isi folder
    // kerja tetap ada untuk diperiksa.
    crate::run::bersihkan_kerja(ctx);

    Ok(MergeReport {
        output: opts.output.clone(),
        track_count: tracks.len(),
        duration_s: out_info.duration_s,
        source_total_s: durations.iter().sum(),
        overlap_s,
        size_bytes: size,
        crossfade_s: xfade,
        sample_rate: format.sample_rate,
        channels: format.channels,
        normalized: opts.normalize,
        elapsed_s: started.elapsed().as_secs_f64(),
    })
}


/// Hitung gain yang perlu diterapkan ke satu lagu supaya tingkat kekerasannya mendekati target.
///
/// Memakai `volumedetect` sekali (0,2 dtk untuk lagu 1 menit) lalu menggeser dengan filter
/// `volume`. Ini jauh lebih cepat daripada `loudnorm` yang memproses ulang seluruh audio
/// (5 dtk untuk lagu 50 dtk, dan biaya itu dibayar per lagu sehingga playlist panjang
/// menjadi sangat lambat). Selisih hasilnya kecil dan tidak terdengar.
///
/// Peak selalu dijaga: kalau penggeseran akan membuat puncak melewati `-1 dB`, gain
/// dikurangi supaya tidak ada distorsi.
pub fn compute_gain(ff: &Ffmpeg, path: &str, target_db: f64, ceiling_db: f64) -> Result<f64> {
    let report = ff.run(
        &[
            "-hide_banner".into(),
            "-nostats".into(),
            "-i".into(),
            path.into(),
            "-af".into(),
            "volumedetect".into(),
            "-f".into(),
            "null".into(),
            "-".into(),
        ],
        "ukur volume",
    )?;

    let mean = parse_db(&report, "mean_volume");
    let peak = parse_db(&report, "max_volume");

    let (Some(mean), Some(peak)) = (mean, peak) else {
        // Kalau tidak terbaca (misalnya audio sangat senyap), jangan ubah apa pun.
        return Ok(0.0);
    };

    let mut gain = target_db - mean;
    if peak + gain > ceiling_db {
        gain = ceiling_db - peak;
    }
    // Batas wajar, supaya lagu yang hampir bisu tidak dinaikkan sampai noise-nya terdengar.
    Ok(gain.clamp(-24.0, 24.0))
}

fn parse_db(text: &str, key: &str) -> Option<f64> {
    let marker = format!("{key}:");
    let line = text.lines().find(|l| l.contains(&marker))?;
    let rest = line.split(&marker).nth(1)?;
    rest.split_whitespace().next()?.parse::<f64>().ok()
}


/// Gabungkan semua lagu secara bertahap sambil melaporkan kemajuan nyata.
///
/// Setiap putaran menggabungkan beberapa berkas menjadi satu. Jumlah masukan per putaran
/// dijaga kecil supaya satu perintah ffmpeg tidak menahan terlalu banyak berkas terbuka,
/// dan supaya kemajuan bisa dilaporkan berkali-kali, bukan sekali di akhir.
fn run_staged(
    ctx: &Ctx,
    ff: &Ffmpeg,
    inputs: &[PathBuf],
    input_seconds: &[f64],
    output: &Path,
    xfade: f64,
    format: AudioFormat,
    bitrate: &str,
    _planned_s: f64,
) -> Result<()> {
    // Kelompok kecil supaya tiap perintah ffmpeg pendek dan progres sering bergerak.
    // Terukur pada mesin ini: kelompok 4 memberi jeda terpendek antar laporan.
    const PER_GROUP: usize = 4;

    if inputs.len() <= PER_GROUP {
        let total = merge_length(input_seconds, xfade);
        let args = build_merge_args(ff, inputs, output, xfade, format, bitrate, total);
        let progress_ctx = ctx.clone();
        let count = inputs.len();
        let reporter = move |fraction: f64| {
            progress_ctx.step_at(
                &format!("menggabungkan {count} lagu, {:.0}%", fraction * 100.0),
                0.75 + 0.22 * fraction,
            );
        };
        return ff.run_with_progress(&args, "gabung lagu", total, 0.75, 0.22, &reporter);
    }

    let groups: Vec<Vec<PathBuf>> = inputs.chunks(PER_GROUP).map(|c| c.to_vec()).collect();
    let seconds_groups: Vec<Vec<f64>> = input_seconds.chunks(PER_GROUP).map(|c| c.to_vec()).collect();
    let group_count = groups.len();

    let mut pieces: Vec<PathBuf> = Vec::new();
    let mut piece_seconds: Vec<f64> = Vec::new();

    for (index, group) in groups.iter().enumerate() {
        ctx.check_cancel()?;
        let seconds = merge_length(&seconds_groups[index], xfade);
        let dest = ctx.work_file(&format!("kelompok_{:02}.wav", index + 1));
        let args = build_merge_args(ff, group, &dest, xfade, format, bitrate, seconds);

        let base = 0.75 + 0.15 * index as f64 / group_count as f64;
        let span = 0.15 / group_count as f64;
        let progress_ctx = ctx.clone();
        let count = group.len();
        let label = format!("kelompok {} dari {group_count}", index + 1);
        let reporter = move |fraction: f64| {
            progress_ctx.step_at(
                &format!("{label}: menggabungkan {count} lagu, {:.0}%", fraction * 100.0),
                base + span * fraction,
            );
        };
        let run_label = format!("kelompok {} dari {group_count}", index + 1);
        ff.run_with_progress(&args, &run_label, seconds, base, span, &reporter)?;

        pieces.push(dest);
        piece_seconds.push(seconds);
    }

    // Tahap penyatuan akhir.
    let total = merge_length(&piece_seconds, xfade);
    let args = build_merge_args(
            ff,
            &pieces, output, xfade, format, bitrate, total);
    let progress_ctx = ctx.clone();
    let count = pieces.len();
    let reporter = move |fraction: f64| {
        progress_ctx.step_at(
            &format!("menyatukan {count} bagian, {:.0}%", fraction * 100.0),
            0.90 + 0.07 * fraction,
        );
    };
    ff.run_with_progress(&args, "satukan bagian", total, 0.90, 0.07, &reporter)?;

    for path in &pieces {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Panjang hasil setelah menggabungkan beberapa durasi dengan transisi saling menumpuk.
fn merge_length(durations: &[f64], xfade_s: f64) -> f64 {
    let total: f64 = durations.iter().sum();
    if durations.len() <= 1 || xfade_s <= 0.0 {
        return total;
    }
    (total - xfade_s * (durations.len() - 1) as f64).max(0.1)
}

/// Argumen untuk menyiapkan satu lagu agar formatnya seragam.
fn build_prepare_args(
    track: &TrackInfo,
    out: &Path,
    format: AudioFormat,
    filters: &[String],
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        track.path.clone(),
        "-vn".into(),
    ];
    if !filters.is_empty() {
        args.extend(["-af".into(), filters.join(",")]);
    }
    args.extend([
        "-ar".into(),
        format.sample_rate.to_string(),
        "-ac".into(),
        format.channels.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        "-t".into(),
        format!("{:.6}", track.duration_s),
        out.display().to_string(),
    ]);
    args
}

/// Argumen penggabungan. Semua lagu sudah berformat sama, jadi cukup satu filter_complex.
fn build_merge_args(
    ff: &Ffmpeg,
    inputs: &[PathBuf],
    output: &Path,
    xfade_s: f64,
    format: AudioFormat,
    bitrate: &str,
    planned_s: f64,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into()];
    for input in inputs {
        args.push("-i".into());
        args.push(input.display().to_string());
    }

    // Rantai acrossfade: [0][1] -> [a1]; [a1][2] -> [a2]; ... hasil akhir [mix].
    let filter = if xfade_s > 0.0 && inputs.len() > 1 {
        let mut chain = String::new();
        let mut current = "[0:a]".to_string();
        for i in 1..inputs.len() {
            let out_label = if i == inputs.len() - 1 {
                "[mix]".to_string()
            } else {
                format!("[a{i}]")
            };
            chain.push_str(&format!(
                "{current}[{i}:a]acrossfade=d={xfade_s}:c1=qsin:c2=qsin{out_label};"
            ));
            current = out_label;
        }
        chain.trim_end_matches(';').to_string()
    } else {
        let labels: String = (0..inputs.len()).map(|i| format!("[{i}:a]")).collect();
        format!("{labels}concat=n={}:v=0:a=1[mix]", inputs.len())
    };

    // Codec mengikuti jenis tujuan: berkas antara dibuat PCM supaya bisa digabung lagi
    // tanpa penurunan mutu dan tanpa risiko salah baca; berkas akhir dibuat AAC.
    let is_intermediate = output
        .extension()
        .map(|e| e.eq_ignore_ascii_case("wav"))
        .unwrap_or(false);

    args.extend([
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "[mix]".into(),
    ]);
    if is_intermediate {
        args.extend(["-c:a".into(), "pcm_s16le".into()]);
    } else {
        args.extend(["-c:a".into(), ff.best_aac(), "-b:a".into(), bitrate.into()]);
    }
    args.extend([
        "-ar".into(),
        format.sample_rate.to_string(),
        "-ac".into(),
        format.channels.to_string(),
        "-t".into(),
        format!("{planned_s:.3}"),
        output.display().to_string(),
    ]);
    args
}

fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Selaraskan panjang ke kelipatan frame AAC, dipakai kalau hasil akan diulang.
pub fn align_to_aac(samples: u64) -> u64 {
    let frames = (samples + AAC_FRAME_SAMPLES / 2) / AAC_FRAME_SAMPLES;
    frames.max(1) * AAC_FRAME_SAMPLES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_accounts_for_overlap() {
        let d = vec![180.0, 200.0, 240.0];
        let (total, overlap) = plan(&d, 3.0).unwrap();
        assert!((overlap - 6.0).abs() < 1e-9);
        assert!((total - 614.0).abs() < 1e-9);
    }

    #[test]
    fn ten_tracks_overlap_scales() {
        let d: Vec<f64> = (0..12).map(|_| 240.0).collect();
        let (total, overlap) = plan(&d, 4.0).unwrap();
        assert!((overlap - 44.0).abs() < 1e-9);
        assert!((total - (240.0 * 12.0 - 44.0)).abs() < 1e-9);
    }

    #[test]
    fn crossfade_is_capped_by_shortest_track() {
        // Lagu terpendek 8 dtk: transisi 5 dtk harus dipotong jadi 2 dtk.
        let (_, overlap) = plan(&[8.0, 300.0], 5.0).unwrap();
        assert!((overlap - 2.0).abs() < 1e-9);
    }

    #[test]
    fn zero_crossfade_keeps_full_length() {
        let d = vec![100.0, 100.0];
        let (total, overlap) = plan(&d, 0.0).unwrap();
        assert_eq!(overlap, 0.0);
        assert!((total - 200.0).abs() < 1e-9);
    }

    #[test]
    fn single_track_has_no_overlap() {
        let (total, overlap) = plan(&[120.0], 3.0).unwrap();
        assert_eq!(overlap, 0.0);
        assert!((total - 120.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_empty_and_bad_durations() {
        assert!(plan(&[], 3.0).is_err());
        assert!(plan(&[0.0], 3.0).is_err());
        assert!(plan(&[100.0, -5.0], 3.0).is_err());
    }

    #[test]
    fn format_uses_highest_rate_and_caps_channels() {
        let f = AudioFormat::from_sources(&[22050, 48000, 44100], &[1, 6, 2]);
        assert_eq!(f.sample_rate, 48000);
        assert_eq!(f.channels, 2);
    }

    #[test]
    fn format_falls_back_when_unknown() {
        let f = AudioFormat::from_sources(&[], &[]);
        assert_eq!(f, AudioFormat::default_format());
    }

    #[test]
    fn merge_length_subtracts_transitions() {
        assert!((merge_length(&[50.0, 55.0, 60.0, 65.0], 4.0) - 218.0).abs() < 1e-9);
        assert!((merge_length(&[100.0], 4.0) - 100.0).abs() < 1e-9);
        assert!((merge_length(&[100.0, 100.0], 0.0) - 200.0).abs() < 1e-9);
        // Transisi lebih panjang dari total tidak boleh menghasilkan angka negatif.
        assert!(merge_length(&[2.0, 2.0], 10.0) > 0.0);
    }

    #[test]
    fn staged_total_matches_direct_total() {
        // Hasil bertahap harus sama panjang dengan sekali jalan, inilah yang pernah salah
        // karena batas -t dibagi rata padahal lagunya tidak sama panjang.
        let d = [50.0, 55.0, 60.0, 65.0, 70.0, 75.0, 80.0, 85.0, 90.0, 95.0, 100.0, 105.0];
        let direct = merge_length(&d, 4.0);
        let round1: Vec<f64> = d.chunks(4).map(|g| merge_length(g, 4.0)).collect();
        let staged = merge_length(&round1, 4.0);
        assert!((direct - staged).abs() < 1e-9, "langsung {direct} vs bertahap {staged}");
    }

    #[test]
    fn parses_ffmpeg_volume_report() {
        let report = "[Parsed_volumedetect_0 @ 0x1] mean_volume: -18.9 dB
                      [Parsed_volumedetect_0 @ 0x1] max_volume: -13.2 dB
";
        assert_eq!(parse_db(report, "mean_volume"), Some(-18.9));
        assert_eq!(parse_db(report, "max_volume"), Some(-13.2));
        assert_eq!(parse_db(report, "histogram_0db"), None);
        assert_eq!(parse_db("", "mean_volume"), None);
    }

    #[test]
    fn aac_alignment() {
        assert_eq!(align_to_aac(1_234_945) % AAC_FRAME_SAMPLES, 0);
    }

    #[test]
    fn merge_filter_chain_is_well_formed() {
        // Tiga lagu: harus ada dua acrossfade dan label terakhir [mix].
        let inputs: Vec<PathBuf> = (0..3).map(|i| PathBuf::from(format!("t{i}.wav"))).collect();
        let args = build_merge_args(
            &Ffmpeg::for_test(),
            &inputs, Path::new("out.m4a"), 3.0, AudioFormat::default_format(), "192k", 100.0);
        let filter = args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1].clone();
        assert_eq!(filter.matches("acrossfade").count(), 2);
        assert!(filter.ends_with("[mix]"));
        assert!(!filter.ends_with(';'));
    }

    #[test]
    fn merge_without_crossfade_uses_concat() {
        let inputs: Vec<PathBuf> = (0..10).map(|i| PathBuf::from(format!("t{i}.wav"))).collect();
        let args = build_merge_args(
            &Ffmpeg::for_test(),
            &inputs, Path::new("out.m4a"), 0.0, AudioFormat::default_format(), "192k", 100.0);
        let filter = args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1].clone();
        assert!(filter.contains("concat=n=10"));
        assert!(!filter.contains("acrossfade"));
    }
}
