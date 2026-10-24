# DESIGN.md — LofiLoop

Arah desain untuk situs LofiLoop. Berkas ini ditulis dari jawaban pemilik produk, bukan
disusun sendiri oleh agen. Setiap keputusan di bawah punya alasan yang bisa dibaca dalam
satu baris (R-31), dan setiap teknik dipakai untuk melayani sesuatu, bukan sebagai bawaan.

## Design Read

> Membaca ini sebagai: situs dokumentasi untuk pengguna alat audio dan video desktop, dengan
> bahasa visual instrumen studio pada malam hari, dial **ENERGY 2 / RHYTHM 2 / MOTION 1**.

## Identitas

**LofiLoop adalah alat studio, bukan layanan.** Ia hidup di laptop kelas bawah, memproses
berkas 8 jam dalam gelap, dan tidak butuh perhatian. Situsnya harus terasa seperti itu:
tenang, presisi, dan tidak mengemis perhatian.

Tiga kata yang mengikat setiap keputusan:

**Tenang.** Tidak ada yang berteriak. Tidak ada gradien menyala, tidak ada tombol berpendar,
tidak ada teks besar yang mendesak. Ruang kosong dipakai sebagai pemisah, bukan sebagai sisa.

**Hangat.** Ini produk lofi. Warnanya diambil dari benda nyata yang hangat: kopi, kuningan
tua, tanah liat bakar. Bukan dari spektrum digital.

**Presisi.** Ini alat. Angka ditata rapi dalam kolom, sejajar, bisa dibandingkan mata.
Bagian yang mengukur sesuatu terlihat seperti bagian yang mengukur sesuatu.

## Dial

| Dial | Nilai | Alasan |
|---|---|---|
| **ENERGY** | 2 | Judul tegas dan yakin, tanpa berteriak. Produk ini untuk orang yang tahu apa yang mereka cari, bukan untuk meyakinkan orang lewat. |
| **RHYTHM** | 2 | Tata letak konsisten dengan beberapa bagian yang sengaja dipecah: bagian angka dan tangkapan layar memakai lebar penuh, bagian teks tetap sempit dan terbaca. |
| **MOTION** | 1 | Hanya keadaan hover dan transisi halus. Tidak ada animasi yang berjalan sendiri. Alasannya: halaman ini dibaca sambil bekerja, dan gerakan yang tidak diminta adalah gangguan. |

## Warna

Diambil dari objek hangat, bukan dari spektrum layar. Rasio kontras dihitung memakai
pemeriksa kontras WCAG, bukan dikira (R-25). Setiap nilai di bawah adalah hasil perhitungan
yang sudah dijalankan, bukan perkiraan.

Catatan kejujuran: beberapa nilai pada versi pertama berkas ini gagal saat dihitung, dan
semuanya sudah dinaikkan sampai lolos. Kegagalan itu terjadi dua kali, dan yang kedua baru
tertangkap setelah pemeriksaannya diperluas.

Pertama, mengukur terhadap latar utama saja tidak cukup. `--tulang-samar` lolos di latar
utama tetapi gagal di permukaan terangkat, karena warna itu juga dipakai di sana untuk kode
sebaris. Kedua, teks di atas tombol utama memakai warna aksen sebagai latar, dan pasangan itu
juga harus diukur terpisah.

Karena itu pemeriksaan kontras sekarang mengukur setiap warna teks terhadap **setiap**
permukaan, ditambah pasangan tombol dan blok kode. Ada 34 pasangan di kedua tema, dan
seluruhnya diperiksa otomatis setiap kali dijalankan. Skripnya
`skrip-uji/periksa-kontras.mjs`, dan ia gagal kalau ada satu pasangan di bawah ambang.

### Palet gelap (bawaan)

| Nama | Nilai | Fungsi |
|---|---|---|
| `--arang` | `#17140F` | Latar utama. Arang hangat, bukan hitam pekat. |
| `--arang-2` | `#1F1B15` | Permukaan kartu dan panel, satu langkah di atas latar. |
| `--arang-3` | `#2A241C` | Permukaan terangkat, dipakai pada elemen yang memang perlu menonjol. |
| `--garis` | `#332C23` | Garis pemisah. Samar, hadir hanya saat dicari. |
| `--tulang` | `#EDE6DA` | Teks utama. Gading hangat, rasio **14,81:1** pada `--arang`. |
| `--tulang-redup` | `#A99E8C` | Teks sekunder. Rasio **6,96:1**, lolos AA untuk teks normal. |
| `--tulang-samar` | `#9E9686` | Keterangan kecil. Rasio **5,24:1** di permukaan terangkat, **6,26:1** di latar. |
| `--kuningan` | `#C9964A` | Satu-satunya aksen. Kuningan tua. Rasio **6,95:1**. |
| `--tanah` | `#CC7F55` | Aksen keadaan gagal atau perhatian. Rasio **4,92:1** di permukaan terangkat. |

Kuningan dan tanah dipakai untuk garis tepi, penanda, dan teks berukuran kecil sekaligus,
jadi keduanya diukur terhadap ambang teks normal (4,5:1), bukan ambang teks besar. Mengukur
terhadap ambang yang lebih longgar akan membuatnya gagal begitu dipakai pada teks kecil.

**Teks di atas tombol utama** memakai token tersendiri, `--di-atas-aksen`, karena warna
aksennya berbeda jauh antara kedua tema. Di tema gelap kuningannya terang sehingga teksnya
gelap; di tema terang kuningannya gelap sehingga teksnya harus terang. Memakai satu warna
teks keras untuk keduanya pernah menghasilkan rasio 3,42:1 di tema terang.

**Alasan aksen kuningan:** kuningan adalah logam pada alat studio nyata, dan warnanya sudah
redup sejak awal sehingga tidak pernah terasa menyala. Ia muncul di **satu tempat per layar**:
tindakan utama, penanda bagian aktif, atau satu angka yang penting. Tidak pernah di tombol,
ikon, garis, latar, dan lencana sekaligus (R-13).

**Yang dilarang:** putih menyilaukan (`#FFFFFF` sebagai latar), gradasi ungu ke biru,
gradasi apa pun sebagai warna utama, dan warna neon (R-01).

### Palet terang

Situs ini menyediakan pilihan tema, jadi **kedua tema harus bekerja penuh** (R-34). Tema
terang bukan versi pucat dari tema gelap, melainkan arah yang sama pada kertas hangat.

| Nama | Nilai | Fungsi |
|---|---|---|
| `--arang` | `#F7F2E9` | Latar utama. Kertas hangat, bukan putih bersih. |
| `--arang-2` | `#FFFCF6` | Permukaan kartu. |
| `--arang-3` | `#EDE5D7` | Permukaan terangkat. |
| `--garis` | `#DCD2C0` | Garis pemisah. |
| `--tulang` | `#241F18` | Teks utama. Rasio **14,66:1**. |
| `--tulang-redup` | `#5A5145` | Teks sekunder. Rasio **6,98:1**. |
| `--tulang-samar` | `#6E6558` | Keterangan kecil. Rasio **5,14:1**. |
| `--kuningan` | `#75531A` | Aksen. Rasio **5,58:1** di permukaan terangkat. |
| `--tanah` | `#96432A` | Aksen keadaan perhatian. Rasio **6,01:1**. |

## Huruf

Dua peran, dua huruf, masing-masing dengan alasan (R-06).

**Judul: Fraunces.** Huruf serif dengan karakter kuat dan sedikit kuno. Dipilih bukan karena
populer, melainkan karena dua hal pada dirinya sesuai produk: ia punya nada hangat dan
sedikit retro yang cocok dengan lofi, dan ia tegas pada ukuran besar sehingga judul tidak
perlu dibesarkan berlebihan untuk terasa penting. Dipakai **hanya untuk judul**. Tidak untuk
teks isi, karena pada ukuran kecil karakternya melelahkan dibaca.

**Isi: Inter.** Netral dan sangat terbaca pada ukuran kecil. Dipilih karena tugasnya memang
tidak boleh terasa: teks dokumentasi harus hilang dari perhatian pembaca. Ini termasuk huruf
yang sering muncul sebagai bawaan AI, dan itu diterima dengan sadar, karena alasan memakainya
di sini adalah keterbacaan, bukan tren (R-06 memperbolehkan selama alasannya ditulis).

**Angka dan kode: JetBrains Mono.** Dipakai untuk **segala yang berupa angka atau kode**:
durasi, jumlah frame, ukuran berkas, waktu proses, dan blok perintah. Alasannya bukan
estetika terminal, melainkan karena angka di situs ini memang dibandingkan: durasi 1 jam
dengan 3 jam, ukuran 58,9 MB dengan 81,6 MB. Huruf monospace menjaga angkanya sejajar dalam
kolom sehingga bisa dibandingkan sekilas. Untuk itu `font-variant-numeric: tabular-nums`
selalu dipasang.

Yang dilarang: huruf monospace untuk judul, dan label HURUF BESAR dengan jarak antar-huruf
lebar (R-06).

## Bentuk dan ruang

**Radius.** Empat nilai, masing-masing punya arti:

| Nilai | Dipakai pada |
|---|---|
| `2px` | Lencana dan tanda kecil. Nyaris siku, supaya tidak terlihat seperti pil. |
| `6px` | Tombol, kolom isian, dan blok kode. |
| `10px` | Kartu dan panel. |
| `999px` | Hanya untuk titik penanda keadaan. Tidak pernah untuk tombol. |

Ini mencegah kegagalan "semuanya berbentuk pil" (R-11). Tombol siku menandakan alat; tombol
pil menandakan aplikasi konsumen.

**Bayangan.** Nyaris tidak dipakai. Hanya satu tingkat bayangan untuk panel yang benar-benar
mengambang di atas isi, seperti daftar isi yang menempel saat digulir. Alasannya: alat studio
tidak mengambang, ia duduk di meja (R-12).

**Cahaya.** Tidak ada pendaran (`glow`) sama sekali, kecuali satu: cincin fokus pada elemen
yang sedang disorot papan tik. Cincin itu bukan hiasan, melainkan penanda keadaan nyata
(R-13, R-32).

**Garis.** Pemisah memakai garis satu piksel berwarna `--garis`, bukan bayangan. Ini pilihan
yang lebih tenang dan lebih sesuai alat.

## Tata letak

**Lebar isi: 68 karakter.** Teks isi dibatasi agar baris tidak terlalu panjang untuk dibaca.
Halaman dokumen memakai dua kolom: bilah samping navigasi, lalu isi.

**Susunan berulang.** Setiap halaman memakai kerangka sama, dan itu disengaja: ini situs
dokumentasi, dan pembaca perlu tahu di mana menemukan navigasi dan di mana menemukan isi.
Yang **bervariasi** adalah susunan di dalam isi: bagian angka memakai lebar penuh, bagian
prosa sempit, bagian perintah memakai blok kode selebar isi. RHYTHM 2 berarti: kerangka
konsisten, komposisi bervariasi di dalamnya.

**Kisi angka.** Angka-angka pengukuran ditata sebagai daftar berlabel dalam dua kolom:
keterangan di kiri, angka monospace rata kanan di kanan. Ini bukan tabel dan bukan kartu,
melainkan daftar, karena itulah bentuk yang paling jujur untuk sekumpulan angka sejenis.

**Titik fokus.** Setiap layar punya tepat satu hal yang paling penting. Di beranda: judul
dan satu tindakan unduh. Di halaman rilis: versi terbaru. Di halaman panduan: langkah
sedang dibaca.

Yang dilarang: kisi bento, jendela terminal tiruan, tiga kolom harga, bilah logo "dipercaya
oleh", dan bilah berwarna di tepi kiri kartu (R-05).

## Gerak

MOTION 1. Yang ada hanya:

- Transisi warna dan garis batas saat hover, 120ms.
- Bilah kemajuan yang benar-benar bergerak karena prosesnya berjalan, bukan hiasan.
- Munculnya daftar isi saat halaman dibaca, sekali, tidak berulang.

Yang tidak ada: elemen yang berdenyut terus-menerus, teks yang muncul satu per satu saat
digulir, gambar yang mengapung, dan apa pun yang berjalan tanpa dipicu pengguna (R-19).

`prefers-reduced-motion` dihormati di seluruh situs. Kalau pengguna mematikannya, semua
transisi menjadi 0ms.

## Suara penulisan

- Bahasa Indonesia, tenang, langsung. Kalimat pendek lebih baik daripada panjang.
- Menyebut angka nyata, bukan kata sifat. "1 jam video jadi dalam 104 detik", bukan
  "pemrosesan sangat cepat".
- Tidak memakai kata pengisi gaya pemasaran: revolusioner, mulus, canggih, terdepan,
  tak tertandingi (R-16).
- Tidak memakai tanda pisah panjang (R-02). Memakai koma, titik, titik dua, atau kurung.
- Tidak mengarang angka, ulasan, atau nama pengguna (R-17, R-18, R-36, R-38). Kalau sebuah
  angka tidak punya sumber, angka itu tidak ditampilkan.
- Ajakan bertindak menyebut tindakannya: "Unduh untuk Windows", bukan "Mulai sekarang".

## Ikon

Hanya ikon yang benar-benar mewakili isinya (R-04). Tidak ada percikan, bintang, kilat,
berlian, atau bola cahaya. Sebagian besar bagian tidak butuh ikon sama sekali dan memakai
labelnya saja. Ikon yang dipakai digambar sebagai SVG sebaris, bukan diimpor dari pustaka,
supaya goresannya bisa disesuaikan dan tidak ikut membawa gaya pustakanya.

## Keadaan wajib

Setiap bagian yang menampilkan data punya tiga keadaan (R-27):

| Keadaan | Wujudnya |
|---|---|
| Kosong | Menyebut sebabnya dan satu tindakan yang mengisinya. Bukan "tidak ada data". |
| Memuat | Menyebut apa yang sedang diambil. Bukan pemintal tanpa keterangan. |
| Gagal | Menyebut apa yang gagal dan apa yang bisa dilakukan. |

Termasuk untuk halaman rilis: kalau `versi.json` tidak memuat tautan unduhan (misalnya
GitHub tidak bisa dihubungi saat build), halaman tetap menampilkan versinya dan menyebutkan
bahwa tautan unduhan bisa diambil dari halaman rilis.

## Alasan yang ditolak

Tiga teknik yang sengaja **tidak** dipakai, beserta alasannya:

| Teknik | Alasan ditolak |
|---|---|
| Kisi latar bergerak | Menambah tekstur tanpa menambah arti. Produk ini soal audio dan durasi, bukan soal kisi (R-07). |
| Kartu dengan bingkai mengikuti kursor | Gerakan yang tidak diminta pada elemen yang tidak sedang dipakai. Bertentangan dengan MOTION 1 (R-19). |
| Latar gelap berpendar di belakang judul | Itu gradien radial, salah satu pola yang paling sering muncul di halaman buatan AI (R-01). |

Situs ini tetap bisa terasa hidup tanpa ketiganya, karena yang membuatnya hidup adalah
tipografi yang dipilih dengan alasan, angka yang nyata, dan ketenangan yang konsisten.
