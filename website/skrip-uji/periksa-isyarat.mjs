#!/usr/bin/env node
/**
 * Memeriksa isyarat di dalam berkas sumber halaman.
 *
 * Berbeda dari pemeriksa tautan yang bekerja pada hasil build, pemeriksa ini
 * membaca berkas sumbernya langsung. Tujuannya menangkap pola yang mudah masuk
 * tanpa sadar dan sulit terlihat saat membaca sekilas:
 *
 * 1. Tanda pisah panjang, yang dilarang di seluruh tulisan situs ini.
 * 2. Emoji di dalam teks halaman, karena emoji menghiasi tanpa menerangkan.
 * 3. Kata pengisi gaya pemasaran, yang menyatakan manfaat tanpa menyebut angka.
 *
 * Ketiganya bukan soal selera. Yang pertama dan kedua adalah pola yang paling
 * sering muncul di tulisan buatan mesin, dan yang ketiga membuat halaman terdengar
 * menjanjikan tanpa mengatakan apa pun.
 */
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, resolve, dirname, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const DIREKTORI_INI = dirname(fileURLToPath(import.meta.url));
const AKAR_SITUS = resolve(DIREKTORI_INI, "..");

/** Kumpulkan berkas halaman, secara menurun. */
function kumpulkanBerkas(akar, hasil = []) {
  if (!existsSync(akar)) return hasil;
  for (const nama of readdirSync(akar)) {
    const jalur = join(akar, nama);
    if (statSync(jalur).isDirectory()) {
      kumpulkanBerkas(jalur, hasil);
    } else if (/\.(astro|md|mdx)$/.test(nama)) {
      hasil.push(jalur);
    }
  }
  return hasil;
}

const berkasHalaman = [
  ...kumpulkanBerkas(join(AKAR_SITUS, "src", "pages")),
  ...kumpulkanBerkas(join(AKAR_SITUS, "src", "layouts")),
  ...kumpulkanBerkas(join(AKAR_SITUS, "src", "komponen")),
];

const masalah = [];

// Tanda pisah panjang dan variasinya.
//
// Tanda hubung ganda dua spasi sengaja TIDAK ikut diperiksa di sini, karena perintah
// baris perintah banyak yang memakainya sebagai bagian sintaksnya, seperti
// `cargo clippy -- -D warnings`. Pemeriksa ini bekerja per baris dan tidak bisa
// membedakan perintah di dalam tanda kutip dari tulisan biasa, jadi menandainya akan
// menghasilkan positif palsu. Tanda pisah panjang sesungguhnya, yaitu em dash dan en
// dash, tetap diperiksa dan itu bentuk yang paling sering muncul di tulisan mesin.
const POLA_PISAH = /[—–]/;

// Emoji. Rentangnya dibatasi pada blok yang benar-benar dipakai sebagai hiasan,
// supaya simbol seperti panah tidak ikut tertangkap.
const POLA_EMOJI =
  /[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}\u{FE0F}]/u;

// Kata pengisi yang menyatakan manfaat tanpa menyebut angka. Daftar ini sengaja
// pendek dan spesifik, supaya tidak ikut menangkap kata biasa.
const KATA_PENGISI = [
  "revolusioner",
  "terdepan di industri",
  "kelas dunia",
  "tanpa batas",
  "tak tertandingi",
  "sangat canggih",
  "mulus tanpa cela",
  "solusi terbaik",
  "paling mutakhir",
  "mengubah segalanya",
];

for (const berkas of berkasHalaman) {
  const isi = readFileSync(berkas, "utf8");
  const relatif = relative(AKAR_SITUS, berkas).split(sep).join("/");
  const baris = isi.split("\n");

  baris.forEach((teks, i) => {
    const nomor = i + 1;

    if (POLA_PISAH.test(teks)) {
      masalah.push({
        berkas: relatif,
        nomor,
        pesan: "memakai tanda pisah panjang. Ganti dengan koma, titik, titik dua, atau kurung.",
        cuplikan: teks.trim().slice(0, 90),
      });
    }

    if (POLA_EMOJI.test(teks)) {
      masalah.push({
        berkas: relatif,
        nomor,
        pesan: "memuat emoji di dalam teks halaman.",
        cuplikan: teks.trim().slice(0, 90),
      });
    }

    const kecil = teks.toLowerCase();
    for (const kata of KATA_PENGISI) {
      if (kecil.includes(kata)) {
        masalah.push({
          berkas: relatif,
          nomor,
          pesan: `memakai kata pengisi "${kata}". Sebut angkanya, atau hapus klaimnya.`,
          cuplikan: teks.trim().slice(0, 90),
        });
      }
    }
  });
}

if (masalah.length > 0) {
  console.error(`Isyarat yang perlu diperbaiki (${masalah.length}):\n`);
  for (const m of masalah) {
    console.error(`  ${m.berkas}:${m.nomor}`);
    console.error(`    ${m.pesan}`);
    console.error(`    ${m.cuplikan}\n`);
  }
  process.exit(1);
}

console.log(
  `isyarat: ${berkasHalaman.length} berkas diperiksa, tidak ada tanda pisah panjang, emoji, atau kata pengisi`,
);
