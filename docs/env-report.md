# Laporan lingkungan LofiLoop

- Waktu: 2026-10-22 13:28
- Mesin: Intel(R) Pentium(R) Silver N5000 CPU @ 1.10GHz, RAM 3.8 GB, Intel(R) UHD Graphics 605
- OS: Microsoft Windows 11 Pro (10.0.28000)
- Gerbang Fase 0: **LULUS**

| Status | Dependensi | Detail | Wajib |
|---|---|---|---|
| OK | MSVC Build Tools (VC++) | C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC\14.51.36231\bin\Hostx64\x64\link.exe | ya |
| OK | Windows SDK (kernel32.lib x64) | 10.0.26100.0 | ya |
| OK | Rust (rustc) | rustc 1.99.1 (58cf8f9c3 2026-10-13) | ya |
| OK | Cargo | cargo 1.99.1 (5f94df478 2026-08-27) | ya |
| OK | Rust target msvc | x86_64-pc-windows-msvc | ya |
| OK | Rust komponen clippy | terpasang | ya |
| OK | Rust komponen rustfmt | terpasang | ya |
| OK | WebView2 Runtime | 157.0.4341.10 | ya |
| OK | Node.js | v24.22.1 | ya |
| OK | npm | 11.19.1 | ya |
| OK | Git | git version 2.56.0.windows.2 | ya |
| OK | Python 3 (dev, opsional) | Python 3.15.1 | tidak |
| OK | Tauri CLI v2 | tauri-cli 2.12.4 | ya |
| OK | FFmpeg | ffmpeg version 9.0.2-essentials_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers | ya |
| OK | FFprobe | ffprobe version 9.0.2-essentials_build-www.gyan.dev Copyright (c) 2007-2026 the FFmpeg developers | ya |
| OK | FFmpeg encoder libx264 | tersedia | ya |
| OK | FFmpeg encoder aac | tersedia | ya |
| OK | FFmpeg encoder h264_qsv | tersedia | tidak |
| OK | FFmpeg filter xfade | tersedia | ya |
| OK | FFmpeg filter acrossfade | tersedia | ya |
| OK | FFmpeg filter loudnorm | tersedia | ya |
| OK | FFmpeg filter atempo | tersedia | ya |
| OK | Smoke: stream_loop + copy | klip 2 dtk x3 = 6.00 dtk | ya |
| OK | Smoke: encode h264_qsv | GPU Intel QSV berfungsi | tidak |
| OK | VLC (pemutar uji) | C:\tools\vlc\vlc.exe | ya |
| WARN | Chrome (pemutar uji, opsional) | Tidak ada. Edge (WebView2) tetap bisa dipakai uji. | tidak |
| OK | Ruang disk (C:) | 35.1 GB bebas (min 15) | ya |
| WARN | RAM bebas | 943 MB bebas, tutup aplikasi lain sebelum build | tidak |
