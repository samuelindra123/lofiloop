/**
 * Penghitung waktu berjalan untuk pekerjaan yang sedang berlangsung.
 *
 * Angka yang ditampilkan sengaja dihitung dari `performance.now()` dan bukan dengan
 * menambah satu setiap detik. `setInterval` di jendela yang tidak aktif bisa tertunda
 * atau tertahan; kalau nilainya dihitung dari selisih waktu nyata, tampilan tetap benar
 * setelah jendela kembali aktif.
 *
 * Dipakai sebagai `$state` biasa dari dalam komponen:
 *
 *     let detik = 0;
 *     const timer = buatTimer((s) => (detik = s));
 *     timer.mulai();
 *     timer.henti();
 */
export function buatTimer(onTick) {
  let mulai = 0;
  let handle = null;

  function tick() {
    onTick((performance.now() - mulai) / 1000);
  }

  return {
    /** Mulai dari nol. Kalau sudah berjalan, hitungannya diulang dari awal. */
    mulai() {
      this.henti();
      mulai = performance.now();
      onTick(0);
      handle = setInterval(tick, 200);
    },
    /** Hentikan hitungan. Aman dipanggil walau timer tidak sedang berjalan. */
    henti() {
      if (handle !== null) {
        clearInterval(handle);
        handle = null;
      }
    },
  };
}

/** Ubah detik menjadi bentuk jam:menit:detik, atau menit:detik kalau di bawah satu jam. */
export function fmtHMS(detik) {
  const total = Math.max(0, Math.floor(detik));
  const j = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const d = total % 60;
  const dua = (n) => String(n).padStart(2, "0");
  return j > 0 ? `${j}:${dua(m)}:${dua(d)}` : `${m}:${dua(d)}`;
}
