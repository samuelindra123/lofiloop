"""Periksa sambungan video hasil stream_loop -c copy di level paket (tanpa decode).

Memeriksa: durasi, jumlah frame, DTS monoton naik, jarak antar-frame konsisten (tidak ada
lompatan di batas unit), dan keyframe tepat di setiap awal unit loop.

Pakai: python -I check_video_seams.py <file.mp4> <unit_frames> <fps>
"""
import subprocess
import sys

path, unit_frames, fps = sys.argv[1], int(sys.argv[2]), float(sys.argv[3])
frame_dur = 1.0 / fps

out = subprocess.run(
    ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
     "packet=pts_time,dts_time,flags", "-of", "csv=p=0", path],
    capture_output=True, text=True, check=True).stdout.splitlines()

pts, dts, keys = [], [], []
for i, line in enumerate(out):
    p, d, f = line.split(",")[:3]
    pts.append(float(p))
    dts.append(float(d))
    if "K" in f:
        keys.append(i)

n = len(pts)
print(f"paket video: {n}")

bad_dts = sum(1 for a, b in zip(dts, dts[1:]) if b <= a)
print(f"DTS tidak monoton naik: {bad_dts}")

gaps = [b - a for a, b in zip(dts, dts[1:])]
off = [i for i, g in enumerate(gaps) if abs(g - frame_dur) > frame_dur * 0.01]
print(f"jarak DTS menyimpang >1% dari {frame_dur * 1000:.1f} ms: {len(off)}")
if off:
    print("  contoh indeks:", off[:5], "gap:", [round(gaps[i], 5) for i in off[:5]])

expected_boundaries = list(range(0, n, unit_frames))
missing = [b for b in expected_boundaries if b not in set(keys)]
print(f"batas unit: {len(expected_boundaries)}, tanpa keyframe: {len(missing)}")
if missing:
    print("  contoh:", missing[:5])

gop = [b - a for a, b in zip(keys, keys[1:])]
print(f"GOP: min {min(gop)}, maks {max(gop)} frame; jumlah keyframe {len(keys)}")

ok = bad_dts == 0 and not off and not missing
print("HASIL:", "LULUS" if ok else "GAGAL")
sys.exit(0 if ok else 1)
