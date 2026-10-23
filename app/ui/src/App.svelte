<script>
  import { onMount } from "svelte";
  import { invoke, listen, inTauri, pickFile } from "./bridge.js";
  import { buatTimer, fmtHMS } from "./timer.js";
  import MergePage from "./MergePage.svelte";

  // ---------- keadaan ----------
  let view = $state("loading"); // loading | setup | main
  let page = $state("loop"); // loop | merge
  let mergeBusy = $state(false)
  let machine = $state(null);
  let config = $state(null);

  let files = $state({ video: null, audio: null });
  let meta = $state({ video: null, audio: null });

  let durationMin = $state(60);
  let loopS = $state(10);
  let xfadeS = $state(2);
  let fadeOutS = $state(3);
  let policy = $state("replace");
  let encoder = $state("quality");
  // 0 berarti mengikuti berkas sumber apa adanya.
  let targetHeight = $state(0);
  let targetFps = $state(0);
  let bitrateVideo = $state("4M");

  let busy = $state(false);
  let progress = $state({ text: "", fraction: null });
  let detik = $state(0);
  const timer = buatTimer((s) => (detik = s));
  let log = $state([]);
  let report = $state(null);
  let banner = $state(null);

  // ---------- penyiapan ----------
  let setupBusy = $state(false);
  let setupSteps = $state([]);
  let setupNote = $state(null);

  // ---------- pembaruan ----------
  let infoUpdate = $state(null);
  let updateDitolak = $state(false);

  /**
   * Periksa versi terbaru, lalu simpan hasilnya untuk ditampilkan.
   *
   * Tujuan `updateDitolak` adalah menghormati pilihan pengguna: begitu notifikasi ditutup,
   * ia tidak muncul lagi sampai aplikasi dibuka ulang. Kesalahan jaringan sengaja tidak
   * ditampilkan sama sekali, karena gagal menghubungi GitHub bukan masalah bagi pengguna
   * yang tidak sedang membutuhkan pembaruan.
   */
  async function periksaPembaruan() {
    try {
      const info = await invoke("check_update");
      if (info?.tersedia) infoUpdate = info;
    } catch {
      // Tidak ada yang perlu dilakukan.
    }
  }

  async function bukaPembaruan() {
    const alamat = infoUpdate?.halaman;
    if (!alamat) return;
    try {
      await invoke("open_url", { url: alamat });
    } catch (err) {
      banner = { kind: "err", text: `Tidak bisa membuka peramban: ${err}` };
    }
  }

  const DURATIONS = [
    { label: "30 menit", value: 30 },
    { label: "1 jam", value: 60 },
    { label: "3 jam", value: 180 },
  ];

  // Pilihan tinggi keluaran. Lebar mengikuti rasio aspek sumber, jadi yang ditawarkan
  // hanya tingginya. Ukuran berkas mengikuti tinggi dan bitrate, bukan fps saja.
  const RESOLUSI = [
    { label: "Ikut sumber", value: 0 },
    { label: "360p", value: 360 },
    { label: "480p", value: 480 },
    { label: "720p", value: 720 },
    { label: "1080p", value: 1080 },
  ];

  const FPS = [
    { label: "Ikut sumber", value: 0 },
    { label: "24", value: 24 },
    { label: "25", value: 25 },
    { label: "30", value: 30 },
    { label: "60", value: 60 },
  ];

  const BITRATE = [
    { label: "1,5M — paling hemat", value: "1.5M" },
    { label: "3M — hemat", value: "3M" },
    { label: "4M — seimbang", value: "4M" },
    { label: "8M — kualitas tinggi", value: "8M" },
    { label: "16M — maksimal", value: "16M" },
  ];


  // Ringkasan hasil: ukuran keluaran dan perkiraan besar berkas.
  //
  // Perkiraan dihitung dari bitrate dikali durasi, bukan pengukuran. Angka ini untuk
  // memberi gambaran sebelum menekan tombol, bukan janji. Hasil sebenarnya bisa berbeda
  // karena isi gambar memengaruhi berapa bitrate yang benar-benar terpakai.
  let bentukHasil = $derived.by(() => {
    const v = meta.video;
    if (!v || !v.width || !v.height) return null;
    const tinggi = targetHeight || v.height;
    const lebar = Math.round((v.width / v.height) * tinggi / 2) * 2;
    const fps = targetFps || v.fps;
    const mbps = Number.parseFloat(bitrateVideo) * 1_000_000;
    const bytes = (mbps / 8) * (durationMin * 60);
    const rasio = tinggi / v.height;
    return {
      teks: `${lebar}×${tinggi} · ${fps.toFixed(2)} fps · ±${fmtBytes(bytes)}`,
      catatan:
        tinggi === v.height && Math.abs(fps - v.fps) < 0.01
          ? `Sama dengan sumber (${v.width}×${v.height}, ${v.fps.toFixed(2)} fps).`
          : `Diskalakan ${(rasio * 100).toFixed(0)}% dari sumber; berkas ${rasio < 1 ? "lebih kecil" : "lebih besar"} dari aslinya.`,
    };
  });

  // Ringkasan apa yang akan dilakukan pada audio, dihitung dari panjang lagu dan durasi target.
  let audioPlan = $derived.by(() => {
    const a = meta.audio?.duration_s;
    if (!a) return null;
    const target = durationMin * 60;
    if (a > target + 0.5) return `Lagu ${fmtLong(a)} lebih panjang dari hasil, dipotong di ${fmtLong(target)} dengan fade-out ${fadeOutS} dtk.`;
    if (Math.abs(a - target) <= 0.5) return `Lagu ${fmtLong(a)} pas dengan durasi hasil, diputar sekali.`;
    const times = target / Math.max(a - 2, 1);
    return `Lagu ${fmtLong(a)} diputar utuh lalu diulang ${times.toFixed(1)}×, fade-out ${fadeOutS} dtk di akhir.`;
  });

  function fmtLong(sec) {
    if (sec >= 3600) return `${(sec / 3600).toFixed(1)} jam`;
    if (sec >= 60) return `${Math.floor(sec / 60)}:${String(Math.round(sec % 60)).padStart(2, "0")} menit`;
    return `${Math.round(sec)} dtk`;
  }

  function pushLog(text, kind = "") {
    log = [...log.slice(-300), { text, kind }];
  }

  function fmtDuration(min) {
    if (min >= 60) {
      const h = min / 60;
      return `${Number.isInteger(h) ? h : h.toFixed(1)} jam`;
    }
    return `${min} menit`;
  }

  function fmtBytes(n) {
    if (n >= 1 << 30) return `${(n / (1 << 30)).toFixed(2)} GB`;
    return `${(n / (1 << 20)).toFixed(1)} MB`;
  }

  function fmtClock(seconds) {
    const s = Math.max(0, Math.round(seconds));
    const m = Math.floor(s / 60);
    const r = s % 60;
    return `${m}:${String(r).padStart(2, "0")}`;
  }

  async function refreshMachine() {
    machine = await invoke("inspect_machine");
    return machine;
  }

  async function loadAll() {
    try {
      config = await invoke("load_config");
      durationMin = config.duration_minutes ?? 60;
      loopS = config.video_loop_s ?? 10;
      xfadeS = config.video_xfade_s ?? 2;
      policy = config.policy ?? "replace";
      encoder = config.encoder ?? "quality";
      bitrateVideo = config.bitrate_video ?? "4M";

      const report = await refreshMachine();
      view = report.ready && config.onboarded ? "main" : "setup";
      if (view === "setup") buildSetupSteps(report);

      // Pemeriksaan pembaruan dijalankan tanpa ditunggu. Kalau jaringan lambat atau tidak
      // ada, aplikasi tetap terbuka dan bisa dipakai; notifikasinya muncul kalau hasilnya ada.
      periksaPembaruan();
    } catch (err) {
      view = "setup";
      setupNote = { kind: "err", text: String(err) };
      setupSteps = [];
    }
  }

  function buildSetupSteps(report) {
    if (!report) return;
    const enc = report.encoders ?? [];
    const found = enc.filter((e) => e.available && e.tested === true).map((e) => e.name);
    setupSteps = [
      {
        key: "ffmpeg",
        name: "FFmpeg",
        done: report.ffmpeg_found,
        fail: !report.ffmpeg_found,
        detail: report.ffmpeg_found
          ? report.ffmpeg_dir ?? "ditemukan"
          : "belum ada. Bisa diunduh otomatis (sekitar 110 MB).",
      },
      {
        key: "encoders",
        name: "Encoder video dan audio",
        done: found.length > 0,
        fail: report.ffmpeg_found && found.length === 0,
        detail: report.ffmpeg_found
          ? found.length
            ? `siap dipakai: ${found.join(", ")}`
            : "tidak ada encoder yang lulus uji"
          : "menunggu FFmpeg",
      },
      {
        key: "speed",
        name: "Uji kecepatan mesin",
        done: report.encode_speed_realtime != null,
        fail: false,
        detail:
          report.encode_speed_realtime != null
            ? `${report.encode_speed_realtime.toFixed(1)}x lebih cepat dari waktu nyata. Perkiraan 1 jam video selesai dalam ${fmtClock(
                3600 / Math.max(report.encode_speed_realtime, 0.1),
              )}.`
            : "belum diukur",
      },
    ];
  }

  async function runSetupDownload() {
    setupBusy = true;
    setupNote = null;
    try {
      const result = await invoke("install_ffmpeg");
      pushLog(`FFmpeg terpasang di ${result.dir}`, "ok");
      if (result.verified_sha256) pushLog("SHA256 berkas unduhan cocok", "ok");
      config = await invoke("load_config");
      const report = await refreshMachine();
      buildSetupSteps(report);
      if (report.ready) {
        setupNote = { kind: "", text: "Mesin siap. Lanjut ke halaman utama." };
      }
    } catch (err) {
      setupNote = { kind: "err", text: String(err) };
    } finally {
      setupBusy = false;
    }
  }

  async function pickFfmpegFolder() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const dir = await open({ directory: true, multiple: false, title: "Pilih folder berisi ffmpeg.exe" });
      if (!dir) return;
      config = await invoke("save_config", { config: { ...config, ffmpeg_dir: dir } });
      const report = await refreshMachine();
      buildSetupSteps(report);
    } catch (err) {
      setupNote = { kind: "err", text: String(err) };
    }
  }

  async function finishSetup() {
    config = await invoke("save_config", { config: { ...config, onboarded: true } });
    view = "main";
  }

  // ---------- berkas ----------
  async function choose(kind) {
    try {
      const path = await pickFile(kind);
      if (!path) return;
      await describe(path, kind);
    } catch (err) {
      pushLog(`gagal membuka dialog: ${err}`, "err");
    }
  }

  async function describe(path, kind) {
    try {
      const info = await invoke("describe", { path });
      files = { ...files, [kind]: path };
      meta = { ...meta, [kind]: info };
      if (kind === "video" && info.duration_s && !meta.video) {
        // Panjang loop tidak boleh melebihi durasi klip.
        loopS = Math.min(loopS, Math.max(3, Math.floor(info.duration_s)));
      }
    } catch (err) {
      pushLog(`${kind} tidak terbaca: ${err}`, "err");
    }
  }

  function clearFile(kind) {
    files = { ...files, [kind]: null };
    meta = { ...meta, [kind]: null };
  }

  function onDrop(event, kind) {
    event.preventDefault();
    const path = event.dataTransfer?.files?.[0]?.path;
    if (path) describe(path, kind);
  }

  // ---------- render ----------
  async function startRender() {
    if (!files.video) {
      banner = { kind: "err", text: "Pilih berkas video dulu." };
      return;
    }
    busy = true;
    report = null;
    banner = null;
    log = [];
    progress = { text: "menyiapkan…", fraction: 0 };
    timer.mulai();

    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const base = files.video.split(/[\\/]/).pop().replace(/\.[^.]+$/, "");
      const output = await save({
        defaultPath: `${base}_${durationMin}m.mp4`,
        filters: [{ name: "Video MP4", extensions: ["mp4"] }],
      });
      if (!output) {
        busy = false;
        timer.henti();
        return;
      }

      const opts = {
        video: files.video,
        audio: files.audio,
        output,
        duration_s: durationMin * 60,
        video_loop_s: loopS,
        audio_fade_out_s: fadeOutS,
        video_xfade_s: xfadeS,
        audio_xfade_s: 2.0,
        policy,
        encoder,
        bitrate_video: bitrateVideo,
        bitrate_audio: "192k",
        target_lufs: config?.target_lufs ?? -14.0,
        silence_dbfs: -50.0,
        target: {
          height: targetHeight || null,
          fps: targetFps || null,
        },
      };

      pushLog(`target: ${output}`);
      const result = await invoke("render", { opts });
      report = result;
      progress = { text: "selesai", fraction: 1 };
      pushLog(`selesai dalam ${result.elapsed_s.toFixed(1)} dtk`, "ok");

      await invoke("save_config", {
        config: {
          ...config,
          duration_minutes: durationMin,
          video_loop_s: loopS,
          video_xfade_s: xfadeS,
          policy,
          encoder,
          bitrate_video: bitrateVideo,
        },
      });
    } catch (err) {
      banner = { kind: "err", text: String(err) };
      pushLog(String(err), "err");
    } finally {
      busy = false;
      timer.henti();
    }
  }

  async function cancelRender() {
    try {
      await invoke("cancel_render");
      pushLog("permintaan batal dikirim; langkah ffmpeg yang berjalan diselesaikan dulu");
    } catch (err) {
      pushLog(String(err), "err");
    }
  }

  onMount(async () => {
    if (!inTauri) {
      view = "setup";
      setupNote = {
        kind: "err",
        text:
          "Halaman ini dibuka di browser, jadi tidak bisa mengakses engine. Jalankan lofiloop-app.exe.",
      };
      return;
    }
    await listen("progress", (event) => {
      const p = event.payload;
      if (p.message) {
        progress = { text: p.message, fraction: p.fraction ?? progress.fraction };
        pushLog(p.message, p.message.startsWith("selesai") ? "ok" : "");
      }
    });
    await listen("download-progress", (event) => {
      const p = event.payload;
      setupNote = { kind: "", text: `${p.phase}… ${fmtBytes(p.received_bytes)}` };
    });
    await loadAll();
  });
</script>

<div class="app">
  <header class="titlebar">
    <img class="mark" src="logo.svg" alt="" width="28" height="28" />
    <div class="wordmark">LofiLoop</div>
    {#if view === "main"}
      <nav class="tabs">
        <button class="tab {page === "loop" ? "active" : ""}" onclick={() => (page = "loop")}>
          Loop video
        </button>
        <button class="tab {page === "merge" ? "active" : ""}" onclick={() => (page = "merge")}>
          Gabung lagu
        </button>
      </nav>
    {/if}
    <div class="spacer"></div>
    {#if view === "main" && page === "loop" && machine}
      <div class="status {machine.ready ? 'ok' : 'warn'}">
        <span class="dot"></span>
        <span class="text">
          {machine.ready ? "siap" : "perlu perhatian"} · {machine.recommended_encoder === "quality"
            ? "libx264"
            : "h264_qsv"}{#if machine.encode_speed_realtime} · {machine.encode_speed_realtime.toFixed(1)}x{/if}
        </span>
      </div>
    {/if}
  </header>

  <div class="body">
    {#if view === "main" && infoUpdate && !updateDitolak}
      <div class="note update" style="max-width:1180px;margin:0 auto 18px">
        <div class="update-text">
          <strong>Versi {infoUpdate.latest.replace(/^v/, "")} sudah tersedia.</strong>
          <span>
            Kamu memakai {infoUpdate.current}. Unduh, lalu pasang seperti biasa — pengaturan
            dan berkasmu tidak terpengaruh.
          </span>
        </div>
        <div class="update-aksi">
          <button class="btn primary" onclick={bukaPembaruan}>Buka unduhan</button>
          <button class="btn" onclick={() => (updateDitolak = true)}>Nanti</button>
        </div>
      </div>
    {/if}

    {#if view === "loading"}
      <div class="setup">
        <div class="card">
          <h2>Memeriksa mesin</h2>
          <div class="sub">Mendeteksi FFmpeg, encoder, dan dukungan GPU…</div>
        </div>
      </div>
    {/if}

    {#if view === "setup"}
      <div class="setup">
        <div class="card">
          <h2>Penyiapan</h2>
          <div class="sub">
            LofiLoop butuh FFmpeg untuk memproses video. Pemeriksaan di bawah berjalan otomatis.
          </div>

          {#if setupSteps.length}
            {#each setupSteps as step (step.key)}
              <div class="step {step.done ? 'done' : step.fail ? 'fail' : 'busy'}">
                <div class="bullet">{step.done ? "✓" : step.fail ? "!" : "…"}</div>
                <div class="content">
                  <div class="name">{step.name}</div>
                  <div class="detail">{step.detail}</div>
                </div>
              </div>
            {/each}
          {/if}

          {#if machine}
            <dl class="facts">
              <dt>Prosesor</dt>
              <dd>{machine.cpu} · {machine.logical_cores} core</dd>
              <dt>GPU</dt>
              <dd>{machine.gpu ?? "tidak terdeteksi"}</dd>
              <dt>Ruang disk</dt>
              <dd>{machine.disk_free_gb != null ? `${machine.disk_free_gb} GB bebas` : "tidak terbaca"}</dd>
              <dt>Rekomendasi</dt>
              <dd>{machine.recommendation_note}</dd>
            </dl>
          {/if}

          {#if setupNote}
            <div class="note {setupNote.kind}">{setupNote.text}</div>
          {/if}

          {#if machine && machine.problems.length}
            <div class="note warn">
              {#each machine.problems as problem}
                <div>• {problem}</div>
              {/each}
            </div>
          {/if}

          <div class="btn-row" style="margin-top:18px">
            {#if machine && !machine.ffmpeg_found}
              <button class="btn primary" onclick={runSetupDownload} disabled={setupBusy}>
                {setupBusy ? "mengunduh…" : "Unduh FFmpeg otomatis (±110 MB)"}
              </button>
              <button class="btn" onclick={pickFfmpegFolder} disabled={setupBusy}>
                Saya punya folder FFmpeg
              </button>
            {:else}
              <button class="btn primary" onclick={finishSetup}>Lanjut ke aplikasi</button>
              <button class="btn" onclick={refreshMachine}>Periksa ulang</button>
              <button class="btn" onclick={pickFfmpegFolder}>Ganti folder FFmpeg</button>
            {/if}
          </div>
        </div>
      </div>
    {/if}

    {#if view === "main" && page === "merge"}
      <MergePage onBusyChange={(v) => (mergeBusy = v)} />
    {/if}

    {#if view === "main" && page === "loop"}
      {#if banner}
        <div class="note {banner.kind}" style="max-width:1180px;margin:0 auto 18px">{banner.text}</div>
      {/if}

      <div class="columns">
        <div class="card">
          <h2>Sumber</h2>
          <div class="sub">Seret berkas ke sini, atau klik untuk memilih.</div>

          <button
            class="picker {files.video ? 'filled' : ''}"
            onclick={() => choose("video")}
            ondragover={(e) => e.preventDefault()}
            ondrop={(e) => onDrop(e, "video")}
          >
            <div class="glyph">🎬</div>
            <div class="info">
              <div class="label">Video</div>
              <div class="value">{files.video?.split(/[\\/]/).pop() ?? "belum dipilih"}</div>
              {#if meta.video}
                <div class="meta">
                  {meta.video.duration_s.toFixed(1)} dtk · {meta.video.width}×{meta.video.height} ·
                  {meta.video.fps.toFixed(2)} fps
                  {#if !meta.video.has_audio}· tanpa audio
                  {:else if meta.video.audible}· ada suara
                  {:else}· audio senyap{/if}
                </div>
              {/if}
            </div>
            {#if files.video}
              <button
                class="clear"
                title="Hapus"
                onclick={(e) => { e.stopPropagation(); clearFile("video"); }}
              >×</button>
            {/if}
          </button>

          <button
            class="picker {files.audio ? 'filled' : ''}"
            onclick={() => choose("audio")}
            ondragover={(e) => e.preventDefault()}
            ondrop={(e) => onDrop(e, "audio")}
          >
            <div class="glyph">🎵</div>
            <div class="info">
              <div class="label">Audio <span style="opacity:.6">(opsional)</span></div>
              <div class="value">{files.audio?.split(/[\\/]/).pop() ?? "belum dipilih"}</div>
              {#if meta.audio}
                <div class="meta">
                  {meta.audio.duration_s.toFixed(1)} dtk · {meta.audio.sample_rate ?? 44100} Hz ·
                  {meta.audio.channels ?? 2} kanal
                </div>
              {/if}
            </div>
            {#if files.audio}
              <button
                class="clear"
                title="Hapus"
                onclick={(e) => { e.stopPropagation(); clearFile("audio"); }}
              >×</button>
            {/if}
          </button>
        </div>

        <div class="card">
          <h2>Hasil</h2>
          <div class="sub">Atur durasi dan cara audio digabungkan.</div>

          <div class="field">
            <div class="head">
              <label for="duration">Durasi</label>
              <span class="readout">{fmtDuration(durationMin)}</span>
            </div>
            <div class="segments" style="margin-bottom:9px">
              {#each DURATIONS as d}
                <button
                  class="seg {durationMin === d.value ? 'active' : ''}"
                  onclick={() => (durationMin = d.value)}
                >{d.label}</button>
              {/each}
            </div>
            <input
              id="duration"
              type="range"
              min="1"
              max="360"
              bind:value={durationMin}
              disabled={busy}
            />
          </div>

          <div class="field">
            <div class="head">
              <label for="loop">Panjang loop</label>
              <span class="readout">{loopS} dtk</span>
            </div>
            <input id="loop" type="range" min="3" max="60" bind:value={loopS} disabled={busy} />
          </div>

          <div class="field">
            <div class="head">
              <label for="xfade">Crossfade video</label>
              <span class="readout">{xfadeS} dtk</span>
            </div>
            <input
              id="xfade"
              type="range"
              min="0.5"
              max="4"
              step="0.5"
              bind:value={xfadeS}
              disabled={busy}
            />
          </div>

          <div class="field">
            <div class="head">
              <label for="fadeout">Fade-out audio di akhir</label>
              <span class="readout">{fadeOutS} dtk</span>
            </div>
            <input id="fadeout" type="range" min="1" max="10" step="1" bind:value={fadeOutS} disabled={busy} />
          </div>

          {#if audioPlan}
            <div class="note" style="margin-bottom:16px">{audioPlan}</div>
          {/if}

          <div class="field">
            <div class="head"><label for="policy">Suara video</label></div>
            <select id="policy" bind:value={policy} disabled={busy}>
              <option value="replace">Matikan, pakai audio saya</option>
              <option value="duck">Campur keduanya</option>
              <option value="keep">Pertahankan suara video</option>
            </select>
          </div>

          <div class="field">
            <div class="head"><label for="encoder">Encode</label></div>
            <select id="encoder" bind:value={encoder} disabled={busy}>
              <option value="quality">libx264 — sambungan paling halus</option>
              <option value="hardware">h264_qsv — lebih cepat, sambungan lebih kasar</option>
            </select>
          </div>

          <div class="field">
            <div class="head">
              <label for="resolusi">Resolusi</label>
              {#if targetHeight === 0 && meta.video}
                <span class="readout">{meta.video.width}×{meta.video.height}</span>
              {/if}
            </div>
            <div class="segments">
              {#each RESOLUSI as r}
                <button
                  class="seg {targetHeight === r.value ? 'active' : ''}"
                  onclick={() => (targetHeight = r.value)}
                  disabled={busy}
                >{r.label}</button>
              {/each}
            </div>
          </div>

          <div class="field">
            <div class="head">
              <label for="fps">Frame per detik</label>
              {#if targetFps === 0 && meta.video}
                <span class="readout">{meta.video.fps.toFixed(2)}</span>
              {/if}
            </div>
            <div class="segments">
              {#each FPS as f}
                <button
                  class="seg {targetFps === f.value ? 'active' : ''}"
                  onclick={() => (targetFps = f.value)}
                  disabled={busy}
                >{f.label}</button>
              {/each}
            </div>
          </div>

          <div class="field">
            <div class="head"><label for="bitrate">Bitrate video</label></div>
            <select id="bitrate" bind:value={bitrateVideo} disabled={busy}>
              {#each BITRATE as b}
                <option value={b.value}>{b.label}</option>
              {/each}
            </select>
          </div>

          {#if bentukHasil}
            <div class="note" style="margin-bottom:16px">
              <div><strong>{bentukHasil.teks}</strong></div>
              <div style="margin-top:4px;opacity:.8">{bentukHasil.catatan}</div>
            </div>
          {/if}
        </div>

        <div class="card wide">
          <h2>Jalankan</h2>
          <div class="btn-row">
            <button class="btn primary" onclick={startRender} disabled={busy || !files.video}>
              {busy ? "sedang berjalan…" : "Mulai render"}
            </button>
            <button class="btn danger" onclick={cancelRender} disabled={!busy}>Batalkan</button>
          </div>

          {#if busy || progress.text}
            <div class="progress">
              <div class="bar {busy && progress.fraction == null ? 'indeterminate' : ''}">
                <div class="fill" style="width:{progress.fraction != null ? progress.fraction * 100 : 100}%"></div>
              </div>
              <div class="caption">
                <span>{progress.text}</span>
                <span class="durasi">
                  {#if busy}
                    <span class="timer" title="Waktu berjalan sejak render dimulai">{fmtHMS(detik)}</span>
                  {/if}
                  {#if machine?.encode_speed_realtime && busy}
                    <span>perkiraan sisa {fmtClock((durationMin * 60) / machine.encode_speed_realtime * 0.25)}</span>
                  {/if}
                </span>
              </div>
            </div>
          {/if}

          {#if report}
            <div class="note" style="margin-top:16px">
              <div class="summary">
                <div class="row"><span class="k">Berkas</span><span class="v">{report.output}</span></div>
                <div class="row"><span class="k">Ukuran</span><span class="v">{fmtBytes(report.size_bytes)}</span></div>
                <div class="row">
                  <span class="k">Durasi video</span>
                  <span class="v">{report.duration_video_s.toFixed(3)} dtk ({report.video_repeats}× loop)</span>
                </div>
                <div class="row">
                  <span class="k">Bentuk keluaran</span>
                  <span class="v">{report.output_width}×{report.output_height} · {report.output_fps.toFixed(2)} fps</span>
                </div>
                {#if report.duration_audio_s}
                  <div class="row">
                    <span class="k">Durasi audio</span>
                    <span class="v">{report.duration_audio_s.toFixed(3)} dtk</span>
                  </div>
                  <div class="row">
                    <span class="k">Selisih A/V</span>
                    <span class="v">{report.av_delta_s.toFixed(3)} dtk</span>
                  </div>
                {/if}
                <div class="row"><span class="k">Waktu proses</span><span class="v">{report.elapsed_s.toFixed(1)} dtk</span></div>
                <div class="row"><span class="k">Audio</span><span class="v">{report.policy_note}</span></div>
              </div>
            </div>
          {/if}

          {#if log.length}
            <div class="log">
              {#each log as line}
                <div class={line.kind}>{line.text}</div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>
