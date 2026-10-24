#!/usr/bin/env node
/**
 * Menyusun data versi untuk halaman riwayat rilis.
 *
 * Dua sumber digabung, dan keduanya diperlukan:
 *
 * 1. `CHANGELOG.md` di akar repo. Inilah sumber utama: ia memuat setiap versi, tanggalnya,
 *    dan isi perubahannya dalam bentuk yang sudah ditulis manusia. Berkas ini selalu ada
 *    di repo, jadi skrip tetap bisa berjalan tanpa jaringan.
 * 2. API rilis GitHub, lewat perintah `gh`. Yang diambil hanya tautan unduhan dan ukuran
 *    berkas, karena keduanya tidak ada di changelog.
 *
 * Pemisahan itu disengaja. Changelog memberi kerangka yang pasti, API hanya melengkapinya.
 * Kalau API tidak bisa dihubungi (tidak ada jaringan, atau jatah permintaan habis), skrip
 * tetap menghasilkan berkas yang sah, hanya tanpa tautan unduhan. Build tidak gagal.
 *
 * Jalankan dengan `--offline` untuk melewati GitHub sepenuhnya saat bekerja di lokal.
 */
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const DIREKTORI_INI = dirname(fileURLToPath(import.meta.url));
const AKAR_SITUS = resolve(DIREKTORI_INI, "..");
const AKAR_REPO = resolve(AKAR_SITUS, "..");
const CHANGELOG = join(AKAR_REPO, "CHANGELOG.md");
const KELUARAN = join(AKAR_SITUS, "src", "data", "versi.json");

const REPO = "samuelindra123/lofiloop";
const offline = process.argv.includes("--offline");

/** Judul bagian changelog yang dikenal, dipakai sebagai kunci agar urutannya tetap. */
const JUDUL_BAGIAN = ["Ditambahkan", "Diperbaiki", "Diubah", "Dihapus", "Catatan"];

/**
 * Baca CHANGELOG.md dan kembalikan daftar versi.
 *
 * Bentuk yang diharapkan mengikuti Keep a Changelog:
 *
 *   ## [0.2.3] — 2026-10-23
 *
 *   ### Diperbaiki
 *
 *   - Satu butir perubahan.
 *
 * Butir yang lebih dari satu baris disatukan menjadi satu kalimat, karena tampilan hanya
 * butuh satu baris per perubahan.
 */
function bacaChangelog(teks) {
  const baris = teks.split("\n");
  const hasil = [];

  let sekarang = null;
  let bagian = null;
  let butir = null;

  const tutupButir = () => {
    if (butir && bagian && sekarang) {
      const bersih = butir.replace(/\s+/g, " ").trim();
      if (bersih) bagian.isi.push(bersih);
    }
    butir = null;
  };

  for (const barisMentah of baris) {
    const b = barisMentah.trimEnd();

    // Judul versi: "## [0.2.3] — 2026-10-23" atau "## [0.2.3] - 2026-10-23".
    const judulVersi = /^##\s+\[([^\]]+)\]\s*[—–-]\s*(\d{4}-\d{2}-\d{2})/.exec(b);
    if (judulVersi) {
      tutupButir();
      sekarang = {
        versi: judulVersi[1],
        tag: `v${judulVersi[1]}`,
        tanggal: judulVersi[2],
        bagian: [],
      };
      hasil.push(sekarang);
      bagian = null;
      continue;
    }

    if (!sekarang) continue;

    // Judul bagian: "### Diperbaiki".
    const judulBagian = /^###\s+(.+?)\s*$/.exec(b);
    if (judulBagian) {
      tutupButir();
      const nama = judulBagian[1];
      bagian = { nama, isi: [] };
      sekarang.bagian.push(bagian);
      continue;
    }

    // Butir daftar: "- Sesuatu" atau "* Sesuatu".
    const butirBaru = /^[-*]\s+(.*)$/.exec(b.trim());
    if (butirBaru && bagian) {
      tutupButir();
      butir = butirBaru[1];
      continue;
    }

    // Sambungan baris butir sebelumnya.
    if (butir && b.trim()) {
      butir += ` ${b.trim()}`;
      continue;
    }

    // Baris kosong menutup butir yang sedang berjalan.
    if (!b.trim()) {
      tutupButir();
    }
  }
  tutupButir();

  // Buang bagian yang tidak berisi apa-apa, lalu urutkan bagian sesuai urutan yang dikenal
  // supaya tampilan tiap versi konsisten.
  for (const v of hasil) {
    v.bagian = v.bagian
      .filter((bagian) => bagian.isi.length > 0)
      .sort((a, b) => {
        const ia = JUDUL_BAGIAN.indexOf(a.nama);
        const ib = JUDUL_BAGIAN.indexOf(b.nama);
        return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib);
      });
  }

  return hasil;
}

/**
 * Ambil daftar rilis beserta lampirannya dari GitHub.
 *
 * memakai `gh` dan bukan permintaan HTTP langsung, karena hanya dengan begitu token yang
 * sudah ada di mesin ini atau di CI ikut terpakai. Tanpa token, batasnya hanya 60 permintaan
 * per jam per alamat IP, dan itu mudah habis. Dengan token, batasnya 5000.
 *
 * Mengembalikan peta tag ke daftar berkas. Kegagalan apa pun dikembalikan sebagai peta
 * kosong, bukan dilempar, supaya build tetap berjalan.
 */
function ambilRilis() {
  if (offline) {
    console.log("[versi] mode offline: GitHub dilewati");
    return new Map();
  }

  let keluaran;
  try {
    keluaran = execFileSync(
      "gh",
      ["api", `repos/${REPO}/releases`, "--paginate"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
  } catch (galat) {
    // Alasan paling sering: `gh` tidak terpasang, belum masuk, atau jatah permintaan habis.
    console.warn(`[versi] GitHub tidak bisa dihubungi, tautan unduhan dilewati: ${galat.message}`);
    return new Map();
  }

  let daftar;
  try {
    daftar = JSON.parse(keluaran);
  } catch {
    console.warn("[versi] jawaban GitHub tidak terbaca, tautan unduhan dilewati");
    return new Map();
  }

  if (!Array.isArray(daftar)) {
    console.warn("[versi] bentuk jawaban GitHub tidak seperti yang diharapkan");
    return new Map();
  }

  const peta = new Map();
  for (const rilis of daftar) {
    if (!rilis || typeof rilis.tag_name !== "string") continue;
    const berkas = Array.isArray(rilis.assets)
      ? rilis.assets
          .filter((a) => a && typeof a.name === "string" && typeof a.browser_download_url === "string")
          .map((a) => ({
            nama: a.name,
            ukuran: typeof a.size === "number" ? a.size : 0,
            tautan: a.browser_download_url,
          }))
      : [];
    peta.set(rilis.tag_name, {
      tautan: typeof rilis.html_url === "string" ? rilis.html_url : "",
      berkas,
    });
  }
  return peta;
}

/**
 * Rapikan teks butir supaya pantas ditampilkan di halaman.
 *
 * Dua hal yang dikerjakan, dan keduanya sengaja dilakukan di sini, bukan di dalam
 * CHANGELOG.md:
 *
 * 1. Tanda pisah panjang diganti. CHANGELOG memakainya di beberapa tempat, dan
 *    halaman situs tidak boleh menampilkannya. Menggantinya di sini berarti berkas
 *    changelog tetap utuh dan tetap sesuai format Keep a Changelog, sementara yang
 *    dibaca pengunjung sudah bersih.
 * 2. Spasi berlebih disatukan, karena butir yang ditulis lebih dari satu baris
 *    membawa indentasi dari berkas asalnya.
 *
 * Tanda tebal sengaja DIBIARKAN di sini, karena halaman yang menampilkannya perlu
 * tahu bagian mana yang harus ditebalkan, dan itu diurus di sisi tampilan.
 */
function rapikanUntukTampilan(teks) {
  return teks
    // Em dash dan en dash diganti koma. Koma dipilih, bukan titik, karena tanda itu
    // umumnya dipakai sebagai sisipan di tengah kalimat, bukan sebagai pemisah
    // kalimat baru.
    .replace(/\s+[—–]\s+/g, ", ")
    .replace(/[—–]/g, ", ")
    // Tanda hubung ganda sebagai pengganti tanda pisah, hanya kalau diapit spasi.
    .replace(/\s+--\s+/g, ", ")
    .replace(/\s+/g, " ")
    .trim();
}

function main() {
  if (!existsSync(CHANGELOG)) {
    console.error(`[versi] CHANGELOG.md tidak ditemukan di ${CHANGELOG}`);
    process.exit(1);
  }

  const teks = readFileSync(CHANGELOG, "utf8");
  const versi = bacaChangelog(teks);

  if (versi.length === 0) {
    console.error("[versi] tidak ada versi yang terbaca dari CHANGELOG.md");
    process.exit(1);
  }

  // Rapikan setiap butir untuk ditampilkan. CHANGELOG.md sendiri tidak disentuh.
  for (const v of versi) {
    for (const bagian of v.bagian) {
      bagian.isi = bagian.isi.map(rapikanUntukTampilan);
    }
  }

  const rilis = ambilRilis();

  for (const v of versi) {
    const r = rilis.get(v.tag);
    v.tautan_rilis = r?.tautan || `https://github.com/${REPO}/releases/tag/${v.tag}`;
    v.berkas = r?.berkas ?? [];
  }

  const data = {
    dibuat_pada: new Date().toISOString(),
    sumber: offline ? "changelog" : "changelog+github",
    terbaru: versi[0].versi,
    versi,
  };

  mkdirSync(dirname(KELUARAN), { recursive: true });
  writeFileSync(KELUARAN, JSON.stringify(data, null, 2) + "\n", "utf8");

  const denganBerkas = versi.filter((v) => v.berkas.length > 0).length;
  console.log(
    `[versi] ${versi.length} versi ditulis ke src/data/versi.json (terbaru ${data.terbaru}, ` +
      `${denganBerkas} versi punya tautan unduhan)`,
  );
}

main();
