# Berkontribusi

Terima kasih sudah tertarik. Berikut hal yang perlu diketahui sebelum mengirim perubahan.

## Sebelum memulai

Buka *issue* dulu untuk perubahan yang cukup besar, supaya arahnya bisa disepakati sebelum kamu
menulis banyak kode.

## Menyiapkan lingkungan

Butuh Rust 1.75+, Node.js 20+, dan FFmpeg di PATH.

```bash
git clone https://github.com/samuelindra123/lofiloop.git
cd lofiloop
npm install --prefix app/ui
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-deps.ps1
```

## Menjalankan tes

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Untuk perubahan yang menyentuh audio atau video, jalankan juga pemeriksa hasil. Tes unit tidak
bisa menangkap sambungan yang kasar, jadi ini yang memastikan:

```bash
python scripts/verify_output.py work/hasil.mp4
```

Untuk situs di `website/`, ujinya dijalankan dari folder itu dan memeriksa hasil build:

```bash
cd website
pnpm install
pnpm uji
```

## Aturan yang dipegang proyek ini

**Logika ada di `crates/engine`, bukan di UI.** UI hanya memanggil. Kalau kamu menambahkan
kemampuan baru, taruh logikanya di engine supaya CLI dan aplikasi memakai jalur yang sama dan
bisa diuji tanpa membuka jendela.

**Jangan encode ulang kalau bisa dihindari.** Prinsip yang membuat LofiLoop cepat adalah
memakai salinan stream untuk pengulangan. Setiap encode ulang yang tidak perlu akan langsung
terasa untuk berkas berdurasi panjang.

**Berkas antara tetap PCM.** Kalau di-encode, setiap tahap lanjutan harus mendekode lalu
meng-encode ulang seluruh audio, dan pekerjaan encoding berlipat.

**Ukur, jangan perkirakan.** Kalau kamu mengubah jalur audio atau video, sertakan angka sebelum
dan sesudahnya. `scripts/verify_output.py` dan `scripts/bench/` ada untuk itu.

**Komentar menjelaskan alasan, bukan mengulang kode.** Bagian yang tampak janggal biasanya
disengaja karena suatu keterbatasan; jelaskan alasannya, bukan apa yang baris itu lakukan.

## Gaya kode

- Rust: ikuti `cargo fmt` dan pastikan `cargo clippy` bersih tanpa peringatan.
- Frontend: Svelte 5 dengan runes (`$state`, `$derived`, `$props`).
- Komentar, nama, dan teks antarmuka memakai bahasa Indonesia, mengikuti kode yang sudah ada.

## Mengirim perubahan

1. Buat cabang dari `main`: `git checkout -b perbaikan/singkat-jelas`
2. Tulis pesan commit yang menjelaskan **mengapa**, bukan hanya apa.
3. Pastikan `cargo test`, `clippy`, dan `fmt` lulus.
4. Buka pull request dan jelaskan cara mengujinya.

## Melaporkan masalah

Sertakan:

- Versi LofiLoop dan Windows.
- Perintah atau langkah yang dijalankan.
- Apa yang diharapkan dan apa yang terjadi.
- Untuk masalah audio/video: jenis berkas, durasi, dan jumlah lagunya.

Kalau masalahnya soal hasil yang terasa aneh di sambungan, jalankan dulu
`python scripts/verify_output.py <berkas>` dan sertakan angkanya.
