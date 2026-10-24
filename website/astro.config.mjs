// Konfigurasi Astro untuk situs LofiLoop.
//
// Situs ini murni statis: tidak ada fungsi server, tidak ada permintaan jaringan saat
// halaman dibuka. Data versi dibekukan menjadi berkas JSON saat build (lihat
// skrip/ambil-versi.mjs), sehingga pengunjung tidak pernah menghubungi API GitHub dan
// tidak ikut terkena batas permintaan per alamat IP.
import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";

// Alamat akhir situs. Dipakai untuk peta situs dan alamat kanonik.
// Di Vercel, nilainya bisa ditimpa lewat variabel lingkungan SITUS_URL.
const SITUS = process.env.SITUS_URL ?? "https://lofiloop.vercel.app";

export default defineConfig({
  site: SITUS,
  integrations: [sitemap()],
  vite: {
    plugins: [tailwindcss()],
  },
  // Halaman dibangun sebagai berkas statis, bukan dirender saat diminta.
  output: "static",
  build: {
    // Satu berkas CSS untuk seluruh situs. Halaman dokumentasi jarang berubah, jadi satu
    // berkas bersama lebih menguntungkan daripada memecahnya: sekali diambil, seluruh
    // halaman berikutnya tidak perlu mengunduh gaya lagi.
    inlineStylesheets: "never",
  },
  markdown: {
    shikiConfig: {
      // Tema sorotan kode mengikuti tema situs, bukan tema bawaan.
      themes: { light: "github-light", dark: "github-dark-dimmed" },
    },
  },
});
