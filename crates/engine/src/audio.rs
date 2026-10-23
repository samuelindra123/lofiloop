//! Audio: lagu diputar utuh, diulang kalau lebih pendek dari video, dan dipotong dengan
//! fade-out lembut di akhir.
//!
//! Hasil pengukuran yang mendasari pilihan ini:
//! 1. Panjang unit dibulatkan ke kelipatan 1024 sample supaya batas unit jatuh tepat di
//!    batas frame AAC.
//! 2. Unit di-encode **sekali**, lalu paket AAC-nya diulang dengan stream copy. Encode AAC
//!    streaming 1 jam butuh 427 dtk di mesin target; jalur ini butuh 4 dtk.
//!
//! Cara kerja sekarang:
//! - **Unit = seluruh lagu.** Panjang loop video tidak ada hubungannya dengan audio.
//! - Ujung lagu di-crossfade ke awalnya, sehingga saat diulang tidak ada lompatan.
//! - **Akhir hasil**: audio dipotong tepat di durasi target dan diberi fade-out lembut.
//!   Fade-out ini dibakar lewat satu encode pendek di ekor, bukan pada seluruh file.

use std::path::Path;

use crate::error::{Error, Result};
use crate::ffmpeg::Ffmpeg;
use crate::run::Ctx;

/// Jumlah sample per frame AAC pada 44,1 kHz. Dipakai untuk menyelaraskan panjang unit.
pub const AAC_FRAME_SAMPLES: u64 = 1024;

/// Crossfade terpendek yang masih terdengar halus pada musik.
const MIN_XFADE_S: f64 = 0.5;

/// Hasil pembangunan unit audio.
#[derive(Debug, Clone)]
pub struct AudioUnit {
    pub path: String,
    /// Durasi satu putaran yang sebenarnya (sudah dikurangi crossfade).
    pub duration_s: f64,
    pub samples: u64,
    pub sample_rate: u32,
    pub channels: u32,
    /// Crossfade yang dipakai di sambungan antar-putaran.
    pub crossfade_s: f64,
}

/// Rencana audio untuk satu render. Dipakai UI dan CLI untuk menampilkan apa yang akan terjadi.
#[derive(Debug, Clone)]
pub struct AudioPlan {
    pub source_s: f64,
    pub target_s: f64,
    pub crossfade_s: f64,
    pub fade_out_s: f64,
    pub unit_s: f64,
    pub repeats: u64,
    pub kind: AudioPlanKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioPlanKind {
    /// Lagu lebih pendek dari video: diulang.
    Loop,
    /// Lagu lebih panjang dari video: dipotong dengan fade-out.
    Trim,
    /// Panjangnya hampir sama: diputar sekali.
    Single,
}

impl AudioPlan {
    pub fn describe(&self) -> String {
        match self.kind {
            AudioPlanKind::Loop => format!(
                "audio {:.0} dtk diulang {:.1}x untuk mengisi {:.0} dtk, sambungan crossfade {:.1} dtk, fade-out {:.1} dtk di akhir",
                self.source_s,
                self.target_s / self.unit_s,
                self.target_s,
                self.crossfade_s,
                self.fade_out_s
            ),
            AudioPlanKind::Trim => format!(
                "audio {:.0} dtk lebih panjang dari video, dipotong di {:.0} dtk dengan fade-out {:.1} dtk",
                self.source_s, self.target_s, self.fade_out_s
            ),
            AudioPlanKind::Single => format!(
                "audio {:.0} dtk cocok dengan video, diputar sekali dengan fade-out {:.1} dtk",
                self.source_s, self.fade_out_s
            ),
        }
    }
}

/// Hitung rencana audio dari panjang lagu dan durasi target.
pub fn plan(source_s: f64, target_s: f64, crossfade_s: f64, fade_out_s: f64) -> Result<AudioPlan> {
    if source_s <= 0.0 || target_s <= 0.0 {
        return Err(Error::Invalid(
            "durasi audio dan target harus lebih dari 0".into(),
        ));
    }

    // Crossfade tidak boleh memakan lebih dari seperempat lagu, dan selalu ada isi di tengah.
    let max_xfade = (source_s / 4.0).max(MIN_XFADE_S);
    let xfade = crossfade_s.clamp(MIN_XFADE_S.min(source_s / 4.0), max_xfade);
    let fade_out = fade_out_s.clamp(0.5, (target_s / 2.0).max(0.5));

    let unit_s = (source_s - xfade).max(source_s / 2.0);

    let kind = if source_s > target_s + 0.5 {
        AudioPlanKind::Trim
    } else if (source_s - target_s).abs() <= 0.5 {
        AudioPlanKind::Single
    } else {
        AudioPlanKind::Loop
    };

    let repeats = match kind {
        AudioPlanKind::Loop => ((target_s / unit_s).ceil() as u64).max(2),
        _ => 1,
    };

    Ok(AudioPlan {
        source_s,
        target_s,
        crossfade_s: xfade,
        fade_out_s: fade_out,
        unit_s,
        repeats,
        kind,
    })
}

/// Bangun unit loop dari **seluruh** lagu.
///
/// Susunan: bagian lagu `[xfade .. akhir - xfade]` lebih dulu, lalu jendela crossfade
/// equal-power yang menyatukan ujung lagu ke awalnya. Saat diulang, akhir menyambung
/// ke awal tanpa klik.
// Argumennya sengaja tetap terpisah, sama seperti `video::build_unit`: semuanya nilai skalar
// yang dibaca sekali di sini.
#[allow(clippy::too_many_arguments)]
pub fn build_unit(
    ctx: &Ctx,
    ff: &Ffmpeg,
    source_path: &str,
    source_s: f64,
    out_dir: &Path,
    xfade_s: f64,
    sample_rate: u32,
    channels: u32,
) -> Result<AudioUnit> {
    if xfade_s <= 0.0 {
        return Err(Error::Invalid(
            "crossfade audio harus lebih dari 0 detik".into(),
        ));
    }
    if xfade_s * 2.0 >= source_s {
        return Err(Error::Invalid(format!(
            "crossfade {xfade_s} dtk terlalu panjang untuk audio {source_s:.1} dtk"
        )));
    }

    // Panjang unit = lagu dikurangi crossfade, dibulatkan ke kelipatan frame AAC.
    let unit_raw = ((source_s - xfade_s) * sample_rate as f64).round() as u64;
    let samples = round_to_aac_frame(unit_raw);
    let unit_s = samples as f64 / sample_rate as f64;

    // Seluruh lagu dipakai: bagian tengah + ekor yang dilebur ke kepala.
    let mid_start = xfade_s;
    let tail_start = source_s - xfade_s;
    let out_path = out_dir.join("unit_audio.wav");
    let out = out_path.display().to_string();

    let filter = format!(
        "[0:a]asplit=3[m][t][h];\
         [m]atrim=start={mid_start}:end={tail_start},asetpts=PTS-STARTPTS[mid];\
         [t]atrim=start={tail_start}:end={source_s},asetpts=PTS-STARTPTS[tail];\
         [h]atrim=start=0:end={xfade_s},asetpts=PTS-STARTPTS[head];\
         [tail][head]acrossfade=d={xfade_s}:c1=qsin:c2=qsin[seam];\
         [mid][seam]concat=n=2:v=0:a=1[a]"
    );

    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        source_path.into(),
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "[a]".into(),
        "-ar".into(),
        sample_rate.to_string(),
        "-ac".into(),
        channels.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        "-t".into(),
        format!("{unit_s}"),
        out.clone(),
    ];

    ctx.step(&format!(
        "audio: memakai seluruh lagu ({source_s:.1} dtk), satu putaran {unit_s:.2} dtk, crossfade sambungan {xfade_s:.1} dtk"
    ));
    ff.run(&args, "unit audio")?;

    Ok(AudioUnit {
        path: out,
        duration_s: unit_s,
        samples,
        sample_rate,
        channels,
        crossfade_s: xfade_s,
    })
}

/// Normalisasi loudness unit sebelum encode. `loudnorm` tidak bisa dipakai di jalur loop paket AAC.
pub fn normalize_unit(
    ctx: &Ctx,
    ff: &Ffmpeg,
    unit: &AudioUnit,
    out_dir: &Path,
    target_lufs: f64,
    true_peak_db: f64,
) -> Result<AudioUnit> {
    let out_path = out_dir.join("unit_audio_norm.wav");
    let out = out_path.display().to_string();

    // Ukur sekali, lalu geser dengan filter volume. Sama seperti di jalur gabung lagu,
    // ini jauh lebih cepat daripada loudnorm yang memproses ulang seluruh audio.
    let gain = crate::merge::compute_gain(ff, &unit.path, target_lufs, true_peak_db)?;
    let filter = if gain.abs() > 0.05 {
        format!("volume={gain:.2}dB")
    } else {
        "anull".to_string()
    };

    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        unit.path.clone(),
        "-af".into(),
        filter,
        "-ar".into(),
        unit.sample_rate.to_string(),
        "-ac".into(),
        unit.channels.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        "-t".into(),
        format!("{}", unit.duration_s),
        out.clone(),
    ];

    ctx.step(&format!(
        "menyamakan volume audio ke sekitar {target_lufs} LUFS (geser {gain:+.1} dB)"
    ));
    ff.run(&args, "normalisasi audio")?;
    Ok(AudioUnit {
        path: out,
        ..unit.clone()
    })
}

/// Encode unit audio ke AAC dan potong salinan yang bersih.
///
/// Encoder AAC menyisipkan priming di awal dan padding di akhir. Supaya potongan bersih,
/// unit ditulis tiga kali berturut-turut, lalu salinan tengah dipotong dengan stream copy.
pub fn encode_unit(
    ctx: &Ctx,
    ff: &Ffmpeg,
    unit: &AudioUnit,
    out_dir: &Path,
    bitrate: &str,
) -> Result<String> {
    let tripled = out_dir.join("unit_audio_x3.m4a");
    let clean = out_dir.join("unit_audio.m4a");

    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-stream_loop".into(),
        "2".into(),
        "-i".into(),
        unit.path.clone(),
        "-c:a".into(),
        ff.best_aac(),
        "-b:a".into(),
        bitrate.into(),
        "-ar".into(),
        unit.sample_rate.to_string(),
        "-ac".into(),
        unit.channels.to_string(),
        tripled.display().to_string(),
    ];
    ctx.step("encode satu putaran audio ke AAC (sekali saja, hasilnya dipakai berulang)");
    ff.run(&args, "encode unit audio")?;

    let dur = format!("{}", unit.duration_s);
    let cut: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        tripled.display().to_string(),
        "-ss".into(),
        dur.clone(),
        "-t".into(),
        dur,
        "-c".into(),
        "copy".into(),
        clean.display().to_string(),
    ];
    ctx.step("memotong salinan tengah untuk membuang priming dan padding encoder");
    ff.run(&cut, "potong unit audio")?;

    let _ = std::fs::remove_file(&tripled);
    Ok(clean.display().to_string())
}

/// Susun audio akhir sepanjang `target_s`.
///
/// - Bagian badan diulang lewat stream copy (cepat), dipotong tepat sebelum fade-out.
/// - Ekor sepanjang `fade_out_s` diambil dari unit sumber, diberi fade-out, lalu di-encode
///   sekali. Hanya ekor ini yang di-encode ulang, jadi tetap cepat untuk durasi berapa pun.
// Argumennya sengaja tetap terpisah, sama seperti `build_unit`.
#[allow(clippy::too_many_arguments)]
pub fn assemble(
    ctx: &Ctx,
    ff: &Ffmpeg,
    unit: &AudioUnit,
    encoded_unit: &str,
    out_dir: &Path,
    target_s: f64,
    fade_out_s: f64,
    bitrate: &str,
) -> Result<String> {
    let fade_out_s = fade_out_s.min(target_s / 2.0).max(0.1);
    let body_s = (target_s - fade_out_s).max(0.0);
    let final_path = out_dir.join("audio_final.m4a");

    // Posisi di dalam unit tempat ekor harus dimulai, supaya fade-out jatuh di audio yang
    // memang sedang diputar pada detik itu.
    let tail_offset = body_s % unit.duration_s;

    let body_path = out_dir.join("audio_body.m4a");
    let body_repeats = ((body_s / unit.duration_s).ceil() as u64).max(1);
    let body_args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-stream_loop".into(),
        (body_repeats.saturating_sub(1)).to_string(),
        "-i".into(),
        encoded_unit.into(),
        "-c".into(),
        "copy".into(),
        "-t".into(),
        format!("{body_s}"),
        body_path.display().to_string(),
    ];

    ctx.step(&format!(
        "audio: badan {:.0} dtk dari {body_repeats} putaran (stream copy, tanpa encode ulang)",
        body_s
    ));
    ff.run(&body_args, "badan audio")?;

    // Ekor: ambil audio dari posisi `tail_offset` di unit PCM, beri fade-out, encode.
    //
    // Pemotongan dan fade dilakukan di dalam satu rantai filter dengan `asetpts` supaya
    // timestamp mulai dari 0. Kalau `-ss` dipakai di sisi output, `afade st=0` dihitung dari
    // timestamp sumber (misalnya 6,98 dtk), sehingga fade dianggap sudah terlewati dan
    // ekornya menjadi senyap total. Rantai filter di bawah menghindari hal itu.
    let tail_path = out_dir.join("audio_tail.m4a");
    let tail_end = tail_offset + fade_out_s;
    let tail_args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-stream_loop".into(),
        "1".into(),
        "-i".into(),
        unit.path.clone(),
        "-af".into(),
        format!(
            "atrim=start={tail_offset}:end={tail_end},asetpts=PTS-STARTPTS,\
             afade=t=out:st=0:d={fade_out_s}:curve=qsin"
        ),
        "-c:a".into(),
        ff.best_aac(),
        "-b:a".into(),
        bitrate.into(),
        "-ar".into(),
        unit.sample_rate.to_string(),
        "-ac".into(),
        unit.channels.to_string(),
        tail_path.display().to_string(),
    ];
    ctx.step(&format!(
        "audio: fade-out lembut {fade_out_s:.1} dtk di akhir (satu-satunya bagian yang di-encode ulang)"
    ));
    ff.run(&tail_args, "ekor audio")?;

    // Gabungkan badan + ekor lewat concat demuxer, tanpa encode ulang. Path di daftar concat
    // dibaca relatif terhadap folder daftar itu sendiri, jadi cukup nama berkas: semuanya
    // memang berada di folder kerja yang sama.
    let list_path = out_dir.join("audio_concat.txt");
    let file_name = |p: &Path| -> String {
        p.file_name()
            .map(|n| n.to_string_lossy().replace('\'', "'\\''"))
            .unwrap_or_default()
    };
    let list = format!(
        "file '{}'\nfile '{}'\n",
        file_name(&body_path),
        file_name(&tail_path)
    );
    std::fs::write(&list_path, list)?;

    let concat_args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_path.display().to_string(),
        "-c".into(),
        "copy".into(),
        final_path.display().to_string(),
    ];
    ff.run(&concat_args, "gabung badan dan ekor audio")?;

    for f in [&body_path, &tail_path, &list_path] {
        let _ = std::fs::remove_file(f);
    }
    Ok(final_path.display().to_string())
}

fn round_to_aac_frame(samples: u64) -> u64 {
    let frames = (samples + AAC_FRAME_SAMPLES / 2) / AAC_FRAME_SAMPLES;
    frames.max(1) * AAC_FRAME_SAMPLES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_song_loops() {
        let p = plan(180.0, 3600.0, 2.0, 3.0).unwrap();
        assert_eq!(p.kind, AudioPlanKind::Loop);
        assert!((p.unit_s - 178.0).abs() < 1e-9);
        assert!(p.repeats >= 21);
    }

    #[test]
    fn long_song_is_trimmed() {
        let p = plan(600.0, 300.0, 2.0, 3.0).unwrap();
        assert_eq!(p.kind, AudioPlanKind::Trim);
        assert_eq!(p.repeats, 1);
    }

    #[test]
    fn near_equal_plays_once() {
        let p = plan(300.0, 300.2, 2.0, 3.0).unwrap();
        assert_eq!(p.kind, AudioPlanKind::Single);
    }

    #[test]
    fn crossfade_is_capped_for_short_audio() {
        // Audio 6 dtk: crossfade 2 dtk melebihi seperempat lagu, harus dipotong jadi 1,5 dtk.
        let p = plan(6.0, 60.0, 2.0, 3.0).unwrap();
        assert!(p.crossfade_s <= 1.5 + 1e-9);
    }

    #[test]
    fn fade_out_never_exceeds_half_the_target() {
        let p = plan(10.0, 4.0, 1.0, 10.0).unwrap();
        assert!(p.fade_out_s <= 2.0 + 1e-9);
    }

    #[test]
    fn rejects_zero_lengths() {
        assert!(plan(0.0, 60.0, 2.0, 3.0).is_err());
        assert!(plan(60.0, 0.0, 2.0, 3.0).is_err());
    }

    #[test]
    fn aac_alignment_rounds_to_1024() {
        assert_eq!(round_to_aac_frame(1_234_944) % AAC_FRAME_SAMPLES, 0);
        assert_eq!(round_to_aac_frame(1) % AAC_FRAME_SAMPLES, 0);
    }
}
