//! Pemeriksaan versi terbaru dari GitHub Releases.
//!
//! Modul ini hanya menjawab satu pertanyaan: adakah versi yang lebih baru daripada yang
//! sedang berjalan, dan di mana berkasnya bisa diunduh.
//!
//! Sengaja **tidak** memasang apa pun. Memasang pembaruan berarti aplikasi menimpa dirinya
//! sendiri dengan berkas yang datang dari jaringan. Kalau langkah itu salah sedikit saja,
//! aplikasi yang sedang dipakai pengguna bisa rusak permanen dan tidak bisa dijalankan
//! lagi. Karena itu pemasangan tetap dilakukan pengguna lewat berkas yang diunduh.
//!
//! Pengambilan data memakai PowerShell, sama seperti pengunduhan ffmpeg di `lofiloop-engine`.
//! Alasannya bukan kebetulan: `reqwest` yang ikut terbawa Tauri dibangun tanpa backend TLS,
//! sehingga permintaan HTTPS ke GitHub akan gagal. PowerShell memakai tumpukan TLS sistem
//! yang sudah pasti tersedia di setiap Windows yang menjalankan aplikasi ini.
//!
//! Berkas dari GitHub diperlakukan sebagai data, bukan perintah. Yang diambil darinya hanya
//! nomor versi, sebuah alamat unduhan, dan nama berkas. Nilainya dibatasi formatnya sebelum
//! dipakai, dan tidak ada bagian dari jawaban itu yang dijalankan sebagai kode.

use serde::{Deserialize, Serialize};

/// Alamat feed rilis. GitHub mengembalikan rilis terbaru yang tidak ditandai prarilis.
const RELEASE_API: &str = "https://api.github.com/repos/samuelindra123/lofiloop/releases/latest";

/// Berapa lama hasil pemeriksaan dipakai sebelum diperiksa ulang, dalam detik.
///
/// GitHub membatasi jumlah permintaan per alamat IP untuk pemanggil tanpa autentikasi.
/// Tanpa jeda ini, membuka aplikasi berulang kali akan menghabiskan jatah tersebut, dan
/// setelah terlampaui seluruh permintaan dari alamat IP yang sama akan ditolak — termasuk
/// dari aplikasi lain. Satu hari cukup sering untuk pemberitahuan versi, dan tidak
/// membebani layanan siapa pun.
pub const JEDA_SIMPAN_DETIK: u64 = 24 * 60 * 60;

/// Hasil pemeriksaan, dipakai UI untuk memutuskan apa yang ditampilkan.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// Versi aplikasi yang sedang berjalan.
    pub current: String,
    /// Versi terbaru yang ditemukan di GitHub.
    pub latest: String,
    /// Benar kalau versi terbaru secara numerik lebih tinggi dari versi berjalan.
    pub tersedia: bool,
    /// Laman rilis, tempat semua berkas bisa diunduh.
    pub halaman: String,
    /// Nama berkas pemasang kalau ada, misalnya `LofiLoop-Setup-0.2.0.exe`.
    pub pemasang: Option<String>,
    /// Hasil ini diambil dari simpanan, bukan dari GitHub pada saat ini.
    pub dari_simpanan: bool,
}

/// Hasil pemeriksaan yang disimpan di disk, beserta waktu pengambilannya.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Simpanan {
    /// Waktu pengambilan, dalam detik sejak epoch Unix.
    pub diambil_pada: u64,
    /// Versi aplikasi saat pemeriksaan itu dilakukan.
    pub versi_saat_itu: String,
    pub latest: String,
    pub halaman: String,
    pub pemasang: Option<String>,
}

/// Bandingkan dua nomor versi bergaya `major.minor.patch`.
///
/// Mengembalikan `true` kalau `kandidat` lebih baru dari `sekarang`. Bagian yang tidak bisa
/// diurai dianggap nol, sehingga versi bertanda seperti `0.2.0-beta` tetap terbaca sebagai
/// `0.2.0` dan tidak membuat aplikasi menawarkan pembaruan yang tidak ada.
pub fn lebih_baru(kandidat: &str, sekarang: &str) -> bool {
    fn angka(teks: &str) -> Vec<u64> {
        teks.trim_start_matches('v')
            // Hentikan pada tanda apa pun yang menandai akhiran versi, seperti `-beta`.
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .next()
            .unwrap_or("")
            .split('.')
            .map_while(|n| n.parse::<u64>().ok())
            .collect()
    }

    let baru = angka(kandidat);
    let lama = angka(sekarang);
    if baru.is_empty() {
        return false;
    }
    // Bandingkan per posisi; kekurangan posisi dianggap nol.
    for i in 0..baru.len().max(lama.len()) {
        let a = baru.get(i).copied().unwrap_or(0);
        let b = lama.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// Ambil isi halaman JSON dari GitHub.
///
/// Dijalankan di thread pemanggil; pemanggil yang bertanggung jawab menjalankannya di luar
/// thread utama supaya jendela tidak berhenti merespons selama menunggu jaringan.
pub fn ambil_json(url: &str) -> Result<String, String> {
    // Batas waktu dipasang supaya aplikasi tidak menggantung saat jaringan lambat. Nilai
    // koneksi dan baca dipisah: koneksi yang tidak pernah terbentuk umumnya berarti tidak
    // ada internet, sedangkan koneksi yang terbentuk tapi lambat butuh waktu baca lebih besar.
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; \
         $ErrorActionPreference='Stop'; \
         (Invoke-WebRequest -UseBasicParsing -Uri '{url}' \
            -Headers @{{ 'User-Agent' = 'LofiLoop' }} \
            -TimeoutSec 12).Content",
        url = url
    );
    let out = lofiloop_engine::proc::command_str("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("tidak bisa menjalankan pemeriksa versi: {e}"))?;

    if !out.status.success() {
        return Err("tidak bisa menghubungi GitHub".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Tanyakan langsung ke GitHub, tanpa memeriksa simpanan.
fn tanya_github(versi_sekarang: &str) -> Result<UpdateInfo, String> {
    let teks = ambil_json(RELEASE_API)?;
    let nilai: serde_json::Value =
        serde_json::from_str(&teks).map_err(|e| format!("jawaban GitHub tidak terbaca: {e}"))?;

    let latest = nilai
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or("GitHub tidak menyertakan nomor versi")?
        .to_string();

    let halaman = nilai
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Cari berkas pemasang di antara lampiran rilis. Hanya nama berkas yang diambil, dan
    // hanya kalau namanya sesuai pola yang dikenal.
    let pemasang = nilai
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|daftar| {
            daftar
                .iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                .find(|nama| nama.starts_with("LofiLoop-Setup-") && nama.ends_with(".exe"))
        })
        .map(str::to_string);

    Ok(UpdateInfo {
        current: versi_sekarang.to_string(),
        tersedia: lebih_baru(&latest, versi_sekarang),
        latest,
        halaman,
        pemasang,
        dari_simpanan: false,
    })
}

/// Waktu sekarang dalam detik sejak epoch Unix.
fn sekarang_detik() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Lokasi berkas simpanan hasil pemeriksaan.
fn path_simpanan() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from))?;
    Some(base.join("LofiLoop").join("update-check.json"))
}

/// Baca simpanan, kalau ada dan masih berlaku.
///
/// Simpanan dianggap tidak berlaku kalau: berkasnya tidak ada, isinya tidak terbaca,
/// usianya melewati [`JEDA_SIMPAN_DETIK`], atau versi aplikasi berubah sejak pemeriksaan
/// itu dibuat. Pemeriksaan versi aplikasi penting supaya setelah pengguna memperbarui,
/// hasil lama tidak terus dipakai.
fn baca_simpanan(versi_sekarang: &str) -> Option<Simpanan> {
    let path = path_simpanan()?;
    let teks = std::fs::read_to_string(&path).ok()?;
    let simpanan: Simpanan = serde_json::from_str(&teks).ok()?;

    if simpanan.versi_saat_itu != versi_sekarang {
        return None;
    }
    let umur = sekarang_detik().saturating_sub(simpanan.diambil_pada);
    if umur >= JEDA_SIMPAN_DETIK {
        return None;
    }
    Some(simpanan)
}

/// Simpan hasil pemeriksaan. Kegagalan menyimpan diabaikan: hasilnya tetap dipakai untuk
/// sesi berjalan, dan yang hilang hanya penghematan permintaan pada pembukaan berikutnya.
fn tulis_simpanan(simpanan: &Simpanan) {
    let Some(path) = path_simpanan() else { return };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(teks) = serde_json::to_string_pretty(simpanan) {
        let _ = std::fs::write(&path, teks);
    }
}

/// Periksa pembaruan dengan memanfaatkan simpanan.
///
/// `paksa` melewati simpanan, dipakai tombol "Periksa sekarang" di aplikasi. Tanpa paksaan,
/// hasil yang masih berlaku dipakai kembali sehingga GitHub hanya dihubungi sekali sehari
/// walau aplikasi dibuka berkali-kali.
pub fn periksa_terjadwal(versi_sekarang: &str, paksa: bool) -> Result<UpdateInfo, String> {
    if !paksa {
        if let Some(s) = baca_simpanan(versi_sekarang) {
            return Ok(UpdateInfo {
                current: versi_sekarang.to_string(),
                tersedia: lebih_baru(&s.latest, versi_sekarang),
                latest: s.latest,
                halaman: s.halaman,
                pemasang: s.pemasang,
                dari_simpanan: true,
            });
        }
    }

    let info = tanya_github(versi_sekarang)?;
    tulis_simpanan(&Simpanan {
        diambil_pada: sekarang_detik(),
        versi_saat_itu: versi_sekarang.to_string(),
        latest: info.latest.clone(),
        halaman: info.halaman.clone(),
        pemasang: info.pemasang.clone(),
    });
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versi_lebih_tinggi_terdeteksi() {
        assert!(lebih_baru("0.2.0", "0.1.1"));
        assert!(lebih_baru("1.0.0", "0.9.9"));
        assert!(lebih_baru("v0.1.2", "0.1.1"));
    }

    #[test]
    fn versi_sama_atau_lebih_rendah_tidak_terdeteksi() {
        assert!(!lebih_baru("0.1.1", "0.1.1"));
        assert!(!lebih_baru("0.1.0", "0.1.1"));
        assert!(!lebih_baru("v0.1.1", "0.1.1"));
    }

    #[test]
    fn versi_bertanda_dibaca_sebagai_angka_dasarnya() {
        // Prarilis tidak boleh membuat aplikasi menawarkan pembaruan yang tidak ada.
        assert!(!lebih_baru("0.1.1-beta", "0.1.1"));
        assert!(lebih_baru("0.2.0-rc1", "0.1.1"));
    }

    #[test]
    fn posisi_yang_tidak_lengkap_dianggap_nol() {
        assert!(lebih_baru("0.2", "0.1.9"));
        assert!(!lebih_baru("0.1", "0.1.1"));
        assert!(lebih_baru("1", "0.9.9"));
    }

    #[test]
    fn masukan_tidak_wajar_tidak_menawarkan_pembaruan() {
        assert!(!lebih_baru("", "0.1.1"));
        assert!(!lebih_baru("bukan-versi", "0.1.1"));
    }
}
