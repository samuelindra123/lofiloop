#!/usr/bin/env node
/**
 * Memeriksa tautan internal pada hasil build, dan memastikan tidak ada permintaan
 * ke API GitHub dari sisi pengunjung.
 *
 * Dua hal yang diperiksa, keduanya menangkap kesalahan yang tidak terlihat mata:
 *
 * 1. Tautan internal yang menuju halaman tidak ada. Halaman yang menautkan ke
 *    alamat yang belum dibangun akan terlihat baik-baik saja sampai ada yang
 *    mengkliknya.
 *
 * 2. Permintaan ke api.github.com yang ikut terbawa ke berkas hasil. Ini penting
 *    karena batas permintaan GitHub hanya 60 per jam per alamat IP tanpa token.
 *    Kalau situs memanggilnya dari sisi pengunjung, seluruh pengunjung dari satu
 *    jaringan bisa saling menghabiskan jatah, dan data versinya akan kosong.
 *
 * Dijalankan setelah `pnpm run build`, karena yang diperiksa adalah berkas hasil,
 * bukan berkas sumber.
 */
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, resolve, dirname, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const DIREKTORI_INI = dirname(fileURLToPath(import.meta.url));
const AKAR_SITUS = resolve(DIREKTORI_INI, "..");
const DIST = join(AKAR_SITUS, "dist");

if (!existsSync(DIST)) {
  console.error("Folder dist belum ada. Jalankan: pnpm run build");
  process.exit(1);
}

/** Kumpulkan seluruh berkas di dalam sebuah folder, secara menurun. */
function kumpulkanBerkas(akar, hasil = []) {
  for (const nama of readdirSync(akar)) {
    const jalur = join(akar, nama);
    if (statSync(jalur).isDirectory()) {
      kumpulkanBerkas(jalur, hasil);
    } else {
      hasil.push(jalur);
    }
  }
  return hasil;
}

const berkasSemua = kumpulkanBerkas(DIST);
const berkasHtml = berkasSemua.filter((f) => f.endsWith(".html"));

/** Ubah jalur berkas hasil menjadi alamat URL, untuk mencocokkan tautan. */
function alamatDari(namaBerkas) {
  const relatif = relative(DIST, namaBerkas).split(sep).join("/");
  if (relatif === "index.html") return "/";
  if (relatif.endsWith("/index.html")) return "/" + relatif.slice(0, -"index.html".length);
  return "/" + relatif;
}

const alamatAda = new Set(berkasSemua.map(alamatDari));
// Halaman yang dibangun sebagai /nama/index.html juga bisa diakses lewat /nama.
for (const alamat of [...alamatAda]) {
  if (alamat.endsWith("/") && alamat !== "/") alamatAda.add(alamat.slice(0, -1));
}

const masalahTautan = [];
let jumlahDiperiksa = 0;

for (const berkas of berkasHtml) {
  const isi = readFileSync(berkas, "utf8");
  const halaman = alamatDari(berkas);

  // Ambil seluruh href dan src, lalu saring yang internal saja.
  for (const m of isi.matchAll(/(?:href|src)="([^"]*)"/g)) {
    const mentah = m[1];

    if (!mentah || mentah.startsWith("#") || mentah.startsWith("mailto:")) continue;
    if (/^[a-z]+:/i.test(mentah)) continue; // tautan luar, tidak diperiksa

    // Tautan dalam halaman dengan bagian, misalnya /panduan#langkah.
    const tanpaBagian = mentah.split("#")[0];
    if (!tanpaBagian) continue;

    jumlahDiperiksa++;

    // Normalkan: buang query, pastikan diawali garis miring.
    const bersih = tanpaBagian.split("?")[0];
    const calon = bersih.startsWith("/")
      ? bersih
      : "/" + join(dirname(halaman), bersih).split(sep).join("/");

    // Folder dianggap ada kalau ada index.html di dalamnya.
    const cocok =
      alamatAda.has(calon) ||
      alamatAda.has(calon.replace(/\/$/, "")) ||
      alamatAda.has(calon.endsWith("/") ? calon : calon + "/");

    if (!cocok) {
      masalahTautan.push(`${halaman} menautkan ke ${mentah}, yang tidak ada di hasil build`);
    }
  }
}

// Permintaan ke GitHub dari sisi pengunjung.
const masalahJaringan = [];
for (const berkas of berkasSemua) {
  // Berkas data versi memang memuat alamat github.com untuk tautan unduhan, dan itu
  // wajar. Yang dicari di sini adalah pemanggilan API, bukan tautan biasa.
  if (berkas.endsWith(".json")) continue;

  if (!/\.(js|mjs|html)$/.test(berkas)) continue;

  const isi = readFileSync(berkas, "utf8");
  const relatif = relative(DIST, berkas).split(sep).join("/");

  for (const pola of ["api.github.com", "api.githubusercontent"]) {
    if (isi.includes(pola)) {
      masalahJaringan.push(`${relatif} memuat panggilan ke ${pola}`);
    }
  }
}

// Laporkan.
let gagal = false;

if (masalahTautan.length > 0) {
  gagal = true;
  console.error(`Tautan internal yang menuju halaman tidak ada (${masalahTautan.length}):`);
  for (const m of masalahTautan) console.error(`  - ${m}`);
}

if (masalahJaringan.length > 0) {
  gagal = true;
  console.error(`Panggilan ke API GitHub ikut terbawa ke hasil build (${masalahJaringan.length}):`);
  for (const m of masalahJaringan) console.error(`  - ${m}`);
  console.error(
    "\nSitus tidak boleh memanggil API GitHub dari sisi pengunjung. Batasnya 60 permintaan\n" +
      "per jam per alamat IP tanpa token, dan menghabiskannya akan mengosongkan data versi\n" +
      "untuk semua orang di jaringan yang sama. Ambil datanya saat build, bukan saat dibuka.",
  );
}

if (gagal) process.exit(1);

console.log(
  `tautan internal: ${jumlahDiperiksa} diperiksa di ${berkasHtml.length} halaman, semuanya menuju halaman yang ada`,
);
console.log("jaringan: tidak ada panggilan ke API GitHub di hasil build");
