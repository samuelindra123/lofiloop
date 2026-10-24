#!/usr/bin/env node
/**
 * Mengambil tangkapan layar tiap langkah dari rekaman demo.
 *
 * Detiknya TIDAK ditulis di sini. Nilainya dibaca dari kolom "waktu" pada
 * src/data/langkah.json, sehingga gambar dan keterangan waktunya tidak bisa lagi
 * berbeda: mengubah satu baris di data ikut mengubah gambar yang diambil.
 *
 * Rekaman direkam dari seluruh layar pada 1280x720, jadi frame-nya dipakai utuh:
 * tidak ada pemangkasan, supaya bilah judul dan lambang aplikasi tetap terlihat.
 * Yang dilakukan hanya memperkecil ke lebar 1000px supaya berkasnya ringan.
 *
 * Cara pakai:
 *     node skrip/ambil-gambar-langkah.mjs
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, existsSync, readFileSync } from "node:fs";
import { basename, join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const DIREKTORI_INI = dirname(fileURLToPath(import.meta.url));
const AKAR_SITUS = resolve(DIREKTORI_INI, "..");
const KELUARAN = join(AKAR_SITUS, "public", "gambar");
const DATA = join(AKAR_SITUS, "src", "data", "langkah.json");

// Rekaman aslinya sengaja disimpan di luar folder situs supaya tidak ikut ter-deploy.
const REKAMAN = process.env.REKAMAN ?? resolve(AKAR_SITUS, "..", "rekaman-asli", "Untitled project.mp4");

// Melebar 1000px: cukup tajam untuk membaca tulisan antarmuka di layar besar,
// dan tetap ringan karena sepuluhnya dipakai di satu halaman.
const FILTER = "scale=1000:-2";

/** Ubah "m:ss" menjadi detik. */
function keDetik(waktu) {
  const cocok = /^(\d+):([0-5]\d)$/.exec(waktu ?? "");
  if (!cocok) throw new Error(`Waktu tidak berbentuk m:ss: ${JSON.stringify(waktu)}`);
  return Number(cocok[1]) * 60 + Number(cocok[2]);
}

const data = JSON.parse(readFileSync(DATA, "utf8"));
const LANGKAH = data.langkah.map((l, i) => ({
  nomor: i + 1,
  berkas: basename(l.gambar),
  detik: keDetik(l.waktu),
}));

if (!existsSync(REKAMAN)) {
  console.error(`Rekaman tidak ditemukan: ${REKAMAN}`);
  console.error("Letakkan berkasnya di rekaman-asli/, atau setel variabel REKAMAN.");
  process.exit(1);
}

mkdirSync(KELUARAN, { recursive: true });

for (const { nomor, berkas, detik } of LANGKAH) {
  const tujuan = join(KELUARAN, berkas);
  execFileSync(
    "ffmpeg",
    [
      "-v", "error",
      "-ss", String(detik),
      "-i", REKAMAN,
      "-frames:v", "1",
      "-vf", FILTER,
      "-q:v", "4",
      "-y", tujuan,
    ],
    { stdio: "inherit" },
  );
  console.log(`langkah-${nomor} ${berkas} diambil pada detik ${detik} (${data.langkah[nomor - 1].waktu})`);
}

console.log(`\nSelesai: ${LANGKAH.length} gambar di ${KELUARAN}`);
