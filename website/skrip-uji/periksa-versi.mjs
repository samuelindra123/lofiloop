#!/usr/bin/env node
/**
 * Memeriksa bahwa `src/data/versi.json` sesuai dengan `CHANGELOG.md`.
 *
 * Gunanya bukan memastikan berkasnya ada, melainkan menangkap keadaan yang paling mudah
 * terjadi: changelog sudah berisi versi baru tetapi data versi belum disusun ulang. Kalau
 * itu terjadi, halaman rilis akan menampilkan versi lama tanpa ada yang memberi tahu.
 */
import { readFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const DIREKTORI_INI = dirname(fileURLToPath(import.meta.url));
const AKAR_SITUS = resolve(DIREKTORI_INI, "..");
const AKAR_REPO = resolve(AKAR_SITUS, "..");
const CHANGELOG = join(AKAR_REPO, "CHANGELOG.md");
const DATA = join(AKAR_SITUS, "src", "data", "versi.json");

const masalah = [];

if (!existsSync(DATA)) {
  console.error("versi.json belum ada. Jalankan: pnpm run ambil-versi");
  process.exit(1);
}

const data = JSON.parse(readFileSync(DATA, "utf8"));
const changelog = readFileSync(CHANGELOG, "utf8");

// Versi yang muncul di judul changelog, dalam urutan kemunculan.
const diChangelog = [...changelog.matchAll(/^##\s+\[([^\]]+)\]/gm)].map((m) => m[1]);

if (diChangelog.length === 0) {
  masalah.push("Changelog tidak memuat satu versi pun. Bentuk judulnya diharapkan: ## [1.2.3] - 2026-01-01");
}

const diData = data.versi.map((v) => v.versi);

// Setiap versi di changelog harus ada di data, dan sebaliknya.
for (const v of diChangelog) {
  if (!diData.includes(v)) masalah.push(`versi ${v} ada di changelog tetapi tidak ada di versi.json`);
}
for (const v of diData) {
  if (!diChangelog.includes(v)) masalah.push(`versi ${v} ada di versi.json tetapi tidak di changelog`);
}

// Urutan harus sama, karena halaman menampilkan versi terbaru lebih dulu.
if (diData.join(",") !== diChangelog.join(",")) {
  masalah.push(`urutan versi berbeda.\n    changelog: ${diChangelog.join(", ")}\n    versi.json: ${diData.join(", ")}`);
}

// Versi teratas harus sama dengan yang tertulis di field terbaru.
if (data.terbaru !== diData[0]) {
  masalah.push(`field "terbaru" berisi ${data.terbaru} tetapi versi teratas adalah ${diData[0]}`);
}

// Setiap versi harus punya tanggal yang berbentuk tanggal.
for (const v of data.versi) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(v.tanggal)) {
    masalah.push(`versi ${v.versi} punya tanggal tidak sah: ${v.tanggal}`);
  }
}

// Tidak boleh ada tanda pisah panjang di teks yang akan ditampilkan.
//
// Pemeriksaan ini ada di sini, bukan di pemeriksa isyarat, karena sumber teksnya
// adalah CHANGELOG.md yang tidak dibaca pemeriksa itu. Tanda pisah di changelog
// dibersihkan skrip ini saat menyusun data, jadi kalau pembersihannya gagal atau
// dilewati, halaman rilis akan menampilkannya. Pemeriksaan ini yang menangkapnya.
const pisah = [];
for (const v of data.versi) {
  for (const bagian of v.bagian) {
    for (const butir of bagian.isi) {
      if (/[—–]/.test(butir)) {
        pisah.push(`versi ${v.versi}, bagian ${bagian.nama}: ${butir.slice(0, 70)}...`);
      }
    }
  }
}
if (pisah.length > 0) {
  masalah.push(
    `ada ${pisah.length} butir yang masih memuat tanda pisah panjang setelah dibersihkan:\n` +
      pisah.map((p) => `      ${p}`).join("\n"),
  );
}

if (masalah.length > 0) {
  console.error("Data versi tidak sesuai changelog:");
  for (const m of masalah) console.error(`  - ${m}`);
  console.error("\nJalankan: pnpm run ambil-versi");
  process.exit(1);
}

console.log(
  `versi.json sesuai changelog: ${diData.length} versi, terbaru ${data.terbaru} (sumber: ${data.sumber})`,
);
