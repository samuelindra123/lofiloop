//! Peluncuran proses bantu.
//!
//! Seluruh pemanggilan `powershell`, `ffmpeg`, dan `ffprobe` melewati modul ini. Tujuannya
//! satu: memastikan tidak ada proses konsol yang memunculkan jendela sendiri.

use std::path::Path;
use std::process::Command;

/// Siapkan `Command` untuk alat bantu baris perintah.
///
/// Aplikasi desktop dibangun tanpa jendela konsol. Tanpa penanganan khusus, setiap proses
/// konsol yang diluncurkan dari situ akan membuat jendela konsol baru yang berkedip lalu
/// menutup sendiri. Tampilan itu mengganggu dan membuat aplikasi terlihat tidak selesai.
///
/// Di Windows, `CREATE_NO_WINDOW` mencegah jendela itu dibuat tanpa menghilangkan keluaran
/// proses: stdout dan stderr tetap terbaca seperti biasa. Di sistem lain fungsi ini hanya
/// meneruskan `Command` apa adanya.
pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Sama seperti [`command`], untuk program yang ditulis sebagai teks biasa.
pub fn command_str(program: &str) -> Command {
    command(Path::new(program))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perintah_tersusun_tanpa_panic() {
        let cmd = command_str("powershell");
        // Tidak ada cara portabel untuk membaca creation_flags dari luar, jadi yang diuji
        // di sini adalah bahwa penyusunannya berhasil dan programnya benar.
        assert_eq!(cmd.get_program(), "powershell");
    }
}
