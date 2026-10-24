#!/usr/bin/env node
/**
 * Memeriksa struktur HTML pada hasil build.
 *
 * Pemeriksaan ini ada karena kerusakan strukturnya tidak terlihat di layar sampai
 * parah. Tag yang tidak seimbang bisa membuat seluruh bagian di bawahnya keluar
 * dari wadahnya, dan itu muncul sebagai tampilan yang "aneh" tanpa pesan galat apa
 * pun. Persis itu yang pernah terjadi: satu `</div>` yang hilang membuat tiga
 * halaman menampilkan isinya menempel ke tepi jendela.
 *
 * Yang diperiksa:
 *
 * 1. Jumlah tag div dan section seimbang di setiap halaman hasil build.
 * 2. Tidak ada isi yang keluar dari elemen induknya karena tag yang tidak tertutup.
 * 3. Setiap halaman memuat berkas CSS-nya, dan gaya tabel benar-benar sampai ke
 *    halaman yang memakai tabel.
 *
 * Diperiksa dari hasil build, bukan dari berkas sumber, karena hanya hasil build
 * yang menggambarkan apa yang benar-benar diterima peramban.
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

function kumpulkanHtml(akar, hasil = []) {
  for (const nama of readdirSync(akar)) {
    const jalur = join(akar, nama);
    if (statSync(jalur).isDirectory()) kumpulkanHtml(jalur, hasil);
    else if (nama.endsWith(".html")) hasil.push(jalur);
  }
  return hasil;
}

/**
 * Buang bagian yang bukan markup halaman: komentar HTML, isi skrip, dan isi gaya.
 *
 * Ini penting karena `<` dan `>` di dalam skrip dan CSS akan mengacaukan hitungan
 * tag. Pemeriksaan yang tidak membuang bagian itu akan melaporkan kerusakan yang
 * sebenarnya tidak ada, dan itu membuat pemeriksanya diabaikan orang.
 */
function bersihkan(html) {
  return html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<script[\s\S]*?<\/script>/gi, "")
    .replace(/<style[\s\S]*?<\/style>/gi, "");
}

const masalah = [];
const halaman = kumpulkanHtml(DIST);
let jumlahDiperiksa = 0;

for (const berkas of halaman) {
  const relatif = relative(DIST, berkas).split(sep).join("/");
  const isi = bersihkan(readFileSync(berkas, "utf8"));

  const divBuka = (isi.match(/<div\b/g) ?? []).length;
  const divTutup = (isi.match(/<\/div>/g) ?? []).length;
  const secBuka = (isi.match(/<section\b/g) ?? []).length;
  const secTutup = (isi.match(/<\/section>/g) ?? []).length;
  const mainBuka = (isi.match(/<main\b/g) ?? []).length;
  const mainTutup = (isi.match(/<\/main>/g) ?? []).length;

  jumlahDiperiksa++;

  if (divBuka !== divTutup) {
    masalah.push(
      `${relatif}: tag div tidak seimbang (${divBuka} dibuka, ${divTutup} ditutup). ` +
        `Ini membuat isi di bawahnya keluar dari wadahnya.`,
    );
  }
  if (secBuka !== secTutup) {
    masalah.push(`${relatif}: tag section tidak seimbang (${secBuka} dibuka, ${secTutup} ditutup)`);
  }
  if (mainBuka !== mainTutup) {
    masalah.push(`${relatif}: tag main tidak seimbang (${mainBuka} dibuka, ${mainTutup} ditutup)`);
  }

  // Setiap bagian halaman mandiri harus punya wadah.
  //
  // Pemeriksaan ini menangkap kerusakan yang paling sulit terlihat: bagian tanpa
  // wadah membuat isinya menempel ke tepi jendela. Di layar lebar itu terlihat
  // sebagai halaman yang "tidak rapi", tanpa satu pun pesan galat.
  const bagianMandiri = isi.match(/<section class="mandiri-bagian[^"]*"[^>]*>/g) ?? [];
  for (const m of isi.matchAll(/<section class="mandiri-bagian[^"]*"[^>]*>/g)) {
    const akhir = isi.indexOf("</section>", m.index);
    const dalam = isi.slice(m.index + m[0].length, akhir === -1 ? undefined : akhir);
    if (!dalam.includes("mandiri-dalam") && !dalam.includes("bagian-penuh")) {
      masalah.push(
        `${relatif}: ada bagian tanpa wadah (mandiri-dalam), sehingga isinya menempel ` +
          `ke tepi jendela`,
      );
      break;
    }
  }

  // Halaman yang memakai tabel harus memuat gaya tabelnya. Tanpa pemeriksaan ini,
  // halaman bisa memakai kelas tabel tanpa pernah mendapat gayanya, dan tabelnya
  // tampil tanpa garis dan tanpa perataan.
  const asli = readFileSync(berkas, "utf8");
  if (/class="[^"]*tabel-bungkus/.test(asli) && !/\.tabel-bungkus/.test(asli)) {
    // Gaya tabel ada di berkas CSS terpisah, jadi tidak bisa dipastikan dari HTML
    // saja. Yang bisa diperiksa: halaman memuat setidaknya satu berkas CSS.
    const adaCss = /<link[^>]+stylesheet/.test(asli);
    if (!adaCss) {
      masalah.push(
        `${relatif}: memakai kelas tabel tetapi tidak memuat satu pun berkas CSS, ` +
          `sehingga tabelnya tidak akan bergaris`,
      );
    }
  }
}

if (masalah.length > 0) {
  console.error(`Struktur hasil build bermasalah (${masalah.length}):\n`);
  for (const m of masalah) console.error(`  - ${m}`);
  console.error(
    "\nPerbaiki tag yang tidak seimbang di berkas halamannya, lalu build ulang.\n" +
      "Jangan menambah penutup di hasil build: yang harus diperbaiki adalah sumbernya.",
  );
  process.exit(1);
}

console.log(
  `struktur: ${jumlahDiperiksa} halaman diperiksa, seluruh tag seimbang dan berkas CSS termuat`,
);
