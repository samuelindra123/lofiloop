<div align="center">

# LofiLoop

**Mengubah klip pendek menjadi video lofi berdurasi panjang — dengan sambungan yang tidak terasa.**

[![Rilis](https://img.shields.io/github/v/release/samuelindra123/lofiloop?style=flat-square&color=5B8DEF)](https://github.com/samuelindra123/lofiloop/releases)
[![Lisensi](https://img.shields.io/badge/lisensi-MIT-5B8DEF?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-5B8DEF?style=flat-square)](#persyaratan)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-5B8DEF?style=flat-square)](https://www.rust-lang.org/)

</div>

---

## Ringkasan

LofiLoop menerima video pendek dan mengulangnya menjadi video berdurasi berapa pun — 1 jam,
3 jam, 8 jam — tanpa sambungan yang terlihat dan tanpa memori yang membengkak. Masalahnya bukan
sekadar mengulang: pemutaran berulang yang naif menghasilkan potongan kasar di setiap sambungan,
audio yang tersendat, dan berkas yang diproses ulang dari awal setiap kali.

LofiLoop menyelesaikannya dengan **pengulangan di tingkat paket** (`-c copy`) alih-alih
memproses ulang seluruh berkas: video diulang sebagai salinan stream, audio diulang sebagai
salinan paket AAC, dan hanya sambungan serta bagian akhir yang benar-benar di-encode. Pada laptop
uji (Celeron N5000, RAM 3,8 GB), 1 jam video 1080p jadi dalam **104 detik** dengan RAM puncak
**29 MB**.

Aplikasi ini juga menggabungkan banyak lagu menjadi satu berkas dengan transisi mulus antar
lagu, yang bisa langsung dipakai sebagai audio di halaman loop video.

## Kemampuan

<table>
<tr><td width="50%" valign="top">

**Loop video panjang**

Klip pendek diulang menjadi durasi berapa pun. Sambungan memakai crossfade, jadi tidak ada
potongan yang terlihat. Pengulangan dilakukan lewat salinan stream, sehingga berkas 8 jam tidak
memakan waktu 8 jam.

</td><td width="50%" valign="top">

**Ganti atau campur audio**

Suara video bisa dimatikan lalu diganti audio sendiri, dicampur keduanya, atau dipertahankan.
Volume audio disamakan otomatis ke -14 LUFS supaya tidak ada lagu yang terlalu keras.

</td></tr>
<tr><td valign="top">

**Gabung banyak lagu**

Berapa pun jumlah lagu, digabung menjadi satu berkas dengan transisi equal-power. Volume antar
lagu disamakan otomatis. Urutan bisa diatur dengan seret atau tombol.

</td><td valign="top">

**Hemat sumber daya**

Dirancang untuk laptop kelas bawah. Tidak ada berkas antara raksasa, tidak ada pemrosesan ulang
yang tidak perlu. Puncak RAM 29 MB untuk video 1 jam.

</td></tr>
</table>

## Memulai

### Pemasangan

**Installer (disarankan).** Unduh `LofiLoop-Setup-x.y.z.exe` dari halaman
[Rilis](https://github.com/samuelindra123/lofiloop/releases), lalu jalankan. Installer memasang
aplikasi beserta FFmpeg sekaligus — tidak ada dependensi yang perlu kamu pasang sendiri. Tidak
memerlukan hak administrator.

Installer selalu menyertakan FFmpeg bawaannya dan tidak memeriksa apakah komputermu sudah punya.
Itu disengaja: versinya jadi bisa dipastikan dan hasilnya seragam antar komputer.

**Versi portabel.** Unduh `LofiLoop-x.y.z-portable.zip`, ekstrak ke folder mana pun, jalankan
`lofiloop-app.exe`. Tidak ada yang ditulis ke sistem.

**Dari kode sumber.** Lihat [Membangun](#membangun).

### Pemakaian pertama

Buka aplikasi, dan wizard akan memeriksa mesin beserta FFmpeg. Kalau FFmpeg tidak ditemukan, ada
tombol unduh otomatis (SHA256 diverifikasi) atau pilihan menunjuk folder FFmpeg yang sudah kamu
punya. Setelah itu kamu masuk ke dua halaman aplikasi:

- **Loop video** — pilih video dan audio, atur durasi dan panjang loop, atur transisi, lalu render.
- **Gabung lagu** — tambah lagu, atur urutan, atur transisi, lalu gabungkan.

## Pemakaian lewat CLI

Semua kemampuan aplikasi tersedia di CLI, dan ini jalur tercepat untuk pekerjaan berulang.

```bash
# Video 1 jam dari klip 10 detik, mempertahankan suara video
lofiloop-cli.exe --video klip.mp4 --duration 1h --loop 10 -o hasil.mp4

# Video 3 jam dengan lagu, suara video dimatikan
lofiloop-cli.exe --video klip.mp4 --audio lagu.mp3 --duration 3h --loop 10 --video-xfade 2 -o hasil.mp4

# Gabung banyak lagu jadi satu berkas dengan transisi 4 detik
lofiloop-cli.exe --merge "D:\musik\*.mp3" --merge-xfade 4 -o "D:\hasil\gabungan.m4a"
```

### Opsi penting

| Opsi | Arti | Default |
|---|---|---|
| `--video` | video sumber | wajib untuk loop |
| `--audio` | audio pengganti, opsional | suara video |
| `--duration` | durasi hasil (`1h`, `90m`, `3600s`) | `1h` |
| `--loop` | panjang satu putaran video | `10` dtk |
| `--video-xfade` | crossfade antar putaran video, `0` = potong langsung | `1.0` dtk |
| `--audio-xfade` | crossfade antar putaran audio | `2.0` dtk |
| `--fade-out` | fade-out di akhir audio | `3.0` dtk |
| `--policy` | `replace` / `duck` / `keep` suara video | `replace` |
| `--encoder` | `quality` (libx264) atau `hardware` (h264_qsv) | `quality` |
| `--merge <berkas...>` | daftar lagu sesuai urutan | — |
| `--merge-xfade` | transisi antar lagu, `0` = potong langsung | `3` dtk |
| `--bitrate-audio` | bitrate audio hasil | `192k` |
| `--check-deps` | periksa dependensi lalu keluar | — |

### Perilaku audio otomatis

Audio tidak perlu diatur secara manual; LofiLoop memilih berdasarkan isi videonya:

- Video **tanpa** audio, atau audio videonya senyap (di bawah -50 dBFS): audio pengguna dipakai apa adanya.
- Video **bersuara**: mengikuti `--policy`. Default `replace` mematikan suara video.
- Tanpa `--audio`: suara video dipertahankan.

## Membangun

Butuh Rust 1.75+, Node.js 20+, dan FFmpeg di PATH.

```bash
git clone https://github.com/samuelindra123/lofiloop.git
cd lofiloop
npm install --prefix app/ui
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-app.ps1 -Release
```

Hasilnya di `target\release\lofiloop-app.exe`.

> **Penting:** jangan membangun dengan `cargo build` biasa. Aset frontend tidak ikut ter-embed,
> sehingga jendela terbuka kosong dan tidak ada tombol yang bekerja. Selalu pakai
> `scripts/build-app.ps1`.

### Membuat installer

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-installer.ps1
```

| Berkas | Isi | Ukuran |
|---|---|---|
| `LofiLoop-Setup-x.y.z.exe` | installer: aplikasi + FFmpeg + ffprobe | ±56 MB |
| `LofiLoop-x.y.z-portable.zip` | aplikasi + FFmpeg, tanpa pemasangan | ±76 MB |

Installer dibuat dengan [Inno Setup](https://jrsoftware.org/isinfo.php) 6+. Kalau belum
terpasang, skrip mencoba memasangnya lewat `winget` lalu memberi tahu perintahnya. Versi portabel
tetap dibuat walau tanpa Inno Setup.

### Menjalankan tes

```bash
cargo test
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-deps.ps1
python scripts/verify_output.py work/hasil.mp4
```

`verify_output.py` memeriksa hasil secara objektif: kesinambungan sambungan audio dan video
dibanding loop mentah, bukan sekadar "berkasnya ada".

## Cara kerja

Bagian ini menjelaskan keputusan rancangan yang membuat LofiLoop cepat. Kalau kamu hanya ingin
memakai aplikasinya, bagian ini bisa dilewati.

### Pengulangan lewat salinan stream

Cara naif mengulang video adalah menulis ulang seluruh berkas. Untuk 8 jam video, itu berarti
memproses 8 jam materi. LofiLoop sebaliknya membangun **satu putaran** yang sudah halus
sambungannya, lalu mengulang putaran itu dengan `-c copy`:

```
video 10 dtk ──► putaran halus 8 dtk ──► diulang 113× (salinan stream) ──► 900 dtk
```

Karena tidak ada frame yang didekode ulang, biaya didominasi tulis disk. Inilah sebabnya 1 jam
video 1080p selesai dalam 104 detik.

### Audio lewat pengulangan paket AAC

Encoding AAC itu lambat: 30 menit audio butuh **105 detik** dengan encoder bawaan ffmpeg. Untuk
audio 8 jam, itu tidak masuk akal.

LofiLoop karena itu meng-encode **satu putaran audio** saja, memotong priming dan padding encoder,
lalu mengulang putaran itu di tingkat paket AAC. Hasilnya 1 jam audio dalam **4 detik**, bukan
menit.

Encoder dipilih otomatis: `aac_mf` (MediaFoundation Windows) kalau tersedia, karena terukur
**49 detik** untuk 30 menit audio dibanding 105 detik encoder bawaan. Kalau tidak tersedia,
aplikasi memakai encoder bawaan.

### Crossfade equal-power

Sambungan antar lagu dan antar putaran memakai kurva **equal-power** (`qsin`), bukan linear.
Crossfade linear membuat volume terasa turun di tengah transisi karena daya totalnya berkurang;
equal-power menjaga daya tetap sehingga transisi terdengar rata.

Saat menggabungkan banyak lagu, transisi dibatasi sampai **seperempat lagu terpendek**, supaya
lagu pendek tidak habis dimakan transisi.

### Mengapa berkas antara selalu PCM

Saat menggabungkan lebih dari empat lagu, setiap kelompok ditulis sebagai PCM, bukan sebagai
AAC. Kalau berkas antara sudah di-encode, setiap tahap berikutnya harus mendekode lalu
meng-encode ulang seluruh audio, dan pekerjaan encoding berlipat untuk setiap tahap tambahan.
Menjaga berkas antara tetap PCM membuat encode hanya terjadi sekali di akhir.

## Hasil pengukuran

Angka lengkap ada di [docs/benchmark.md](docs/benchmark.md). Laptop uji: Celeron N5000, RAM 3,8 GB.

| Skenario | Waktu | Catatan |
|---|---|---|
| Video 1080p 1 jam | **104 dtk** | RAM puncak 29 MB |
| Audio 1 jam | **4 dtk** | lewat pengulangan paket AAC |
| Gabung 2 lagu | **5,5 dtk** | 6 menit materi |
| Gabung 24 lagu | **72 dtk** | 30 menit materi |
| Encode AAC 30 menit | **49 dtk** | `aac_mf`, dibanding 105 dtk bawaan |

Kualitas sambungan diukur, bukan diklaim. Untuk video, rasio selisih frame di sambungan
dibanding selisih frame tetangga: **1,05** (batas 1,5; loop mentah 10,9). Untuk audio,
kontinuitas **0,45–0,82** (batas 1,2; loop mentah 10,5).

RAM idle aplikasi 318 MB (Tauri + WebView2).

## Struktur repositori

```
crates/engine/    seluruh logika: probe, loop video, loop audio, gabung lagu, mux (tanpa UI)
crates/cli/       alat uji dan pemakaian harian
app/src-tauri/    shell Tauri, tipis
app/ui/           tampilan Svelte 5 (dua halaman: loop video, gabung lagu)
scripts/          build, installer, gerbang dependensi, pemeriksa hasil
docs/             catatan pengukuran dan laporan lingkungan
```

Logika seluruhnya ada di `crates/engine`, terpisah dari UI. UI hanya memanggil, dan CLI memakai
pustaka yang sama. Karena itu setiap perilaku bisa diuji tanpa membuka jendela.

## Persyaratan

| | |
|---|---|
| Sistem | Windows 10 1809+ atau Windows 11 (WebView2) |
| FFmpeg | disertakan installer; atau di PATH, `LOFILOOP_FFMPEG_DIR`, atau `assets/ffmpeg` |
| Membangun | Rust 1.75+, Node.js 20+, Inno Setup 6+ (untuk installer) |

## Lisensi

[MIT](LICENSE) © 2026 Samuel Indra Bastian.

FFmpeg tidak termasuk dalam lisensi ini. FFmpeg adalah proyek terpisah dengan lisensinya sendiri
(LGPL/GPL, tergantung build). Salinan yang disertakan installer berasal dari build resmi
[gyan.dev](https://www.gyan.dev/ffmpeg/builds/) dan diverifikasi SHA256 sebelum dipakai.
