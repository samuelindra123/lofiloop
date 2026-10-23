# Riwayat perubahan

Semua perubahan penting pada proyek ini dicatat di berkas ini.
Format mengikuti [Keep a Changelog](https://keepachangelog.com/id/1.1.0/),
dan proyek ini memakai [Semantic Versioning](https://semver.org/lang/id/).

## [0.1.1] — 2026-10-23

### Diperbaiki

- **Folder kerja tidak lagi tertinggal setelah pekerjaan selesai.** Sebelumnya setiap render
  meninggalkan berkas antara di `<hasil>.work` di sebelah berkas hasil — sekitar 1–2 GB per
  render — dan terus menumpuk. Sekarang folder itu dihapus setelah hasil jadi.
  Kalau pekerjaan gagal, isinya sengaja **disisakan** supaya bisa diperiksa.
- Penghapusan folder kerja tidak pernah bisa menggagalkan pekerjaan yang sudah berhasil:
  berkas yang masih terkunci dibiarkan, dan hasil tetap dilaporkan sukses.

### Diubah

- **Seluruh build berjalan di GitHub Actions.** Setiap dorongan ke `main` menjalankan
  `cargo fmt`, `cargo clippy`, `cargo test`, dan build frontend. Tag `v*` menghasilkan
  installer dan versi portabel, lalu menerbitkan rilis secara otomatis.
  Tidak ada lagi langkah build yang perlu dijalankan di komputer pengembang.

[0.1.1]: https://github.com/samuelindra123/lofiloop/releases/tag/v0.1.1

## [0.1.0] — 2026-10-23

Rilis pertama.

### Ditambahkan

- **Loop video** — mengulang klip pendek menjadi durasi berapa pun (1 jam sampai 8 jam) dengan
  crossfade antar putaran, lewat salinan stream sehingga tidak memproses ulang seluruh berkas.
- **Audio pengganti** — suara video bisa diganti, dicampur, atau dipertahankan; audio disamakan
  otomatis ke -14 LUFS.
- **Gabung banyak lagu** — menggabungkan berapa pun lagu menjadi satu berkas dengan transisi
  equal-power dan penyamaan volume antar lagu.
- **Aplikasi desktop** (Tauri + Svelte 5) dengan dua halaman, plus wizard pemasangan FFmpeg
  yang mengunduh dan memverifikasi SHA256 sendiri.
- **CLI** dengan seluruh kemampuan aplikasi, untuk pemakaian berulang dan pengujian.
- **Installer Windows** yang menyertakan FFmpeg dan ffprobe sekaligus, plus versi portabel.
- Skrip pengukuran sambungan audio dan video (`scripts/bench/`), serta pemeriksa hasil
  (`scripts/verify_output.py`).

### Catatan teknis

- Pengulangan audio memakai salinan paket AAC alih-alih encode ulang: 1 jam audio dalam 4 detik,
  dibanding 427 detik kalau di-encode penuh.
- Encoder AAC dipilih otomatis: `aac_mf` (MediaFoundation) kalau tersedia, karena terukur
  49 detik untuk 30 menit audio dibanding 105 detik encoder bawaan ffmpeg.
- Berkas antara saat menggabung lagu selalu PCM, supaya encode hanya terjadi sekali di akhir
  dan tidak berlipat untuk setiap tahap tambahan.

### Terverifikasi

- Video 1080p 1 jam: 104 detik, RAM puncak 29 MB.
- Sambungan video: rasio 1,05 (batas 1,5; loop mentah 10,9).
- Sambungan audio: kontinuitas 0,45–0,82 (batas 1,2; loop mentah 10,5).
- Gabung 2 lagu: 5,5 detik. Gabung 24 lagu: 72 detik.
- Installer diuji: dipasang tanpa suara, aplikasi berjalan memakai FFmpeg bawaannya tanpa FFmpeg
  di PATH, lalu dicopot bersih termasuk registry.

[0.1.0]: https://github.com/samuelindra123/lofiloop/releases/tag/v0.1.0
