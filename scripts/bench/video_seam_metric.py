"""Metrik sambungan video: selisih frame di batas unit dibanding selisih frame tetangga.

Decode N detik pertama (grayscale 160x90), hitung mean abs diff antar-frame berurutan,
lalu bandingkan selisih di batas unit loop dengan median selisih frame biasa.
Rasio < 1.5 dianggap mulus (target di PLAN.md).

Pakai: python -I video_seam_metric.py <file.mp4> <unit_frames> <seconds> [fps]
"""
import subprocess
import sys

import numpy as np

path, unit_frames, seconds = sys.argv[1], int(sys.argv[2]), float(sys.argv[3])
fps = float(sys.argv[4]) if len(sys.argv) > 4 else 25.0
W, H = 160, 90

raw = subprocess.run(
    ["ffmpeg", "-v", "error", "-i", path, "-t", str(seconds), "-vf", f"scale={W}:{H},format=gray",
     "-f", "rawvideo", "-"], capture_output=True, check=True).stdout
frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, H, W).astype(np.int16)
diffs = np.abs(np.diff(frames, axis=0)).mean(axis=(1, 2))  # diffs[i] = frame i -> i+1
n = len(frames)
print(f"frame didecode: {n}  selisih median: {np.median(diffs):.3f}  maks: {diffs.max():.3f}")

boundaries = [b for b in range(unit_frames, n, unit_frames)]
ratios = []
NEIGHBORS = 10  # pembanding = rata-rata selisih 10 frame sebelum dan 10 sesudah (tanpa sambungan itu sendiri)
for b in boundaries:
    s = b - 1  # frame b-1 -> b melintasi sambungan
    seam = diffs[s]
    around = np.concatenate([diffs[max(0, s - NEIGHBORS):s], diffs[s + 1:s + 1 + NEIGHBORS]])
    ratio = seam / max(around.mean(), 1e-6)
    ratios.append(ratio)
    print(f"  sambungan @ frame {b} ({b / fps:.1f}s): selisih {seam:.3f}  tetangga {around.mean():.3f}  rasio {ratio:.2f}")

worst = max(ratios) if ratios else 0.0
print(f"rasio terburuk: {worst:.2f} (batas 1.5)")
print("HASIL:", "LULUS" if worst < 1.5 else "GAGAL")
sys.exit(0 if worst < 1.5 else 1)
