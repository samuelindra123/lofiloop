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

/// Batas panjang satu putaran loop untuk sebuah klip.
///
/// Satu putaran disusun dari tiga bagian klip: sapuan tengah, ekor, dan kepala, yang
/// disatukan oleh jendela crossfade. Ekor dan kepala memakai frame yang sama dengan
/// crossfade, sehingga klip harus menyediakan panjang putaran **ditambah** jendela
/// crossfade. Itulah sebabnya klip 10 detik tidak bisa menghasilkan putaran 10 detik.
///
/// Batas dihitung dari **panjang nyata klip**, bukan dari fps keluaran. Alasannya: kalau
/// fps keluaran dinaikkan di atas fps sumber, ffmpeg menggandakan frame yang sudah ada —
/// tidak ada informasi baru. Pada 60 fps, klip 240 frame memang menjadi 600 frame, tetapi
/// isinya tetap 10 detik materi. Menghitung batas dari fps keluaran akan melaporkan angka
/// yang tidak bisa dipakai dan membuat pesan galat menyarankan hal yang mustahil.
///
/// Nilai yang dikembalikan adalah panjang putaran terbesar yang masih muat, dalam detik,
/// pada skala waktu nyata klip. UI memakai angka ini untuk membatasi penggeser, dan
/// `build_unit` memakai perhitungan yang sama untuk menolak nilai yang tidak muat.
pub struct BatasLoop {
    /// Panjang putaran terbesar yang muat, detik. `None` berarti klip tidak bisa dipakai
    /// sama sekali, misalnya karena fps-nya tidak terbaca.
    pub maks_s: Option<f64>,
    /// Panjang klip sumber, detik.
    pub klip_s: f64,
    /// fps sumber, yang menentukan berapa frame nyata yang tersedia.
    pub fps_sumber: f64,
    /// fps keluaran yang dipilih.
    pub fps_keluaran: f64,
    /// Jendela crossfade yang dihitung, detik.
    pub xfade_s: f64,
    /// Frame nyata yang tersedia di klip. `None` kalau tidak terbaca.
    pub frame_tersedia: Option<u64>,
}

impl BatasLoop {
    /// Apakah nilai yang diminta muat.
    pub fn muat(&self, panjang_s: f64) -> bool {
        match self.maks_s {
            Some(maks) => panjang_s <= maks + TOLERANSI_FRAME_DETIK,
            None => false,
        }
    }

    /// Apakah menghitung ulang pada fps keluaran berbeda akan meminta frame yang lebih
    /// banyak daripada yang tersedia di klip. Dipakai UI untuk memberi peringatan tanpa
    /// melarang pilihan.
    pub fn rapat(self) -> bool {
        match (self.frame_tersedia, self.maks_s) {
            (Some(tersedia), Some(maks)) if tersedia > 0 => {
                let frame_dipakai = (maks * self.fps_keluaran).round() as u64;
                frame_dipakai > tersedia
            }
            _ => false,
        }
    }

    /// Penjelasan singkat untuk ditampilkan ke pengguna, dalam satuan detik.
    ///
    /// Sengaja memakai detik, bukan jumlah frame: antarmuka tidak pernah menampilkan frame,
    /// jadi angka frame tidak bisa dicocokkan oleh siapa pun yang membacanya.
    pub fn penjelasan(&self) -> String {
        match self.maks_s {
            None => "fps klip ini tidak terbaca, jadi panjang loop tidak bisa dihitung".into(),
            Some(maks) => format!(
                "klip {:.2} dtk ({:.2} fps), satu putaran paling panjang {:.2} dtk \
                 — ekor dan kepala diambil dari dalam klip yang sama",
                self.klip_s, self.fps_sumber, maks
            ),
        }
    }
}

/// Toleransi perbandingan panjang, dalam detik.
///
/// Dipakai supaya nilai batas yang ditampilkan ke pengguna selalu diterima kembali saat
/// dikirim. Tanpa ini, pembulatan pada dua sisi bisa menghasilkan selisih sepersekian
/// frame dan menolak angka yang sebenarnya sudah pas.
pub const TOLERANSI_FRAME_DETIK: f64 = 1.0 / 240.0;

/// Hitung batas panjang loop untuk sebuah klip.
///
/// Batasnya adalah **durasi klip itu sendiri**, dan tidak bergantung pada fps keluaran.
///
/// Alasannya ada pada urutan kerja ffmpeg: perubahan fps dipasang sebelum pemotongan frame,
/// sehingga saat pemotongan berjalan, aliran sudah sepanjang durasi asli klip pada fps
/// keluaran. Klip 10 detik tetap 10 detik pada 24 fps maupun 60 fps — yang berbeda hanya
/// jumlah frame per detiknya. Menghitung batas dari frame sumber lalu membaginya dengan fps
/// keluaran akan menghasilkan angka yang jauh lebih pendek daripada durasi klip, dan angka
/// itu memotong video pengguna tanpa alasan.
///
/// Satu-satunya pengurangan adalah bahan crossfade, yang diambil dari dalam klip yang sama.
pub fn batas_loop(source: &MediaInfo, xfade_s: f64, target: TargetVideo) -> BatasLoop {
    let fps_sumber = if source.fps > 0.0 { source.fps } else { 0.0 };
    let fps_keluaran = target.fps_efektif(source.fps);
    let klip_s = source.duration_s;

    let kosong = |xfade: f64| BatasLoop {
        maks_s: None,
        klip_s,
        fps_sumber,
        fps_keluaran,
        xfade_s: xfade,
        frame_tersedia: source.frame_count,
    };

    if fps_sumber <= 0.0 || klip_s <= 0.0 {
        return kosong(xfade_s);
    }

    // Crossfade harus menyisakan isi di tengah putaran. Aturannya dihitung pada fps sumber
    // karena frame crossfade diambil dari klip asli.
    let xfade_frame_sumber = (xfade_s * fps_sumber).round() as u64;
    let frame_klip = (klip_s * fps_sumber).round() as u64;
    if xfade_frame_sumber == 0 || xfade_frame_sumber * 2 >= frame_klip {
        return kosong(xfade_frame_sumber as f64 / fps_sumber);
    }

    // Batas adalah durasi klip. Pemotongan terjadi setelah fps keluaran diterapkan, jadi
    // seluruh durasi klip tersedia untuk dipakai satu putaran.
    BatasLoop {
        maks_s: Some(klip_s),
        klip_s,
        fps_sumber,
        fps_keluaran,
        xfade_s: xfade_frame_sumber as f64 / fps_sumber,
        frame_tersedia: Some(frame_klip),
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
        return Err(Error::Invalid(format!(
            "crossfade {xfade_s:.2} dtk terlalu pendek untuk {fps:.2} fps, \
             sehingga tidak ada frame yang bisa dipakai menyambung"
        )));
    }
    if xfade_frames * 2 >= total_frames {
        return Err(Error::Invalid(format!(
            "crossfade {xfade_s:.2} dtk terlalu panjang untuk putaran {length_s:.2} dtk \
             pada {fps:.2} fps"
        )));
    }
    // Durasi putaran tidak boleh melebihi durasi klip.
    //
    // Perbandingannya memakai detik, bukan jumlah frame, karena pemotongan terjadi setelah
    // fps keluaran diterapkan: pada fps keluaran berapa pun, panjang yang tersedia tetap
    // sepanjang klip. Membandingkan jumlah frame akan salah begitu fps keluaran berbeda dari
    // fps sumber — frame keluaran memang lebih banyak, tetapi durasinya sama.
    let info_batas = batas_loop(source, xfade_s, target);
    if !info_batas.muat(length_s) {
        let saran = match info_batas.maks_s {
            Some(maks) => format!(" Panjang loop paling besar {maks:.2} dtk."),
            None => String::new(),
        };
        return Err(Error::Invalid(format!(
            "klip ini {:.2} dtk, jadi belum bisa membuat satu putaran {length_s:.2} dtk.{saran}",
            source.duration_s
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Klip seperti yang dipakai pengguna: 10,01 detik pada 24 fps.
    fn klip_10_detik() -> MediaInfo {
        MediaInfo {
            path: "klip.mp4".into(),
            duration_s: 10.01,
            has_video: true,
            has_audio: false,
            width: 1280,
            height: 720,
            fps: 24.0,
            frame_count: Some(240),
            video_codec: Some("h264".into()),
            audio_codec: None,
            sample_rate: None,
            channels: None,
        }
    }

    #[test]
    fn batas_loop_tidak_berubah_walau_fps_keluaran_dinaikkan() {
        // Inilah cacat yang dilaporkan: dengan fps keluaran 60, batas sebelumnya dihitung
        // dari 60 fps sehingga melaporkan angka yang tidak bisa dipakai.
        let klip = klip_10_detik();
        let pada_sumber = batas_loop(&klip, 2.0, TargetVideo::asal());
        let pada_60 = batas_loop(
            &klip,
            2.0,
            TargetVideo {
                height: None,
                fps: Some(60.0),
            },
        );

        let a = pada_sumber.maks_s.expect("batas pada fps sumber");
        let b = pada_60.maks_s.expect("batas pada 60 fps");
        assert!(
            (a - b).abs() < 1e-9,
            "batas tidak boleh berubah karena fps keluaran: {a} vs {b}"
        );
        assert!((a - 10.0).abs() < 1e-9, "harus 10 dtk, dapat {a}");
    }

    #[test]
    fn angka_batas_yang_ditawarkan_selalu_diterima() {
        // Nilai yang ditampilkan ke pengguna harus lolos saat dikirim kembali, tanpa
        // terpengaruh pembulatan frame di kedua sisi.
        let klip = klip_10_detik();
        for fps in [None, Some(24.0), Some(30.0), Some(60.0)] {
            let target = TargetVideo { height: None, fps };
            let info = batas_loop(&klip, 2.0, target);
            let maks = info.maks_s.expect("batas tersedia");
            assert!(
                info.muat(maks),
                "batas {maks} harus dianggap muat pada fps {fps:?}"
            );
        }
    }

    #[test]
    fn panjang_loop_yang_dilaporkan_adalah_durasi_klip() {
        let klip = klip_10_detik();
        let info = batas_loop(&klip, 2.0, TargetVideo::asal());
        let maks = info.maks_s.expect("batas tersedia");

        // Klip 240 frame pada 24 fps berisi 10,01 dtk, dan seluruh durasi itu tersedia untuk
        // satu putaran. Nilai yang lebih kecil berarti video pengguna terpotong tanpa alasan.
        assert!(
            (maks - klip.duration_s).abs() < 1e-9,
            "batas harus sepanjang klip ({:.2} dtk), dapat {maks:.2}",
            klip.duration_s
        );
    }

    #[test]
    fn fps_60_tidak_memotong_klip_menjadi_seperempat() {
        // Cacat yang dilaporkan: pada 60 fps, batas pernah dilaporkan 4 dtk untuk klip
        // 10 dtk, karena frame sumber dibagi fps keluaran. Itu memotong video pengguna.
        let klip = klip_10_detik();
        let info = batas_loop(
            &klip,
            2.0,
            TargetVideo {
                height: None,
                fps: Some(60.0),
            },
        );
        let maks = info.maks_s.expect("batas tersedia");
        assert!(
            maks > 9.9,
            "pada 60 fps batas harus tetap sepanjang klip, bukan {maks:.2} dtk"
        );
    }

    #[test]
    fn putaran_sepanjang_klip_diterima_pada_semua_fps_keluaran() {
        // Yang paling penting bagi pengguna: memakai seluruh video harus berhasil, pada fps
        // keluaran berapa pun.
        let klip = klip_10_detik();
        for fps in [None, Some(24.0), Some(30.0), Some(60.0)] {
            let target = TargetVideo { height: None, fps };
            let info = batas_loop(&klip, 2.0, target);
            assert!(
                info.muat(klip.duration_s),
                "seluruh klip harus muat sebagai satu putaran pada fps {fps:?}"
            );
        }
    }

    #[test]
    fn meminta_lebih_panjang_dari_batas_ditolak() {
        let klip = klip_10_detik();
        let info = batas_loop(&klip, 2.0, TargetVideo::asal());
        assert!(!info.muat(15.0), "15 dtk tidak boleh dianggap muat");
        assert!(info.muat(6.0), "6 dtk harus muat");
    }

    #[test]
    fn fps_sumber_tidak_terbaca_tidak_membuat_batas_palsu() {
        let mut klip = klip_10_detik();
        klip.fps = 0.0;
        let info = batas_loop(&klip, 2.0, TargetVideo::asal());
        assert!(
            info.maks_s.is_none(),
            "tanpa fps, batas harus tidak diketahui"
        );
    }

    #[test]
    fn crossfade_terlalu_panjang_menghabiskan_klip() {
        // Crossfade 8 dtk pada klip 10 dtk menyisakan terlalu sedikit: 8 * 2 > 10.
        let klip = klip_10_detik();
        let info = batas_loop(&klip, 8.0, TargetVideo::asal());
        assert!(info.maks_s.is_none());
    }
}
