"""Uji sambungan otomatis untuk hasil LofiLoop. Dipakai sebelum menyerahkan hasil ke user.

Memeriksa metrik yang sama seperti spike Fase 1:
  1. Durasi video dan audio, serta selisihnya.
  2. Sambungan video: selisih frame di batas unit dibanding rata-rata 10 frame tetangga.
  3. Sambungan audio: kontinuitas |x[n]-x[n-1]| di batas unit dan dip level di zona crossfade.

Pakai:
  python -I scripts/verify_output.py <berkas.mp4> --unit-video 200 --fps 25 \
      --unit-audio 10.008 --audio-xfade 2 [--video-sec 90] [--audio-sec 140]
"""
import argparse
import subprocess
import sys

import numpy as np

VIDEO_LIMIT = 1.5
AUDIO_JUMP_LIMIT = 1.2
AUDIO_DIP_LIMIT_DB = -3.0


def probe_value(path, entries, stream=None):
    args = ["ffprobe", "-v", "error"]
    if stream:
        args += ["-select_streams", stream]
    args += ["-show_entries", entries, "-of", "default=nw=1:nk=1", path]
    out = subprocess.run(args, capture_output=True, text=True, check=True).stdout.split()
    return out


def check_video(path, unit_frames, fps, seconds):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-t", str(seconds),
         "-vf", "scale=160:90,format=gray", "-f", "rawvideo", "-"],
        capture_output=True, check=True).stdout
    frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, 90, 160).astype(np.int16)
    diffs = np.abs(np.diff(frames, axis=0)).mean(axis=(1, 2))
    if len(frames) <= unit_frames:
        print(f"  video: cuma {len(frames)} frame, tidak ada sambungan untuk diuji")
        return None
    ratios = []
    for b in range(unit_frames, len(frames), unit_frames):
        s = b - 1
        around = np.concatenate([diffs[max(0, s - 10):s], diffs[s + 1:s + 11]])
        ratios.append(float(diffs[s] / max(around.mean(), 1e-6)))
    worst = max(ratios)
    ok = worst < VIDEO_LIMIT
    print(f"  sambungan video: {len(ratios)} titik, rasio terburuk {worst:.2f} "
          f"(median selisih {np.median(diffs):.3f}) {'LULUS' if ok else 'GAGAL'}")
    return ok


def check_audio(path, unit_s, xfade_s, seconds, sample_rate):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-t", str(seconds), "-i", path, "-ac", "2",
         "-ar", str(sample_rate), "-f", "f32le", "-"],
        capture_output=True, check=True).stdout
    x = np.frombuffer(raw, dtype=np.float32).reshape(-1, 2)
    unit = int(round(unit_s * sample_rate))
    bounds = [b for b in range(unit, len(x) - 1, unit)]
    if not bounds:
        print("  audio: rentang terlalu pendek untuk memuat satu batas unit")
        return None

    d = np.abs(np.diff(x, axis=0)).max(axis=1)
    guard = int(0.002 * sample_rate)
    mask = np.ones(len(d), bool)
    for b in bounds:
        mask[max(0, b - 1 - guard):b - 1 + guard + 1] = False
    base = d[mask].max()
    worst_jump = max(d[max(0, b - 1 - guard):b - 1 + guard + 1].max() for b in bounds) / base

    win = int(0.1 * sample_rate)
    nw = len(x) // win
    rms = np.sqrt((x[:nw * win].astype(np.float64) ** 2).mean(axis=1).reshape(nw, win).mean(axis=1))
    t = (np.arange(nw) + 0.5) * win / sample_rate
    in_xf = np.zeros(nw, bool)
    for b in bounds + [0]:
        in_xf |= (t >= (b - int(round(xfade_s * sample_rate))) / sample_rate) & (t <= b / sample_rate)
    dip_db = 20 * np.log10(rms[in_xf].min() / rms[~in_xf].min()) if in_xf.any() else float("nan")

    ok = worst_jump < AUDIO_JUMP_LIMIT and dip_db > AUDIO_DIP_LIMIT_DB
    print(f"  sambungan audio: {len(bounds)} titik, kontinuitas {worst_jump:.2f} "
          f"(batas <{AUDIO_JUMP_LIMIT}), dip {dip_db:+.1f} dB (batas >{AUDIO_DIP_LIMIT_DB}) "
          f"{'LULUS' if ok else 'GAGAL'}")
    return ok


def main():
    p = argparse.ArgumentParser()
    p.add_argument("path")
    p.add_argument("--unit-video", type=int, required=True, help="jumlah frame per unit video")
    p.add_argument("--fps", type=float, default=25.0)
    p.add_argument("--unit-audio", type=float, help="panjang unit audio dalam detik")
    p.add_argument("--audio-xfade", type=float, default=2.0)
    p.add_argument("--sample-rate", type=int, default=44100)
    p.add_argument("--video-sec", type=float, default=90.0)
    p.add_argument("--audio-sec", type=float, default=140.0)
    p.add_argument("--skip-video", action="store_true")
    p.add_argument("--skip-audio", action="store_true")
    args = p.parse_args()

    dur = probe_value(args.path, "format=duration")
    vdur = probe_value(args.path, "stream=duration,nb_frames", "v:0")
    adur = probe_value(args.path, "stream=duration", "a:0")
    print(f"berkas: {args.path}")
    print(f"  durasi wadah {float(dur[0]):.3f} dtk, video {vdur[0]} ({vdur[1] if len(vdur) > 1 else '?'} frame)"
          + (f", audio {float(adur[0]):.3f} dtk" if adur and adur[0] not in ("N/A", "") else ", tanpa audio"))
    if adur and adur[0] not in ("N/A", ""):
        delta = abs(float(dur[0]) - float(adur[0]))
        print(f"  selisih A/V {delta:.3f} dtk {'LULUS' if delta < 0.5 else 'GAGAL'}")

    results = []
    if not args.skip_video:
        results.append(check_video(args.path, args.unit_video, args.fps, args.video_sec))
    if not args.skip_audio:
        if adur and adur[0] not in ("N/A", "") and args.unit_audio:
            results.append(check_audio(args.path, args.unit_audio, args.audio_xfade,
                                       args.audio_sec, args.sample_rate))
        else:
            print("  audio: dilewati (tidak ada audio atau --unit-audio tidak diberikan)")

    checked = [r for r in results if r is not None]
    if not checked:
        print("HASIL: tidak ada yang diuji")
        return 0
    ok = all(checked)
    print("HASIL:", "LULUS" if ok else "GAGAL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
