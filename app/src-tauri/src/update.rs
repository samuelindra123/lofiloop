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

use serde::Serialize;

/// Alamat feed rilis. GitHub mengembalikan rilis terbaru yang tidak ditandai prarilis.
const RELEASE_API: &str = "https://api.github.com/repos/samuelindra123/lofiloop/releases/latest";

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

/// Periksa rilis terbaru, lalu bandingkan dengan versi yang sedang berjalan.
pub fn periksa(versi_sekarang: &str) -> Result<UpdateInfo, String> {
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
    })
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
