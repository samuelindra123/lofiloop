# Laporan Fase 1 — Spike validasi

Tanggal: 2026-10-22. Mesin: Intel Pentium Silver N5000 (4 core, 1,1 GHz), RAM 3,8 GB, Intel UHD Graphics 605, Windows 11 Pro, disk C: eMMC.

Semua angka di bawah hasil ukur di mesin ini, bukan estimasi.

## 1. Kesimpulan

| Yang diukur | Hasil |
|---|---|
| Loop video 1 jam dengan `-stream_loop` + `-c copy` bisa tanpa glitch? | **Ya.** 104 dtk, RAM puncak 29 MB, DTS monoton, keyframe di setiap batas unit, durasi tepat |
| Loop audio 1 jam bisa mulus? | **Ya**, tapi encode AAC streaming lambat (427 dtk). Jalur paket AAC: **4 dtk** |
| Sambungan bisa diukur, bukan cuma dirasa? | **Ya.** Metrik video dan audio, keduanya lulus, dan keduanya bisa mendeteksi loop mentah tanpa crossfade |
| RAM idle Tauri vs Slint? | **Tauri 318 MB** (7 proses), **Slint 67 MB** (1 proses) |
| Keputusan stack | **Tauri 2** (dipilih user 2026-10-22). Lihat bagian 5 |

## 2. Video

Sumber uji: 1080p25 10 dtk, libx264 veryfast 4 Mbps (4,7 MB).

Unit loop: 224 frame (8,96 dtk), keyframe tiap 50 frame (2 dtk), closed GOP
(`-g 50 -keyint_min 50 -sc_threshold 0 -x264-params "open-gop=0:scenecut=0"`).

Hasil render 1 jam (`-stream_loop 401 -c copy -t 3600`):

| Metrik | Nilai |
|---|---|
| Waktu dinding | 103,6 dtk |
| CPU | 10,9 dtk (sisanya I/O) |
| RAM puncak | 28,7 MB |
| Ukuran | 1.787 MB |
| Frame | 90.002 |
| Durasi | 3600,08 dtk |

Pemeriksaan level paket (`check_video_seams.py`): 0 DTS tidak monoton, 0 jarak frame menyimpang,
0 batas unit tanpa keyframe, GOP 24–50 frame.

Metrik sambungan (`video_seam_metric.py`), selisih frame di batas dibagi rata-rata 10 frame tetangga:

| Kasus | Rasio | Hasil |
|---|---|---|
| Unit dengan crossfade berakhir di frame 0 klip | **1,10** | LULUS (batas 1,5) |
| Unit crossfade berakhir 1 frame sebelum frame 0 | 1,65 | GAGAL |
| Loop mentah tanpa crossfade (kontrol negatif) | 10,82 | GAGAL |

**Temuan yang mengubah implementasi:** crossfade harus berakhir tepat di frame pertama klip.
Kalau tidak, frame terakhir unit bukan 100% klip dan muncul lompatan kecil di sambungan.
Unit dibangun dengan nomor frame (`trim=start_frame=26:end_frame=224`), bukan dengan detik.

## 3. Audio

Sumber uji: 30 dtk stereo 44,1 kHz hasil sintesis (konten periodik, amplitudo termodulasi lambat).

Unit loop: `acrossfade` equal-power 2 dtk dari tail (28–30 dtk) ke head (0–2 dtk), lalu disambung
dengan mid (2–28 dtk) sehingga kontinu saat diulang. Panjang unit 28 dtk.

| Cara render 1 jam | Waktu | CPU | RAM puncak |
|---|---|---|---|
| Loop PCM lalu encode AAC streaming | 427,4 dtk | 352,5 dtk | 38,5 MB |
| **Encode unit sekali, lalu loop paket AAC (`-c copy`)** | **4 dtk** | — | — |

Metrik sambungan (`audio_seam_metric.py`), kontinuitas = |x[n]−x[n−1]| terbesar di batas dibagi
terbesar di tempat lain; dip = minimum RMS 100 ms di zona crossfade dibanding minimum alami:

| Kasus | Kontinuitas | Dip | Hasil |
|---|---|---|---|
| AAC copy, 0–140 dtk (awal file) | 0,79 | +2,0 dB | LULUS |
| AAC copy, 3500–3600 dtk (akhir file) | 0,51 | +1,9 dB | LULUS |
| PCM streaming, 0–140 dtk | 0,82 | +2,5 dB | LULUS |
| Loop mentah tanpa crossfade (kontrol negatif) | 10,46 | +1,7 dB | GAGAL |

**Temuan yang mengubah implementasi:** encode AAC 1 jam ≈ 11× realtime (terukur: 120 dtk audio →
10,9 dtk), jadi encode streaming 3 jam akan memakan ±20 menit hanya untuk audio. Encode unit sekali
lalu loop paket AAC-nya menurunkan itu ke hitungan detik. Supaya batas unit jatuh tepat di batas
frame AAC, panjang unit dibulatkan ke kelipatan 1024 sample (44,1 kHz → 1206 frame = 28,0033 dtk).
Untuk membuang priming/padding encoder, unit di-encode bersama dua salinan lain, lalu salinan tengah
dipotong dengan `-ss/-t -c copy`.

Konsekuensi: `loudnorm` dan normalisasi per-sample tidak bisa dipakai saat loop copy. Keduanya harus
dibakar ke unit audio sebelum encode.

## 4. Hasil gabungan 1 jam

`final_1h.mp4` = video_1h.mp4 + audio_1h_copy.m4a, mux `-c copy`, `+faststart`.

| Metrik | Nilai |
|---|---|
| Waktu mux | 118,7 dtk |
| RAM puncak | 53,9 MB |
| Ukuran | 1.964.264.011 byte (1,96 GB) |
| Video | 90.002 frame, 3600,08 dtk |
| Audio | 155.040 paket, 3600,0218 dtk |
| Selisih A/V | 0,058 dtk (±1,5 frame) |
| Dekode penuh 1 jam | 800 dtk, **0 error** |

## 5. RAM idle: Tauri vs Slint

Aplikasi uji: satu jendela, satu slider dan satu tombol, tanpa logika. Diukur 15 dtk setelah start,
profil release.

| UI | Proses | RAM total | Rincian |
|---|---|---|---|
| Slint 1.18 | 1 | **67,2 MB** | spike-slint 67,2 |
| Tauri 2 | 7 | **318,2 MB** | 6 proses WebView2 (8,7–107 MB) + spike-tauri 26,3 MB |

Catatan: pengukuran pertama melaporkan 616 MB karena setiap proses WebView2 terhitung dua kali
(sekali lewat penelusuran proses anak, sekali lewat snapshot WebView2). Skrip sudah diperbaiki.

**Keputusan:** Tauri 2, dipilih user. Tauri ±251 MB lebih berat dari Slint di jendela kosong, tapi
membawa preview video dengan decode hardware dan pembuatan UI yang jauh lebih cepat. Mitigasi:
webview ditutup saat render dan jendela disembunyikan ke tray, sehingga hanya proses Rust kecil
yang tersisa. Angka itu belum diukur dan harus dibuktikan di Fase 3. Risiko yang perlu diuji:
di Windows WebView2 kadang meninggalkan proses zombie saat aplikasi induk ditutup paksa.

## 6. Waktu kompilasi di mesin ini

| Build | Waktu | Catatan |
|---|---|---|
| Slint release (percobaan gagal, dengan LTO) | 373 dtk lalu crash | `STATUS_STACK_BUFFER_OVERRUN` |
| Tauri release (percobaan gagal, dengan LTO) | 1236 dtk lalu crash | crate `windows` |
| **Slint release (tanpa LTO)** | **25 mnt 06 dtk** | 8,7 MB |
| **Tauri release (tanpa LTO)** | **21 mnt 50 dtk** | 3,3 MB |

Penyebab crash: `lto = true` + `codegen-units = 1` dengan RAM bebas hanya ±224 MB. Profil rilis
sekarang `opt-level = "s"`, `panic = "abort"`, `strip = true` tanpa LTO. Kesalahan proses yang ikut
memperlambat: memaksa `CARGO_BUILD_JOBS=1` sehingga 3 dari 4 core menganggur.

Konsekuensi untuk proyek: jaga jumlah dependensi tetap kecil, dan bangun rilis hanya saat perlu.

## 7. Temuan tambahan Fase 2: encoder hardware merusak sambungan

Saat pipeline sungguhan pertama kali dijalankan (5 menit, 1080p, `work/e2e/hasil_5m.mp4`),
metrik sambungan video **gagal di 1,73**. Penyebabnya bukan struktur filter, melainkan encoder.

Uji pemisahan, semuanya dengan struktur filter dan argumen GOP yang sama:

| Encoder unit | Rasio sambungan | Hasil |
|---|---|---|
| `h264_qsv` (UHD 605) | 1,73, konsisten di setiap batas unit | GAGAL |
| `libx264` preset veryfast | 1,38 | LULUS |
| `libx264` + crossfade 2 dtk | **1,05** (terburuk), 0,64 di sambungan khas | LULUS |
| Loop mentah tanpa crossfade (kontrol negatif) | 10,92 | GAGAL |

Dugaan: encoder hardware menaruh artefak di frame pertama setiap GOP. Karena setiap pengulangan
unit dimulai tepat di keyframe, artefak itu berulang persis di sambungan dan terlihat sebagai
lompatan kecil.

**Keputusan:** default encoder adalah `libx264`. `--encoder hardware` tetap tersedia untuk
keperluan cepat, dengan konsekuensi sambungan lebih kasar. Ini juga alasan `--video-xfade`
default dinaikkan dari 1 detik ke 2 detik: crossfade lebih panjang menurunkan lompatan
sisa (1,38 pada 1 dtk, 1,05 pada 2 dtk).

## 9. Verifikasi pipeline sungguhan (Fase 2)

Setelah engine selesai, hasilnya diuji lewat CLI dengan skrip `scripts/verify_output.py`
(yang memeriksa durasi, selisih A/V, dan kedua metrik sambungan sekaligus).

| Uji | Hasil |
|---|---|
| 5 menit, 1080p, video + audio | 23,1 dtk, 161 MB, selisih A/V 0,000 dtk, video 1,05, audio kontinuitas 0,45 |
| **1 jam, 1080p, video + audio** | **250,6 dtk, 1913 MB, video 3600,000 dtk / 90000 frame, audio 3599,999 dtk, selisih 0,001 dtk** |
| Sambungan video 1 jam (14 titik di 120 dtk pertama) | rasio terburuk **1,06** |
| Sambungan audio 1 jam (13 titik) | kontinuitas **0,45**, dip **+0,1 dB** |
| Sambungan audio di akhir file (3500–3600 dtk) | kontinuitas 0,64, dip +2,2 dB |
| Kontrol negatif: loop mentah tanpa crossfade | rasio **10,92** (GAGAL, sesuai harapan) |

Selisih A/V 0,001 dtk hanya 1/40 frame, jadi video dan audio jatuh tepat bersamaan.

## 10. Yang belum dibuktikan

1. **Durasi 3 jam.** Hanya 1 jam yang diuji. Kerja streaming-nya linear, tapi belum diukur.
2. **Penilaian telinga dan mata manusia.** Metrik membuktikan tidak ada klik dan tidak ada lompatan
   frame. Apakah sambungan *terasa* mulus belum diuji. Konten uji sintetis, bukan video lofi asli.
3. **Crossfade audio tanpa keselarasan beat.** Untuk materi musik, crossfade 2 dtk yang tidak selaras
   bar bisa terdengar sebagai ropekan. Solusi di rencana (titik potong di zero-crossing dan kelipatan
   bar) belum diimplementasi maupun diuji.
4. **RAM saat webview ditutup dan aplikasi di tray.** Mitigasi untuk angka Tauri 318 MB.
5. **Video 3 jam + audio panjang: ukuran file dan ruang disk.** 1 jam sudah 1,96 GB, jadi 3 jam
   ±5,9 GB. Cek ruang disk sebelum render wajib ada.

## 11. Dampak ke PLAN.md

- Unit loop video harus dibangun dengan nomor frame supaya crossfade berakhir di frame 0 klip.
- Loop audio pakai jalur paket AAC sebagai default, bukan encode streaming.
- Panjang unit audio dibulatkan ke kelipatan 1024 sample.
- `loudnorm` dibakar ke unit audio sebelum encode.
- Profil rilis: tanpa LTO, dengan `opt-level = "s"`.
- Anggaran RAM UI: 318 MB terukur untuk jendela kosong, bukan 150 MB seperti estimasi awal.
