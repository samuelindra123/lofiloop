// Pembungkus tipis di atas API Tauri. Dipisah supaya setiap modul lain mengimpor dari sini
// dan tidak ada satu tempat pun yang mengira-ngira apakah jembatan Tauri tersedia.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

/** Apakah kode berjalan di dalam jendela Tauri, bukan di browser biasa. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Panggil perintah backend. Kalau tidak berjalan di Tauri, kembalikan error yang jelas
 * supaya UI bisa menampilkannya alih-alih diam-diam tidak bekerja.
 */
export async function invoke(cmd, args = {}) {
  if (!inTauri) {
    throw new Error(
      "Aplikasi ini harus dijalankan sebagai aplikasi desktop (lofiloop-app.exe), bukan di browser.",
    );
  }
  return tauriInvoke(cmd, args);
}

/** Berlangganan event dari backend; tidak melakukan apa-apa kalau di luar Tauri. */
export async function listen(event, handler) {
  if (!inTauri) return () => {};
  return tauriListen(event, handler);
}

/** Buka dialog berkas. Memakai plugin dialog, dengan penyaring sesuai jenis berkas. */
export async function pickFile(kind) {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const filters =
    kind === "video"
      ? [{ name: "Video", extensions: ["mp4", "mov", "mkv", "webm", "avi", "m4v"] }]
      : [{ name: "Audio", extensions: ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"] }];
  return open({ multiple: false, directory: false, filters });
}

/** Buka dialog berkas dengan pilihan banyak sekaligus. Mengembalikan daftar path. */
export async function pickFiles(kind) {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const filters =
    kind === "video"
      ? [{ name: "Video", extensions: ["mp4", "mov", "mkv", "webm", "avi", "m4v"] }]
      : [{ name: "Audio", extensions: ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"] }];
  const picked = await open({ multiple: true, directory: false, filters });
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}
