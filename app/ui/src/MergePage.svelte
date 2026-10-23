<script>
  import { onMount } from "svelte";
  import { invoke, listen, pickFiles } from "./bridge.js";

  let { onBusyChange } = $props();

  let tracks = $state([]);
  let crossfadeS = $state(3);
  let normalize = $state(true);
  let bitrate = $state("192k");
  let busy = $state(false);
  let log = $state([]);
  let report = $state(null);
  let error = $state(null);
  let progress = $state({ text: "", fraction: null });
  let reading = $state(false);
  let choosingFolder = $state(false);
  let lastMessageAt = $state(0);
  let nowTick = $state(Date.now());

  // Penunjuk waktu: memastikan layar tidak pernah diam tanpa keterangan walau pesan
  // dari backend tersendat. Ini jaring pengaman, bukan pengganti laporan sebenarnya.
  $effect(() => {
    if (!busy) return;
    const id = setInterval(() => (nowTick = Date.now()), 500);
    return () => clearInterval(id);
  });

  const detikSejakPesan = $derived(
    lastMessageAt ? Math.floor((nowTick - lastMessageAt) / 1000) : 0,
  );
  let dragIndex = $state(null);
  let overIndex = $state(null);

  onMount(() => {
    // Langganan dipasang SEKARANG, tanpa await, dan pembersihannya dikembalikan dari
    // onMount. Kalau ditulis `onMount(async () => { await listen(...) })`, tombol yang
    // ditekan sebelum await selesai akan berjalan tanpa langganan sama sekali: prosesnya
    // tetap jalan di backend, tapi tidak satu pun pesan sampai ke layar.
    let cleanup = () => {};
    let hidup = true;

    listen("progress", (event) => {
      const p = event.payload;
      if (!p?.message) return;
      progress = { text: p.message, fraction: p.fraction ?? null };
      lastMessageAt = Date.now();
      pushLog(p.message, p.message.startsWith("selesai") ? "ok" : "");
    }).then((un) => {
      // Kalau komponen sudah dilepas saat pendaftaran selesai, langsung lepaskan.
      if (hidup && typeof un === "function") cleanup = un;
      else if (typeof un === "function") un();
    });

    return () => {
      hidup = false;
      cleanup();
    };
  });

  function pushLog(text, kind = "") {
    log = [...log.slice(-200), { text, kind }];
  }

  function fmtBytes(n) {
    if (n >= 1 << 30) return `${(n / (1 << 30)).toFixed(2)} GB`;
    return `${(n / (1 << 20)).toFixed(1)} MB`;
  }

  function fmtClock(sec) {
    const s = Math.max(0, Math.round(sec));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const r = s % 60;
    return h > 0
      ? `${h}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`
      : `${m}:${String(r).padStart(2, "0")}`;
  }

  // Durasi hasil dan jumlah yang hilang karena transisi saling menumpuk.
  let stats = $derived.by(() => {
    const durations = tracks.map((t) => t.duration_s).filter((d) => d > 0);
    if (!durations.length) return null;
    const shortest = Math.min(...durations);
    const xf = Math.min(crossfadeS, shortest / 4);
    const total = durations.reduce((a, b) => a + b, 0);
    const overlap = tracks.length > 1 ? xf * (tracks.length - 1) : 0;
    return { total, overlap, result: total - overlap, effectiveXfade: xf, shortest };
  });

  export async function addFiles() {
    try {
      const paths = await pickFiles("audio");
      if (!paths?.length) return;
      error = null;

      const fresh = paths.filter((p) => !tracks.some((t) => t.path === p));
      if (fresh.length) {
        // Membaca properti tiap lagu berjalan serentak, bukan satu per satu, dan
        // kemajuannya ditampilkan supaya tidak terasa menggantung.
        let done = 0;
        reading = true;
        progress = { text: `membaca ${fresh.length} berkas…`, fraction: 0 };
        const results = await Promise.all(
          fresh.map(async (path) => {
            try {
              const info = await invoke("describe", { path });
              return { path, info };
            } catch (err) {
              return { path, info: null, err: String(err) };
            } finally {
              done += 1;
              progress = {
                text: `membaca berkas ${done}/${fresh.length}…`,
                fraction: done / fresh.length,
              };
            }
          }),
        );
        for (const r of results) {
          if (!r.info) {
            pushLog(`${r.path} tidak terbaca: ${r.err}`, "err");
            continue;
          }
          if (!r.info.has_audio) {
            pushLog(`${r.path.split(/[\/]/).pop()} tidak punya audio, dilewati`, "err");
            continue;
          }
          tracks = [...tracks, { path: r.path, duration_s: r.info.duration_s }];
        }
        reading = false;
        progress = { text: "", fraction: null };
      }
    } catch (err) {
      error = String(err);
    }
  }

  function remove(index) {
    tracks = tracks.filter((_, i) => i !== index);
    report = null;
  }

  function move(from, to) {
    if (to < 0 || to >= tracks.length || from === to) return;
    const next = [...tracks];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    tracks = next;
  }

  function onDragStart(index) {
    dragIndex = index;
  }

  function onDragOver(index, event) {
    event.preventDefault();
    overIndex = index;
  }

  function onDrop(index) {
    if (dragIndex !== null) move(dragIndex, index);
    dragIndex = null;
    overIndex = null;
  }

  export async function startMerge() {
    try {
      return await jalankanMerge();
    } catch (err) {
      // Laporkan ke berkas supaya penyebabnya bisa dibaca dari luar, bukan hanya
      // terlihat sekilas di layar.
      error = `Gagal memulai: ${err}`;
      pushLog(`gagal memulai: ${err}`, "err");
      busy = false;
      choosingFolder = false;
      onBusyChange?.(false);
    }
  }

  async function jalankanMerge() {
    if (tracks.length < 2) {
      error = "Butuh minimal dua lagu. Tambahkan lagi.";
      return;
    }
    error = null;
    report = null;

    // Tanya folder tujuan LEBIH DULU, sebelum status apa pun ditampilkan.
    //
    // Ini penting: dialog folder menunggu jawaban pengguna. Kalau status "menyiapkan"
    // ditampilkan sebelum dialog dibuka, layar akan memperlihatkan "menyiapkan" tanpa
    // ada proses apa pun yang berjalan -- tampak seperti macet padahal hanya menunggu
    // pilihan folder.
    let folder;
    choosingFolder = true;
    try {
      folder = await invoke("pick_folder", { title: "Pilih folder tujuan hasil gabungan" });
    } catch (err) {
      choosingFolder = false;
      error = `Tidak bisa membuka dialog folder: ${err}. Coba lagi, atau jalankan lewat CLI.`;
      return;
    }
    choosingFolder = false;
    if (!folder) return; // pengguna membatalkan, tidak ada yang perlu dikerjakan

    const sep = folder.includes("\\") ? "\\" : "/";
    const output = `${folder}${sep}gabungan_${tracks.length}_lagu.m4a`;

    // Periksa apakah ada pekerjaan lama yang statusnya masih terkunci. Pemeriksaan ini
    // hanya untuk memberi tahu: apa pun hasilnya, proses tetap dilanjutkan. Panggilan yang
    // menggantung di sini pernah membuat layar berhenti di "menyiapkan" tanpa pesan, jadi
    // sekarang dibatasi waktunya dan kegagalannya tidak menghentikan apa pun.
    try {
      const terkunci = await Promise.race([
        invoke("job_state"),
        new Promise((_, tolak) => setTimeout(() => tolak(new Error("lewat waktu")), 800)),
      ]);
      if (terkunci) {
        pushLog(
          "peringatan: status pekerjaan lama masih terkunci. Kalau gagal, tutup dan buka lagi aplikasinya.",
          "err",
        );
      }
    } catch (err) {
      pushLog(`pemeriksaan status dilewati: ${err}`, "err");
    }

    progress = { text: "memulai…", fraction: 0 };
    lastMessageAt = Date.now();
    pushLog(`menggabungkan ${tracks.length} lagu…`);
    pushLog(`tujuan: ${output}`);

    busy = true;
    onBusyChange?.(true);
    try {
      const result = await invoke("merge_songs", {
        opts: {
          tracks: tracks.map((t) => t.path),
          durations: tracks.map((t) => t.duration_s),
          output,
          crossfade_s: crossfadeS,
          normalize,
          target_lufs: -14.0,
          bitrate,
        },
      });
      report = result;
      progress = { text: "selesai", fraction: 1 };
      pushLog(`selesai: ${result.track_count} lagu, ${fmtClock(result.duration_s)}`, "ok");
    } catch (err) {
      error = String(err);
      progress = { text: "gagal", fraction: null };
      pushLog(String(err), "err");
    } finally {
      busy = false;
      onBusyChange?.(false);
    }
  }

  export function isBusy() {
    return busy;
  }

  /// Batalkan proses yang sedang berjalan. Backend memeriksa permintaan ini di antara
  /// langkah-langkah, jadi pembatalan terjadi dalam beberapa detik, bukan seketika.
  export async function cancelMerge() {
    try {
      await invoke("cancel_job");
      pushLog("permintaan batal dikirim, menunggu langkah saat ini selesai…");
      progress = { text: "membatalkan…", fraction: null };
    } catch (err) {
      pushLog(`gagal membatalkan: ${err}`, "err");
    }
  }
</script>

<div class="columns">
  <div class="card">
    <h2>Daftar lagu</h2>
    <div class="sub">
      Urutan di sini menentukan urutan hasil. Seret untuk mengubah urutan.
    </div>

    <button class="picker" onclick={addFiles} disabled={busy || reading}>
      <div class="glyph">➕</div>
      <div class="info">
        <div class="label">Tambah lagu</div>
        <div class="value">
          {tracks.length ? `${tracks.length} lagu siap digabung` : "belum ada lagu"}
        </div>
        <div class="meta">MP3, WAV, M4A, AAC, FLAC, OGG, OPUS · pilih beberapa sekaligus</div>
      </div>
    </button>

    {#if tracks.length}
      <ul class="tracklist">
        {#each tracks as track, i (track.path)}
          <li
            class="track {overIndex === i ? 'over' : ''}"
            draggable={!busy}
            ondragstart={() => onDragStart(i)}
            ondragover={(e) => onDragOver(i, e)}
            ondragleave={() => (overIndex = null)}
            ondrop={() => onDrop(i)}
          >
            <span class="num">{i + 1}</span>
            <span class="grip" title="Seret untuk memindah">⠿</span>
            <span class="name" title={track.path}>{track.path.split(/[\\/]/).pop()}</span>
            <span class="dur">{fmtClock(track.duration_s)}</span>
            <span class="nudge">
              <button title="Naikkan" onclick={() => move(i, i - 1)} disabled={busy || i === 0}>↑</button>
              <button title="Turunkan" onclick={() => move(i, i + 1)} disabled={busy || i === tracks.length - 1}>↓</button>
            </span>
            <button class="clear" title="Hapus" onclick={() => remove(i)} disabled={busy}>×</button>
          </li>
        {/each}
      </ul>
      <div class="btn-row" style="margin-top:10px">
        <button class="btn" onclick={addFiles} disabled={busy}>Tambah lagi</button>
        <button class="btn" onclick={() => (tracks = [])} disabled={busy}>Kosongkan</button>
      </div>
    {/if}
  </div>

  <div class="card">
    <h2>Transisi</h2>
    <div class="sub">Atur seberapa lembut perpindahan antar lagu.</div>

    <div class="field">
      <div class="head">
        <label for="xfade">Transisi antar lagu</label>
        <span class="readout">{crossfadeS} dtk</span>
      </div>
      <input id="xfade" type="range" min="0" max="10" step="0.5" bind:value={crossfadeS} disabled={busy} />
      {#if stats && stats.effectiveXfade < crossfadeS}
        <div class="sub" style="margin:6px 0 0">
          Dibatasi jadi {stats.effectiveXfade.toFixed(1)} dtk karena lagu terpendek {fmtClock(stats.shortest)}.
        </div>
      {/if}
    </div>

    <div class="field">
      <div class="row">
        <label for="normalize" style="width:auto;flex:1">Samakan volume antar lagu</label>
        <input id="normalize" type="checkbox" bind:checked={normalize} disabled={busy} />
      </div>
    </div>

    <div class="field">
      <div class="head"><label for="bitrate">Kualitas audio</label></div>
      <select id="bitrate" bind:value={bitrate} disabled={busy}>
        <option value="128k">128 kbps — paling hemat</option>
        <option value="192k">192 kbps — seimbang</option>
        <option value="256k">256 kbps — lebih baik</option>
        <option value="320k">320 kbps — terbaik</option>
      </select>
    </div>

    {#if stats}
      <div class="note">
        <div class="summary">
          <div class="row"><span class="k">Jumlah lagu</span><span class="v">{tracks.length}</span></div>
          <div class="row"><span class="k">Total materi</span><span class="v">{fmtClock(stats.total)}</span></div>
          <div class="row"><span class="k">Panjang hasil</span><span class="v">{fmtClock(stats.result)}</span></div>
          {#if stats.overlap > 0}
            <div class="row">
              <span class="k">Hilang karena transisi</span>
              <span class="v">{fmtClock(stats.overlap)}</span>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  <div class="card wide">
    <h2>Gabungkan</h2>
    <div class="btn-row">
      <button class="btn primary" onclick={startMerge} disabled={busy || tracks.length < 2}>
        {busy
          ? `${tracks.length} lagu sedang digabungkan…`
          : stats
            ? `Gabungkan ${tracks.length} lagu jadi ${fmtClock(stats.result)}`
            : "Gabungkan jadi satu berkas"}
      </button>
      {#if busy}
        <button class="btn" onclick={cancelMerge}>Batalkan</button>
      {/if}
      {#if choosingFolder}
        <span class="sub" style="align-self:center">
          memilih folder tujuan… kalau jendela dialog tidak terlihat, tekan Alt+Tab
        </span>
      {/if}
    </div>

    {#if busy || reading || choosingFolder || progress.text}
      <div class="progress">
        <div class="bar {busy && progress.fraction == null ? 'indeterminate' : ''}">
          <div class="fill" style="width:{progress.fraction != null ? progress.fraction * 100 : 100}%"></div>
        </div>
        <div class="caption">
          <span>{progress.text || "menyiapkan…"}</span>
          {#if progress.fraction != null}
            <span>{Math.round(progress.fraction * 100)}%</span>
          {/if}
        </div>
        {#if busy && detikSejakPesan >= 3}
          <!-- Jaring pengaman: kalau pesan dari backend tersendat, layar tetap memberi
               kabar bahwa prosesnya masih berjalan dan sudah berapa lama. -->
          <div class="sub" style="margin-top:6px">
            masih berjalan di belakang… {detikSejakPesan} detik sejak kabar terakhir
          </div>
        {/if}
      </div>
    {/if}

    {#if error}
      <div class="note err">{error}</div>
    {/if}

    {#if report}
      <div class="note" style="margin-top:16px">
        <div class="summary">
          <div class="row"><span class="k">Berkas</span><span class="v">{report.output}</span></div>
          <div class="row"><span class="k">Jumlah lagu</span><span class="v">{report.track_count}</span></div>
          <div class="row"><span class="k">Panjang</span><span class="v">{fmtClock(report.duration_s)}</span></div>
          <div class="row"><span class="k">Ukuran</span><span class="v">{fmtBytes(report.size_bytes)}</span></div>
          <div class="row"><span class="k">Transisi</span><span class="v">{report.crossfade_s} dtk</span></div>
          <div class="row">
            <span class="k">Format</span>
            <span class="v">{report.sample_rate} Hz · {report.channels} kanal</span>
          </div>
          <div class="row">
            <span class="k">Volume disamakan</span>
            <span class="v">{report.normalized ? "ya" : "tidak"}</span>
          </div>
          <div class="row"><span class="k">Waktu proses</span><span class="v">{report.elapsed_s.toFixed(1)} dtk</span></div>
        </div>
      </div>
    {/if}

    {#if log.length}
      <div class="log">
        {#each log as line}<div class={line.kind}>{line.text}</div>{/each}
      </div>
    {/if}
  </div>
</div>
