//! Pembangunan unit loop video.
//!
//! Aturan penting: crossfade harus berakhir tepat di frame pertama klip.
//! Karena itu unit dibangun memakai **nomor frame**, bukan detik. Kalau memakai detik,
//! frame terakhir unit bukan 100% klip dan muncul lompatan kecil di sambungan
//! (terukur: rasio metrik 1,65 versus 1,10).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ffmpeg::{Ffmpeg, VideoEncoder};
use crate::probe::MediaInfo;
use crate::run::Ctx;

/// Hasil pembangunan unit video.
#[derive(Debug, Clone)]
pub struct VideoUnit {
    pub path: String,
    pub frames: u64,
    pub fps: f64,
    pub duration_s: f64,
    /// Jumlah frame yang dipakai untuk crossfade di awal unit.
    pub crossfade_frames: u64,
}

/// Pengaturan bentuk keluaran video.
///
/// Semua bidang bersifat opsional: `None` berarti "pertahankan seperti sumber".
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct TargetVideo {
    /// Tinggi keluaran dalam piksel. Lebarnya dihitung sendiri agar rasio aspek tetap.
    pub height: Option<u32>,
    /// Frame per detik keluaran.
    pub fps: Option<f64>,
}

impl TargetVideo {
    /// Tidak ada perubahan bentuk sama sekali.
    pub fn asal() -> Self {
        Self::default()
    }

    /// Apakah ada yang perlu diubah dari sumber.
    pub fn mengubah(self) -> bool {
        self.height.is_some() || self.fps.is_some()
    }

    /// Tinggi keluaran: dari pengaturan, atau dari sumber kalau tidak diatur.
    ///
    /// Angka ganjil dibulatkan ke bawah supaya hasilnya genap. Encoder H.264 dengan
    /// `yuv420p` menuntut kedua sisi genap; tinggi ganjil membuat encode gagal.
    pub fn tinggi_efektif(self, sumber: u32) -> u32 {
        match self.height {
            Some(h) if h > 0 => h & !1,
            _ => sumber & !1,
        }
    }

    /// Fps keluaran: dari pengaturan, atau dari sumber kalau tidak diatur.
    pub fn fps_efektif(self, sumber: f64) -> f64 {
        match self.fps {
            Some(f) if f > 0.0 => f,
            _ => sumber,
        }
    }
}

/// Bangun unit loop video dari `source` sepanjang `length_s`, dengan crossfade `xfade_s`.
///
/// Susunan unit: frame klip `[xfade_frames .. length_frames]` lebih dulu, lalu jendela
/// crossfade yang berakhir tepat di frame 0 klip. Jadi saat unit diulang, frame terakhir
/// menyambung ke frame pertama tanpa lompatan.
///
/// `target` mengatur skala dan fps keluaran. Kalau kosong, sumber dipakai apa adanya.
/// Nilainya dipakai untuk dua hal sekaligus dan keduanya harus memakai angka yang sama:
/// perhitungan nomor frame di fungsi ini, dan `-r` pada ffmpeg. Kalau keduanya berbeda,
/// panjang unit yang dihasilkan tidak sesuai rencana dan sambungan loop menjadi kasar.
// Argumennya sengaja tetap terpisah: semuanya nilai skalar yang dibaca sekali di sini, dan
// membungkusnya jadi struct hanya memindahkan kerumitan ke pemanggil tanpa menambah kejelasan.
#[allow(clippy::too_many_arguments)]
pub fn build_unit(
    ctx: &Ctx,
    ff: &Ffmpeg,
    source: &MediaInfo,
    out_dir: &Path,
    length_s: f64,
    xfade_s: f64,
    encoder: VideoEncoder,
    bitrate: &str,
    target: TargetVideo,
) -> Result<VideoUnit> {
    if source.fps <= 0.0 {
        return Err(Error::Invalid("fps sumber tidak terbaca".into()));
    }
    if length_s <= 0.0 {
        return Err(Error::Invalid(
            "panjang unit harus lebih dari 0 detik".into(),
        ));
    }

    // Fps keluaran dipakai untuk menghitung nomor frame, dan angka yang sama dipasang
    // sebagai `-r` di bawah. Keduanya wajib identik.
    let fps = target.fps_efektif(source.fps);
    let total_frames = (length_s * fps).round() as u64;
    let xfade_frames = (xfade_s * fps).round() as u64;

    if xfade_frames == 0 {
        return Err(Error::Invalid(
            "crossfade terlalu pendek untuk fps ini".into(),
        ));
    }
    if xfade_frames * 2 >= total_frames {
        return Err(Error::Invalid(format!(
            "crossfade {} dtk terlalu panjang untuk unit {} dtk",
            xfade_s, length_s
        )));
    }
    if total_frames > source.frame_count.unwrap_or(u64::MAX) {
        return Err(Error::Invalid(format!(
            "klip cuma {} frame, tidak cukup untuk unit {} frame",
            source.frame_count.unwrap_or(0),
            total_frames
        )));
    }

    let head_end = xfade_frames; // frame 0..xfade_frames = jendela yang di-crossfade
    let tail_start = total_frames - xfade_frames; // sapuan akhir klip
    let mid_end = tail_start;
    let mid_frames = mid_end - head_end;

    let unit_path = out_dir.join("unit_video.mp4");
    let unit = unit_path.display().to_string();

    // Skala dan fps dipasang di paling awal, sebelum pemotongan frame. Urutan ini penting:
    // `trim` bekerja dengan nomor frame, jadi jumlah frame harus sudah final saat trim
    // dijalankan. Kalau `fps` dipasang setelah trim, ffmpeg menambah atau membuang frame
    // belakangan dan nomor frame yang dipakai trim tidak lagi menggambarkan hasil akhir.
    //
    // `scale=-2:tinggi` menjaga rasio aspek dan membulatkan lebar ke bilangan genap.
    // `fps` tanpa `-r` juga diperlukan: `-r` saja hanya mengubah cara pembacaan, bukan
    // jumlah frame yang benar-benar ada di dalam aliran.
    let mut persiapan = String::new();
    if target.mengubah() {
        if let Some(h) = target.height.filter(|h| *h > 0) {
            persiapan.push_str(&format!("scale=-2:{},", target.tinggi_efektif(h)));
        }
        if target.fps.filter(|f| *f > 0.0).is_some() {
            persiapan.push_str(&format!("fps={fps},"));
        }
    }

    let filter = format!(
        "[0:v]{persiapan}split=3[a][b][c];\
         [a]trim=start_frame={head_end}:end_frame={mid_end},setpts=PTS-STARTPTS[mid];\
         [b]trim=start_frame={tail_start}:end_frame={total_frames},setpts=PTS-STARTPTS[tail];\
         [c]trim=start_frame=0:end_frame={head_end},setpts=PTS-STARTPTS[head];\
         [tail][head]xfade=transition=fade:duration={}:offset=0[seam];\
         [mid][seam]concat=n=2:v=1:a=0[v]",
        xfade_frames as f64 / fps
    );

    // GOP 2 detik, tertutup, tanpa scenecut: setiap awal unit jadi keyframe sehingga
    // pengulangan dengan -c copy selalu mulai dari IDR.
    let gop = (fps * 2.0).round().max(1.0) as u64;

    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        source.path.clone(),
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "[v]".into(),
    ];
    args.extend(encoder.args(bitrate, true));
    if encoder == VideoEncoder::X264 {
        args.extend([
            "-g".into(),
            gop.to_string(),
            "-keyint_min".into(),
            gop.to_string(),
            "-sc_threshold".into(),
            "0".into(),
        ]);
    } else {
        args.extend(["-g".into(), gop.to_string()]);
    }
    args.extend([
        "-r".into(),
        format!("{fps}"),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-an".into(),
        unit.clone(),
    ]);

    ctx.step(&format!(
        "membangun unit video: {total_frames} frame ({:.2} dtk), crossfade {} frame{}",
        total_frames as f64 / fps,
        xfade_frames,
        if target.mengubah() {
            let h = target.tinggi_efektif(source.height);
            format!(", keluaran {h}p pada {fps:.3} fps")
        } else {
            String::new()
        }
    ));
    ff.run(&args, "unit video")?;

    let frames = count_frames(ff, &unit).unwrap_or(total_frames);
    if mid_frames == 0 {
        ctx.step("peringatan: unit hanya berisi jendela crossfade");
    }

    Ok(VideoUnit {
        path: unit,
        frames,
        fps,
        duration_s: frames as f64 / fps,
        crossfade_frames: xfade_frames,
    })
}

/// Jumlah frame sebenarnya di berkas, dibaca dengan menghitung paket.
pub fn count_frames(ff: &Ffmpeg, path: &str) -> Result<u64> {
    let args: Vec<String> = vec![
        "-v".into(),
        "error".into(),
        "-select_streams".into(),
        "v:0".into(),
        "-count_packets".into(),
        "-show_entries".into(),
        "stream=nb_read_packets".into(),
        "-of".into(),
        "default=nw=1:nk=1".into(),
        path.into(),
    ];
    let out = ff.probe_text(&args)?;
    out.trim().parse::<u64>().map_err(|_| {
        Error::Probe(format!(
            "jumlah frame {path} tidak terbaca (keluaran: {out:?})"
        ))
    })
}
