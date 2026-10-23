<#
.SYNOPSIS
  Bangun installer LofiLoop untuk Windows: aplikasi + FFmpeg dalam satu berkas.

.DESCRIPTION
  Tiga langkah:
    1. Bangun aplikasi mode release (kalau belum ada).
    2. Siapkan FFmpeg: pakai yang sudah ada di mesin, atau unduh build resmi gyan.dev
       lalu verifikasi SHA256.
    3. Susun installer dengan Inno Setup, sekaligus memasukkan FFmpeg ke dalamnya.

  Hasilnya di folder dist-installer/: satu berkas exe siap dibagikan, plus versi portabel.

.PARAMETER SkipAppBuild
  Pakai exe release yang sudah ada, jangan bangun ulang.

.PARAMETER FfmpegSource
  Folder berisi ffmpeg.exe dan ffprobe.exe yang mau disertakan. Kalau tidak diisi,
  skrip mencari di PATH, lalu mengunduh kalau tidak ketemu.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-installer.ps1
#>
[CmdletBinding()]
param(
    [switch]$SkipAppBuild,
    [string]$FfmpegSource
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
            [Environment]::GetEnvironmentVariable('Path', 'User')
Set-Location $root

function Langkah($n, $teks) {
    Write-Host ''
    Write-Host "== $n $teks ==" -ForegroundColor Cyan
}

function Gagal($pesan) {
    Write-Host "GAGAL: $pesan" -ForegroundColor Red
    exit 1
}

# ---------------------------------------------------------------------------
# Versi diambil dari Cargo.toml aplikasi supaya tidak ada dua sumber kebenaran.
# ---------------------------------------------------------------------------
$cargoToml = Join-Path $root 'app\src-tauri\Cargo.toml'
$versi = (Select-String -Path $cargoToml -Pattern '^version\s*=\s*"([^"]+)"' |
          Select-Object -First 1).Matches.Groups[1].Value
if (-not $versi) { Gagal "tidak bisa membaca versi dari $cargoToml" }
Write-Host "LofiLoop versi $versi"

$dist = Join-Path $root 'dist-installer'
New-Item -ItemType Directory -Force -Path $dist | Out-Null

# ---------------------------------------------------------------------------
# 1. Aplikasi release
# ---------------------------------------------------------------------------
Langkah '1/4' 'menyiapkan aplikasi mode release'
$exe = Join-Path $root 'target\release\lofiloop-app.exe'
if ($SkipAppBuild -and (Test-Path $exe)) {
    Write-Host "dilewati, memakai yang ada: $exe"
} else {
    & (Join-Path $PSScriptRoot 'build-app.ps1') -Release
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $exe)) { Gagal 'build aplikasi gagal' }
}
$mbExe = [math]::Round((Get-Item $exe).Length / 1MB, 1)
Write-Host "aplikasi: $exe ($mbExe MB)"

# ---------------------------------------------------------------------------
# 2. FFmpeg: cari yang ada dulu, unduh kalau perlu
# ---------------------------------------------------------------------------
Langkah '2/4' 'menyiapkan FFmpeg'

function Cari-Ffmpeg {
    param([string]$Mulai)

    if ($Mulai) {
        $kandidat = Join-Path $Mulai 'ffmpeg.exe'
        if (Test-Path $kandidat) {
            # Kalau yang ditunjuk folder induk yang berisi bin/, telusuri satu tingkat.
            return (Split-Path -Parent $kandidat)
        }
        $dalamBin = Join-Path $Mulai 'bin\ffmpeg.exe'
        if (Test-Path $dalamBin) { return (Split-Path -Parent $dalamBin) }
    }

    # Dari PATH.
    $diPath = Get-Command ffmpeg.exe -ErrorAction SilentlyContinue
    if ($diPath) { return (Split-Path -Parent $diPath.Source) }

    # Dari konfigurasi yang pernah disimpan aplikasi.
    $cfg = Join-Path $env:APPDATA 'lofiloop\config.json'
    if (Test-Path $cfg) {
        try {
            $isi = Get-Content $cfg -Raw | ConvertFrom-Json
            if ($isi.ffmpeg_dir -and (Test-Path (Join-Path $isi.ffmpeg_dir 'ffmpeg.exe'))) {
                return $isi.ffmpeg_dir
            }
        } catch { }
    }
    return $null
}

$ffDir = Cari-Ffmpeg -Mulai $FfmpegSource
if ($ffDir) {
    Write-Host "FFmpeg ditemukan: $ffDir"
} else {
    Write-Host 'FFmpeg belum ada, mengunduh build resmi (sekitar 90 MB)...'
    $zip = Join-Path $env:TEMP 'lofiloop-ffmpeg.zip'
    $shaFile = "$zip.sha256"
    $url = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'

    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    Invoke-WebRequest -Uri "$url.sha256" -OutFile $shaFile -UseBasicParsing

    # Verifikasi SHA256 sebelum dipakai. Berkas yang tidak cocok tidak akan diekstrak.
    $harap = ((Get-Content $shaFile -Raw).Trim() -split '\s+')[0].ToLower()
    $dapat = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    if ($harap -ne $dapat) {
        Gagal "SHA256 tidak cocok. harap $harap, dapat $dapat"
    }
    Write-Host "SHA256 cocok: $dapat"

    $ekstrak = Join-Path $env:TEMP 'lofiloop-ffmpeg-extract'
    Remove-Item $ekstrak -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive -Path $zip -DestinationPath $ekstrak -Force

    $ffExe = Get-ChildItem $ekstrak -Recurse -Filter 'ffmpeg.exe' | Select-Object -First 1
    if (-not $ffExe) { Gagal 'ffmpeg.exe tidak ditemukan di dalam arsip' }
    $ffDir = $ffExe.DirectoryName
    Write-Host "FFmpeg siap: $ffDir"
}

foreach ($alat in @('ffmpeg.exe', 'ffprobe.exe')) {
    if (-not (Test-Path (Join-Path $ffDir $alat))) { Gagal "$alat tidak ada di $ffDir" }
}

# ---------------------------------------------------------------------------
# 3. Inno Setup
# ---------------------------------------------------------------------------
Langkah '3/4' 'menyiapkan Inno Setup'
$iscc = $null
foreach ($kandidat in @(
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno\ISCC.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 7\ISCC.exe'))) {
    if ($kandidat -and (Test-Path $kandidat)) { $iscc = $kandidat; break }
}
if (-not $iscc) {
    $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($cmd) { $iscc = $cmd.Source }
}

# Kalau belum ketemu, telusuri folder Programs sekali. Lokasi pemasangan Inno berbeda
# antar versi (6 memakai 'Inno Setup 6', 7 memakai 'Inno'), jadi menebak satu jalur
# saja mudah meleset.
if (-not $iscc) {
    $akar = @((Join-Path $env:LOCALAPPDATA 'Programs'), $env:ProgramFiles, ${env:ProgramFiles(x86)})
    foreach ($a in $akar) {
        if (-not $a -or -not (Test-Path $a)) { continue }
        $temu = Get-ChildItem $a -Recurse -Depth 3 -Filter 'ISCC.exe' -ErrorAction SilentlyContinue |
                Select-Object -First 1
        if ($temu) { $iscc = $temu.FullName; break }
    }
}

if (-not $iscc) {
    # Inno Setup TIDAK diunduh otomatis di sini. Situsnya mengembalikan halaman HTML
    # untuk URL unduhan langsung, dan memasang pemasang tak dikenal tanpa pemeriksaan
    # bukan hal yang pantas dilakukan skrip build. Cukup beri tahu perintahnya.
    $winget = Get-Command winget.exe -ErrorAction SilentlyContinue
    if ($winget) {
        Write-Host 'Inno Setup belum ada, mencoba memasang lewat winget...' -ForegroundColor Yellow
        try {
            & winget install --id JRSoftware.InnoSetup --accept-source-agreements `
                --accept-package-agreements --silent 2>&1 | Out-Null
        } catch { }
        Start-Sleep 3
        foreach ($kandidat in @(
                (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
                (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
                (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'))) {
            if ($kandidat -and (Test-Path $kandidat)) { $iscc = $kandidat; break }
        }
    }
}

if (-not $iscc) {
    Write-Host ''
    Write-Host 'Inno Setup 6 belum terpasang, jadi installer exe belum bisa dibuat.' -ForegroundColor Yellow
    Write-Host ''
    Write-Host 'Yang sudah jadi: versi portabel (aplikasi + FFmpeg dalam satu zip).'
    Write-Host ''
    Write-Host 'Untuk membuat installer, pasang Inno Setup dulu:'
    Write-Host ''
    Write-Host '  winget install JRSoftware.InnoSetup' -ForegroundColor White
    Write-Host ''
    Write-Host 'lalu jalankan skrip ini lagi. Unduhan manual: https://jrsoftware.org/isdl.php'
    Write-Host ''
    $iscc = $null
}

Write-Host "Inno Setup: $iscc"

# ---------------------------------------------------------------------------
# 4. Susun installer dan versi portabel
# ---------------------------------------------------------------------------
Langkah '4/4' 'menyusun installer dan versi portabel'

if ($iscc) {
    & $iscc "/DAppVersion=$versi" "/DSourceExe=$exe" "/DFfmpegDir=$ffDir" `
        "/DOutputDir=$dist" (Join-Path $PSScriptRoot 'installer.iss')
    if ($LASTEXITCODE -ne 0) { Gagal "Inno Setup gagal (exit $LASTEXITCODE)" }
}

# Versi portabel: aplikasi + FFmpeg dalam satu zip, tanpa pemasangan.
$portabel = Join-Path $dist "LofiLoop-$versi-portable"
if (Test-Path $portabel) { Remove-Item $portabel -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $portabel 'ffmpeg') | Out-Null
Copy-Item $exe $portabel
Copy-Item (Join-Path $ffDir 'ffmpeg.exe') (Join-Path $portabel 'ffmpeg')
Copy-Item (Join-Path $ffDir 'ffprobe.exe') (Join-Path $portabel 'ffmpeg')
Copy-Item (Join-Path $root 'README.md') $portabel
Copy-Item (Join-Path $root 'LICENSE') $portabel

$zipPortabel = Join-Path $dist "LofiLoop-$versi-portable.zip"
Remove-Item $zipPortabel -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $portabel '*') -DestinationPath $zipPortabel
Remove-Item $portabel -Recurse -Force

Write-Host ''
Write-Host 'selesai. Berkas di folder dist-installer:' -ForegroundColor Green
Get-ChildItem $dist -File | ForEach-Object {
    $mb = [math]::Round($_.Length / 1MB, 1)
    Write-Host ("  {0,-42} {1,8} MB" -f $_.Name, $mb)
}
