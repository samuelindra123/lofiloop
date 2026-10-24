#!/usr/bin/env node
/**
 * Memeriksa rasio kontras setiap pasangan warna yang benar-benar dipakai.
 *
 * Pemeriksaan ini ada karena kegagalannya tidak terlihat mata. Abu-abu di atas abu-abu
 * yang terlihat "cukup jelas" bisa berada di bawah ambang WCAG, dan itu membuat halaman
 * sulit dibaca oleh sebagian orang tanpa ada yang menyadarinya.
 *
 * Yang diukur adalah pasangan yang benar-benar muncul di halaman, termasuk warna teks
 * di atas setiap permukaan yang ada, bukan hanya di atas latar utama. Mengukur hanya
 * terhadap latar utama pernah membuat satu nilai lolos di pemeriksaan tapi gagal di
 * halaman, karena nilai itu juga dipakai di atas permukaan yang lebih terang.
 *
 * Ambang yang dipakai adalah 4,5:1 untuk seluruh teks, termasuk teks kecil. Ambang
 * 3:1 untuk teks besar sengaja tidak dipakai di sini, karena hampir semua teks di
 * situs ini berukuran normal, dan memakai ambang yang lebih longgar akan meloloskan
 * warna yang gagal begitu dipakai untuk keterangan kecil.
 */
import { readFileSync, existsSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const DIREKTORI_INI = dirname(fileURLToPath(import.meta.url));
const AKAR_SITUS = resolve(DIREKTORI_INI, "..");
const CSS = join(AKAR_SITUS, "src", "gaya", "global.css");

if (!existsSync(CSS)) {
  console.error("global.css tidak ditemukan.");
  process.exit(1);
}

const css = readFileSync(CSS, "utf8");

/** Ambil nilai token dari satu blok tema. */
function tokenTema(namaTema) {
  const pola = new RegExp(`\\[data-tema="${namaTema}"\\]\\s*\\{([\\s\\S]*?)\\n\\}`, "m");
  const blok = pola.exec(css);
  if (!blok) return null;

  const token = {};
  for (const m of blok[1].matchAll(/--([a-z0-9-]+):\s*(#[0-9a-fA-F]{3,8})\s*;/g)) {
    token[m[1]] = m[2];
  }
  return token;
}

/** Rasio kontras WCAG 2.x antara dua warna heksadesimal. */
function rasio(hex1, hex2) {
  const ke = (hex) => {
    let h = hex.replace("#", "");
    if (h.length === 3) h = h.split("").map((c) => c + c).join("");
    return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
  };

  const luminansi = (hex) => {
    const [r, g, b] = ke(hex).map((c) =>
      c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4),
    );
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };

  const a = luminansi(hex1);
  const b = luminansi(hex2);
  const terang = Math.max(a, b);
  const gelap = Math.min(a, b);
  return (terang + 0.05) / (gelap + 0.05);
}

// Pasangan yang diukur. Daftar ini harus mencakup setiap kombinasi teks dan latar
// yang benar-benar muncul di halaman. Kalau ada warna teks baru yang dipakai di atas
// permukaan baru, pasangannya harus ditambahkan di sini.
const PERMUKAAN = ["arang", "arang-2", "arang-3"];
const TEKS = ["tulang", "tulang-redup", "tulang-samar", "kuningan", "tanah"];

const masalah = [];
let jumlahDiperiksa = 0;
const AMBANG = 4.5;

for (const tema of ["gelap", "terang"]) {
  const token = tokenTema(tema);
  if (!token) {
    masalah.push(`Token tema "${tema}" tidak terbaca dari global.css`);
    continue;
  }

  // Setiap warna teks di atas setiap permukaan.
  for (const namaTeks of TEKS) {
    for (const namaLatar of PERMUKAAN) {
      const warnaTeks = token[namaTeks];
      const warnaLatar = token[namaLatar];
      if (!warnaTeks || !warnaLatar) {
        masalah.push(`Tema ${tema}: token --${namaTeks} atau --${namaLatar} tidak ada`);
        continue;
      }

      jumlahDiperiksa++;
      const r = rasio(warnaTeks, warnaLatar);
      if (r < AMBANG) {
        masalah.push(
          `Tema ${tema}: --${namaTeks} (${warnaTeks}) di atas --${namaLatar} (${warnaLatar}) ` +
            `hanya ${r.toFixed(2)}:1, di bawah ${AMBANG}:1`,
        );
      }
    }
  }

  // Teks di atas tombol utama, yang latarnya adalah warna aksen, bukan permukaan biasa.
  const diAtasAksen = token["di-atas-aksen"];
  const aksen = token["kuningan"];
  if (diAtasAksen && aksen) {
    jumlahDiperiksa++;
    const r = rasio(diAtasAksen, aksen);
    if (r < AMBANG) {
      masalah.push(
        `Tema ${tema}: teks tombol utama (${diAtasAksen}) di atas --kuningan (${aksen}) ` +
          `hanya ${r.toFixed(2)}:1, di bawah ${AMBANG}:1`,
      );
    }
  } else {
    masalah.push(`Tema ${tema}: token --di-atas-aksen atau --kuningan tidak ada`);
  }

  // Kode sebaris dan blok kode memakai latar sendiri.
  const kodeLatar = token["kode-latar"];
  if (kodeLatar && token["tulang"]) {
    jumlahDiperiksa++;
    const r = rasio(token["tulang"], kodeLatar);
    if (r < AMBANG) {
      masalah.push(
        `Tema ${tema}: teks kode di atas --kode-latar hanya ${r.toFixed(2)}:1, ` +
          `di bawah ${AMBANG}:1`,
      );
    }
  }
}

if (masalah.length > 0) {
  console.error(`Pasangan warna yang gagal ambang kontras (${masalah.length}):\n`);
  for (const m of masalah) console.error(`  - ${m}`);
  console.error(
    `\nAmbangnya ${AMBANG}:1 untuk seluruh teks, termasuk teks kecil. Jangan menurunkan\n` +
      "ambangnya untuk meloloskan sebuah warna: naikkan atau gelapkan warnanya, lalu ukur ulang.",
  );
  process.exit(1);
}

console.log(
  `kontras: ${jumlahDiperiksa} pasangan warna diukur pada kedua tema, semuanya lolos ${AMBANG}:1`,
);
