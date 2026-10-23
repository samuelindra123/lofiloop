// UI LofiLoop. Sengaja tanpa framework: semua pekerjaan berat ada di engine (Rust),
// jadi frontend hanya perlu menangani input dan menampilkan progres.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog;

const state = {
  video: null,
  audio: null,
  running: false,
};

const $ = (id) => document.getElementById(id);

function fmtDuration(seconds) {
  if (seconds >= 3600) {
    const h = seconds / 3600;
    return `${Number.isInteger(h) ? h : h.toFixed(1)} jam`;
  }
  if (seconds >= 60) return `${Math.round(seconds / 60)} menit`;
  return `${seconds} dtk`;
}

function fmtBytes(n) {
  if (n > 1 << 30) return `${(n / (1 << 30)).toFixed(2)} GB`;
  return `${(n / (1 << 20)).toFixed(1)} MB`;
}

function logLine(text, cls = "") {
  const li = document.createElement("li");
  li.textContent = text;
  if (cls) li.className = cls;
  const list = $("log");
  list.appendChild(li);
  list.scrollTop = list.scrollHeight;
}

function setProgress(fraction, text) {
  $("progress-wrap").hidden = false;
  $("progress-fill").style.width = `${Math.max(0, Math.min(1, fraction)) * 100}%`;
  if (text) $("progress-text").textContent = text;
}

// ---------- pemilihan berkas ----------

async function pick(kind) {
  const filters = kind === "video"
    ? [{ name: "Video", extensions: ["mp4", "mov", "mkv", "webm", "avi", "m4v"] }]
    : [{ name: "Audio", extensions: ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"] }];
  const picked = await open({ multiple: false, filters });
  if (!picked) return;
  await describe(picked, kind);
}

async function describe(path, kind) {
  try {
    const info = await invoke("describe", { path });
    state[kind] = path;
    const base = path.split(/[\\/]/).pop();
    $(`${kind}-name`).textContent = base;
    const bits = [`${info.duration_s.toFixed(1)} dtk`];
    if (kind === "video") {
      bits.push(`${info.width}x${info.height}`, `${info.fps.toFixed(2)} fps`);
      bits.push(info.has_audio ? (info.audible ? "ada suara" : "audio senyap") : "tanpa audio");
    } else {
      bits.push(`${info.sample_rate ?? 44100} Hz`, `${info.channels ?? 2} kanal`);
    }
    $(`${kind}-meta`).textContent = bits.join(" · ");
    $(`drop-${kind}`).classList.add("filled");
  } catch (err) {
    logLine(`${kind} tidak terbaca: ${err}`, "error");
  }
}

for (const kind of ["video", "audio"]) {
  const el = $(`drop-${kind}`);
  el.addEventListener("click", () => pick(kind));
  el.addEventListener("dragover", (e) => { e.preventDefault(); el.classList.add("over"); });
  el.addEventListener("dragleave", () => el.classList.remove("over"));
  el.addEventListener("drop", async (e) => {
    e.preventDefault();
    el.classList.remove("over");
    const path = e.dataTransfer.files?.[0]?.path;
    if (path) await describe(path, kind);
  });
}

// ---------- kontrol ----------

$("duration").addEventListener("input", (e) => {
  $("duration-value").textContent = fmtDuration(Number(e.target.value) * 60);
  document.querySelectorAll(".chip").forEach((c) => c.classList.remove("active"));
});

document.querySelectorAll(".chip").forEach((chip) => {
  chip.addEventListener("click", () => {
    const minutes = Number(chip.dataset.duration) / 60;
    $("duration").value = minutes;
    $("duration-value").textContent = fmtDuration(Number(chip.dataset.duration));
    document.querySelectorAll(".chip").forEach((c) => c.classList.remove("active"));
    chip.classList.add("active");
  });
});

$("loop").addEventListener("input", (e) => { $("loop-value").textContent = `${e.target.value} dtk`; });
$("xfade").addEventListener("input", (e) => { $("xfade-value").textContent = `${e.target.value} dtk`; });

// ---------- render ----------

$("render").addEventListener("click", async () => {
  if (!state.video) { logLine("pilih berkas video dulu", "error"); return; }
  const minutes = Number($("duration").value);
  const output = await invoke("pick_output", {
    suggested: `lofiloop_${minutes}m.mp4`,
  });
  if (!output) return;

  const opts = {
    video: state.video,
    audio: state.audio,
    output,
    duration_s: minutes * 60,
    video_loop_s: Number($("loop").value),
    audio_loop_s: null,
    video_xfade_s: Number($("xfade").value),
    audio_xfade_s: 2.0,
    policy: $("policy").value,
    encoder: $("encoder").value,
    bitrate_video: "4M",
    bitrate_audio: "192k",
    target_lufs: -14.0,
    silence_dbfs: -50.0,
  };

  state.running = true;
  $("render").disabled = true;
  $("cancel").disabled = false;
  $("log").innerHTML = "";
  setProgress(0, "menyiapkan…");
  logLine(`target: ${output.split(/[\\/]/).pop()}`);

  try {
    const report = await invoke("render", { opts });
    setProgress(1, "selesai");
    const basic = [
      `selesai: ${fmtBytes(report.size_bytes)}`,
      `video ${report.duration_video_s.toFixed(1)} dtk (${report.video_repeats} kali loop)`,
    ];
    if (report.duration_audio_s) {
      basic.push(`audio ${report.duration_audio_s.toFixed(1)} dtk`, `selisih ${report.av_delta_s.toFixed(3)} dtk`);
    }
    basic.push(`waktu proses ${report.elapsed_s.toFixed(1)} dtk`);
    logLine(basic.join(" · "), "result");
    logLine(report.policy_note);
  } catch (err) {
    setProgress(0, "gagal");
    logLine(String(err), "error");
  } finally {
    state.running = false;
    $("render").disabled = false;
    $("cancel").disabled = true;
  }
});

$("cancel").addEventListener("click", async () => {
  try {
    await invoke("cancel_render");
    logLine("permintaan batal dikirim; menunggu langkah ffmpeg saat ini selesai", "error");
  } catch (err) {
    logLine(String(err), "error");
  }
});

// ---------- jembatan ke backend ----------

await listen("progress", (event) => {
  const { message, fraction } = event.payload;
  setProgress(fraction ?? 0, message);
  logLine(message);
});

listen("engine-status", (event) => {
  const { ok, detail } = event.payload;
  const tag = $("engine-tag");
  tag.textContent = detail;
  tag.className = `tag ${ok ? "ok" : "err"}`;
  if (!ok) {
    $("render").disabled = true;
    logLine(detail, "error");
  }
});

try {
  const status = await invoke("check_engine");
  const tag = $("engine-tag");
  tag.textContent = status.detail;
  tag.className = `tag ${status.ok ? "ok" : "err"}`;
  if (!status.ok) logLine(status.detail, "error");
} catch (err) {
  $("engine-tag").textContent = `ffmpeg tidak siap: ${err}`;
  $("engine-tag").className = "tag err";
}
