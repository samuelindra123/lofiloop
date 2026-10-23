"""Metrik sambungan audio loop: kontinuitas di batas unit dan dip level di zona crossfade.

Layout unit: [... konten ...][crossfade xf detik] lalu ulang. Batas unit = kelipatan unit_s.

1. Kontinuitas: selisih sample berurutan (|x[n]-x[n-1]|) terbesar dalam +-2 ms di batas unit,
   dibanding selisih terbesar di tempat lain. Rasio ~1 = tidak ada klik. Klik tajam = rasio besar.
2. Dip level: RMS jendela 100 ms. Minimum di zona crossfade dibanding minimum alami di luar zona.

Pakai: python -I audio_seam_metric.py <file> <unit_s> <xf_s> <start_s> <dur_s> [sr]
  start_s = offset awal yang didecode (kelipatan unit_s supaya batas jatuh tepat)
"""
import subprocess
import sys

import numpy as np

path = sys.argv[1]
unit_s, xf_s, start_s, dur_s = (float(x) for x in sys.argv[2:6])
sr = int(sys.argv[6]) if len(sys.argv) > 6 else 44100

raw = subprocess.run(
    ["ffmpeg", "-v", "error", "-ss", str(start_s), "-t", str(dur_s), "-i", path,
     "-ac", "2", "-ar", str(sr), "-f", "f32le", "-"], capture_output=True, check=True).stdout
x = np.frombuffer(raw, dtype=np.float32).reshape(-1, 2)
n = len(x)
print(f"sample didecode: {n} ({n / sr:.1f}s)")

unit = int(round(unit_s * sr))
xf = int(round(xf_s * sr))
bounds = [b for b in range(unit, n - 1, unit)]
if not bounds:
    sys.exit("tidak ada batas unit di rentang ini")

d = np.abs(np.diff(x, axis=0)).max(axis=1)  # d[i] = |x[i+1]-x[i]|
guard = int(0.002 * sr)
mask = np.ones(len(d), bool)
for b in bounds:
    mask[max(0, b - 1 - guard):b - 1 + guard + 1] = False
base_max = d[mask].max()
base_p999 = np.percentile(d[mask], 99.9)

worst_jump = 0.0
for b in bounds:
    seg = d[max(0, b - 1 - guard):b - 1 + guard + 1]
    worst_jump = max(worst_jump, seg.max() / base_max)
print(f"kontinuitas: selisih sample terbesar di batas / terbesar di tempat lain = {worst_jump:.2f} "
      f"(base maks {base_max:.5f}, p99.9 {base_p999:.5f})")

win = int(0.1 * sr)
nw = n // win
rms = np.sqrt((x[:nw * win].astype(np.float64) ** 2).mean(axis=1).reshape(nw, win).mean(axis=1))
t = (np.arange(nw) + 0.5) * win / sr  # detik

in_xf = np.zeros(nw, bool)
for b in bounds + [0]:
    lo, hi = (b - xf) / sr, b / sr
    in_xf |= (t >= lo) & (t <= hi)
nat_min = rms[~in_xf].min()
xf_min = rms[in_xf].min() if in_xf.any() else float("nan")
dip_db = 20 * np.log10(xf_min / nat_min)
print(f"RMS(100ms): minimum alami {nat_min:.4f}, minimum di zona crossfade {xf_min:.4f}  ({dip_db:+.1f} dB)")

ok_jump = worst_jump < 1.2
ok_dip = dip_db > -3.0
print(f"kontinuitas {'LULUS' if ok_jump else 'GAGAL'} (batas <1.2), dip {'LULUS' if ok_dip else 'GAGAL'} (batas > -3 dB)")
print("HASIL:", "LULUS" if ok_jump and ok_dip else "GAGAL")
sys.exit(0 if ok_jump and ok_dip else 1)
