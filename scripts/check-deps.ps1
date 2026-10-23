<#
.SYNOPSIS
  Memeriksa semua dependensi yang dibutuhkan LofiLoop.

.DESCRIPTION
  Keluaran per dependensi: [OK] / [WARN] / [FAIL] beserta versi.
  Exit code 0 kalau semua dependensi WAJIB lulus, 1 kalau ada yang gagal.
  Skrip hanya membaca dan menjalankan smoke test di folder temp. Tidak memasang apa pun.

.PARAMETER ReportPath
  Kalau diisi, hasil ditulis sebagai laporan markdown (mis. docs\env-report.md).

.PARAMETER SkipSmoke
  Lewati smoke test FFmpeg (generate klip, stream_loop, copy).

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check-deps.ps1 -ReportPath docs\env-report.md
#>
[CmdletBinding()]
param(
    [string]$ReportPath,
    [switch]$SkipSmoke
)

$ErrorActionPreference = 'Continue'

# Segarkan PATH dari registry supaya hasil instalasi baru terdeteksi tanpa membuka terminal baru.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')

$MinDiskGB = 15
$WarnFreeRamMB = 1500
$results = New-Object System.Collections.Generic.List[object]

function Add-Result {
    param(
        [string]$Name,
        [ValidateSet('OK', 'FAIL', 'WARN')][string]$Status,
        [string]$Detail,
        [bool]$Required = $true
    )
    # Dependensi opsional yang gagal hanya jadi peringatan.
    if ($Status -eq 'FAIL' -and -not $Required) { $Status = 'WARN' }
    $results.Add([pscustomobject]@{ Name = $Name; Status = $Status; Detail = $Detail; Required = $Required })
    $color = @{ OK = 'Green'; WARN = 'Yellow'; FAIL = 'Red' }[$Status]
    Write-Host ('[{0,-4}] {1,-34} {2}' -f $Status, $Name, $Detail) -ForegroundColor $color
}

function Invoke-Tool {
    param([string]$Exe, [string[]]$Arguments)
    $cmd = Get-Command $Exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $cmd) { return $null }
    $out = & $cmd.Source @Arguments 2>&1 | Out-String
    return $out.Trim()
}

function Get-FirstLine([string]$Text) {
    if (-not $Text) { return '' }
    return ($Text -split "`r?`n")[0].Trim()
}

Write-Host ''
Write-Host '== LofiLoop: pemeriksaan dependensi ==' -ForegroundColor Cyan

# --- Mesin ---
$os = Get-CimInstance Win32_OperatingSystem
$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$gpu = (Get-CimInstance Win32_VideoController | Select-Object -First 1).Name
$totalRamGB = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1)
$freeRamMB = [math]::Round($os.FreePhysicalMemory / 1KB)
Write-Host ("Mesin: {0} | RAM {1} GB | {2} | {3}" -f $cpu, $totalRamGB, $gpu, $os.Caption)
Write-Host ''

# --- 1. MSVC Build Tools + Windows SDK ---
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path $vswhere) {
    $vcPath = & $vswhere -all -products '*' -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if ($vcPath) {
        # vswhere bisa melapor komponen ada walau isinya kosong, jadi cek link.exe dan cl.exe langsung.
        $msvcRoot = Join-Path $vcPath 'VC\Tools\MSVC'
        $linkExe = $null
        if (Test-Path $msvcRoot) {
            $linkExe = Get-ChildItem $msvcRoot -Directory | Sort-Object Name -Descending |
                ForEach-Object { Join-Path $_.FullName 'bin\Hostx64\x64\link.exe' } |
                Where-Object { Test-Path $_ } | Select-Object -First 1
        }
        if ($linkExe) {
            Add-Result 'MSVC Build Tools (VC++)' 'OK' $linkExe
        } else {
            Add-Result 'MSVC Build Tools (VC++)' 'FAIL' "Komponen terdaftar di $vcPath tapi link.exe/cl.exe tidak ada. Repair/Modify lewat Visual Studio Installer."
        }
    } else {
        Add-Result 'MSVC Build Tools (VC++)' 'FAIL' 'Komponen "MSVC x64/x86 build tools" tidak ada. Tambahkan lewat Visual Studio Installer.'
    }
} else {
    Add-Result 'MSVC Build Tools (VC++)' 'FAIL' 'Visual Studio Installer / vswhere tidak ditemukan.'
}

$kitsRoot = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue).KitsRoot10
if (-not $kitsRoot) { $kitsRoot = (Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue).KitsRoot10 }
$sdkVer = $null
if ($kitsRoot -and (Test-Path (Join-Path $kitsRoot 'Lib'))) {
    $sdkVer = Get-ChildItem (Join-Path $kitsRoot 'Lib') -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName 'um\x64\kernel32.lib') } |
        Sort-Object Name -Descending | Select-Object -First 1
}
if ($sdkVer) {
    Add-Result 'Windows SDK (kernel32.lib x64)' 'OK' $sdkVer.Name
} else {
    Add-Result 'Windows SDK (kernel32.lib x64)' 'FAIL' 'Tidak ditemukan. Tambahkan "Windows 11 SDK" lewat Visual Studio Installer.'
}

# --- 2. Rust ---
$rustc = Invoke-Tool 'rustc' @('--version')
if ($rustc) {
    Add-Result 'Rust (rustc)' 'OK' (Get-FirstLine $rustc)
    $cargo = Invoke-Tool 'cargo' @('--version')
    if ($cargo) { Add-Result 'Cargo' 'OK' (Get-FirstLine $cargo) } else { Add-Result 'Cargo' 'FAIL' 'cargo tidak ditemukan' }

    $verbose = Invoke-Tool 'rustc' @('-vV')
    if ($verbose -match 'host:\s*x86_64-pc-windows-msvc') {
        Add-Result 'Rust target msvc' 'OK' 'x86_64-pc-windows-msvc'
    } else {
        Add-Result 'Rust target msvc' 'FAIL' 'Host bukan x86_64-pc-windows-msvc (mungkin toolchain gnu).'
    }

    $components = Invoke-Tool 'rustup' @('component', 'list', '--installed')
    foreach ($c in 'clippy', 'rustfmt') {
        if ($components -match "(?m)^$c") { Add-Result "Rust komponen $c" 'OK' 'terpasang' }
        else { Add-Result "Rust komponen $c" 'FAIL' "Jalankan: rustup component add $c" }
    }
} else {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe'
    if (Test-Path $cargoBin) {
        Add-Result 'Rust (rustc)' 'FAIL' "Ada di $cargoBin tapi tidak ada di PATH. Buka terminal baru."
    } else {
        Add-Result 'Rust (rustc)' 'FAIL' 'Belum terpasang (pasang lewat rustup-init.exe).'
    }
}

# --- 3. WebView2 Runtime ---
$wv2Keys = @(
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
    'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
    'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
)
$wv2 = $wv2Keys | ForEach-Object { (Get-ItemProperty $_ -ErrorAction SilentlyContinue).pv } | Where-Object { $_ } | Select-Object -First 1
if ($wv2) { Add-Result 'WebView2 Runtime' 'OK' $wv2 } else { Add-Result 'WebView2 Runtime' 'FAIL' 'Tidak terdeteksi.' }

# --- 4. Node, npm, Git, Python ---
$node = Invoke-Tool 'node' @('--version')
if ($node -match 'v(\d+)\.') {
    if ([int]$Matches[1] -ge 18) { Add-Result 'Node.js' 'OK' $node } else { Add-Result 'Node.js' 'FAIL' "$node (butuh >= 18)" }
} else { Add-Result 'Node.js' 'FAIL' 'Belum terpasang.' }

$npm = Invoke-Tool 'npm' @('--version')
if ($npm) { Add-Result 'npm' 'OK' (Get-FirstLine $npm) } else { Add-Result 'npm' 'FAIL' 'Belum terpasang.' }

$git = Invoke-Tool 'git' @('--version')
if ($git) { Add-Result 'Git' 'OK' (Get-FirstLine $git) } else { Add-Result 'Git' 'FAIL' 'Belum terpasang.' }

$py = Invoke-Tool 'python' @('--version')
if ($py -match 'Python 3') { Add-Result 'Python 3 (dev, opsional)' 'OK' (Get-FirstLine $py) -Required $false }
else { Add-Result 'Python 3 (dev, opsional)' 'FAIL' 'Tidak ditemukan.' -Required $false }

# --- 5. Tauri CLI ---
$tauri = Invoke-Tool 'tauri' @('--version')
if ($tauri -notmatch 'tauri-cli\s+\d' -and (Get-Command cargo -ErrorAction SilentlyContinue)) { $tauri = Invoke-Tool 'cargo' @('tauri', '--version') }
if ($tauri -notmatch 'tauri-cli\s+\d') {
    if (Get-Command npm -ErrorAction SilentlyContinue) {
        $global = Invoke-Tool 'npm' @('ls', '-g', '--depth=0', '@tauri-apps/cli')
        if ($global -match '@tauri-apps/cli@([\d\.]+)') { $tauri = "tauri-cli (npm) $($Matches[1])" }
    }
}
if ($tauri -match '(tauri-cli[^\r\n]*\d[^\r\n]*)') { Add-Result 'Tauri CLI v2' 'OK' $Matches[1].Trim() }
else { Add-Result 'Tauri CLI v2' 'FAIL' 'Belum terpasang (npm install -g @tauri-apps/cli).' }

# --- 6. FFmpeg + FFprobe ---
$ffmpegOk = $false
$ffmpegVer = Invoke-Tool 'ffmpeg' @('-hide_banner', '-version')
$ffprobeVer = Invoke-Tool 'ffprobe' @('-hide_banner', '-version')
if ($ffmpegVer) { Add-Result 'FFmpeg' 'OK' (Get-FirstLine $ffmpegVer); $ffmpegOk = $true }
else { Add-Result 'FFmpeg' 'FAIL' 'Belum ada di PATH.' }
if ($ffprobeVer) { Add-Result 'FFprobe' 'OK' (Get-FirstLine $ffprobeVer) } else { Add-Result 'FFprobe' 'FAIL' 'Belum ada di PATH.' }

if ($ffmpegOk) {
    $encoders = Invoke-Tool 'ffmpeg' @('-hide_banner', '-encoders')
    $filters = Invoke-Tool 'ffmpeg' @('-hide_banner', '-filters')

    foreach ($e in @(
            @{ n = 'libx264'; req = $true }, @{ n = 'aac'; req = $true }, @{ n = 'h264_qsv'; req = $false })) {
        if ($encoders -match "\s$($e.n)\s") { Add-Result "FFmpeg encoder $($e.n)" 'OK' 'tersedia' -Required $e.req }
        else { Add-Result "FFmpeg encoder $($e.n)" 'FAIL' 'tidak ada di build ini' -Required $e.req }
    }
    foreach ($f in 'xfade', 'acrossfade', 'loudnorm', 'atempo') {
        if ($filters -match "\s$f\s") { Add-Result "FFmpeg filter $f" 'OK' 'tersedia' }
        else { Add-Result "FFmpeg filter $f" 'FAIL' 'tidak ada di build ini' }
    }

    # Smoke test: unit loop closed-GOP -> stream_loop -c copy -> durasi hasil.
    if (-not $SkipSmoke) {
        $tmp = Join-Path $env:TEMP ("lofiloop-check-" + $PID)
        New-Item -ItemType Directory -Path $tmp -Force | Out-Null
        try {
            $unit = Join-Path $tmp 'unit.mp4'
            $looped = Join-Path $tmp 'looped.mp4'
            & ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'testsrc=d=2:s=320x180:r=25' `
                -c:v libx264 -preset ultrafast -g 50 -keyint_min 50 -sc_threshold 0 -pix_fmt yuv420p $unit 2>&1 | Out-Null
            $genOk = ($LASTEXITCODE -eq 0) -and (Test-Path $unit)
            if ($genOk) {
                & ffmpeg -hide_banner -loglevel error -y -stream_loop 2 -i $unit -c copy $looped 2>&1 | Out-Null
                $loopOk = ($LASTEXITCODE -eq 0) -and (Test-Path $looped)
                if ($loopOk -and $ffprobeVer) {
                    $dur = (& ffprobe -v error -show_entries format=duration -of csv=p=0 $looped 2>&1 | Out-String).Trim()
                    $d = 0.0
                    if ([double]::TryParse($dur, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$d) -and [math]::Abs($d - 6.0) -lt 0.3) {
                        Add-Result 'Smoke: stream_loop + copy' 'OK' ("klip 2 dtk x3 = {0:N2} dtk" -f $d)
                    } else {
                        Add-Result 'Smoke: stream_loop + copy' 'FAIL' "durasi hasil tidak sesuai (baca: '$dur', harapan ~6.0)"
                    }
                } else {
                    Add-Result 'Smoke: stream_loop + copy' 'FAIL' 'stream_loop -c copy gagal / ffprobe tidak ada'
                }
            } else {
                Add-Result 'Smoke: stream_loop + copy' 'FAIL' 'gagal membuat klip uji dengan libx264'
            }

            # Tes encode hardware (hanya peringatan): UHD 605 harusnya mendukung QSV H.264.
            & ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'testsrc=d=1:s=320x180:r=25' -c:v h264_qsv (Join-Path $tmp 'qsv.mp4') 2>&1 | Out-Null
            if ($LASTEXITCODE -eq 0) { Add-Result 'Smoke: encode h264_qsv' 'OK' 'GPU Intel QSV berfungsi' -Required $false }
            else { Add-Result 'Smoke: encode h264_qsv' 'FAIL' 'QSV tidak jalan, pakai libx264' -Required $false }
        } finally {
            Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
        }
    }
}

# --- 10. Pemutar uji ---
$vlcPaths = @(
    (Join-Path $env:ProgramFiles 'VideoLAN\VLC\vlc.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'VideoLAN\VLC\vlc.exe'),
    'C:\tools\vlc\vlc.exe'
)
$vlc = $vlcPaths | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($vlc) { Add-Result 'VLC (pemutar uji)' 'OK' $vlc } else { Add-Result 'VLC (pemutar uji)' 'FAIL' 'Belum terpasang.' }

$chromePaths = @(
    (Join-Path $env:ProgramFiles 'Google\Chrome\Application\chrome.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Google\Chrome\Application\chrome.exe'),
    (Join-Path $env:LOCALAPPDATA 'Google\Chrome\Application\chrome.exe')
)
$chrome = $chromePaths | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($chrome) { Add-Result 'Chrome (pemutar uji, opsional)' 'OK' $chrome -Required $false }
else { Add-Result 'Chrome (pemutar uji, opsional)' 'FAIL' 'Tidak ada. Edge (WebView2) tetap bisa dipakai uji.' -Required $false }

# --- 11. Disk & RAM ---
$drive = (Get-Item $PSScriptRoot).PSDrive
$freeGB = [math]::Round($drive.Free / 1GB, 1)
if ($freeGB -ge $MinDiskGB) { Add-Result "Ruang disk ($($drive.Name):)" 'OK' "$freeGB GB bebas (min $MinDiskGB)" }
else { Add-Result "Ruang disk ($($drive.Name):)" 'FAIL' "$freeGB GB bebas, butuh >= $MinDiskGB" }

if ($freeRamMB -ge $WarnFreeRamMB) { Add-Result 'RAM bebas' 'OK' "$freeRamMB MB bebas dari $totalRamGB GB" -Required $false }
else { Add-Result 'RAM bebas' 'WARN' "$freeRamMB MB bebas, tutup aplikasi lain sebelum build" -Required $false }

# --- Ringkasan ---
$failed = @($results | Where-Object { $_.Status -eq 'FAIL' })
$warned = @($results | Where-Object { $_.Status -eq 'WARN' })
$passed = @($results | Where-Object { $_.Status -eq 'OK' })

Write-Host ''
Write-Host ("Ringkasan: {0} OK, {1} peringatan, {2} gagal (wajib)" -f $passed.Count, $warned.Count, $failed.Count) -ForegroundColor Cyan
if ($failed.Count -eq 0) {
    Write-Host 'GERBANG FASE 0: LULUS. Boleh lanjut ke Fase 1.' -ForegroundColor Green
} else {
    Write-Host 'GERBANG FASE 0: BELUM LULUS. Perbaiki yang gagal lalu jalankan ulang.' -ForegroundColor Red
    $failed | ForEach-Object { Write-Host ("  - {0}: {1}" -f $_.Name, $_.Detail) -ForegroundColor Red }
}

# --- Laporan markdown ---
if ($ReportPath) {
    $dir = Split-Path -Parent $ReportPath
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add('# Laporan lingkungan LofiLoop')
    $lines.Add('')
    $lines.Add("- Waktu: $(Get-Date -Format 'yyyy-MM-dd HH:mm')")
    $lines.Add("- Mesin: $cpu, RAM $totalRamGB GB, $gpu")
    $lines.Add("- OS: $($os.Caption) ($($os.Version))")
    $lines.Add("- Pemeriksaan dependensi: " + $(if ($failed.Count -eq 0) { '**LULUS**' } else { '**BELUM LULUS**' }))
    $lines.Add('')
    $lines.Add('| Status | Dependensi | Detail | Wajib |')
    $lines.Add('|---|---|---|---|')
    foreach ($r in $results) {
        $detail = ($r.Detail -replace '\|', '/')
        $req = if ($r.Required) { 'ya' } else { 'tidak' }
        $lines.Add("| $($r.Status) | $($r.Name) | $detail | $req |")
    }
    $lines | Out-File -FilePath $ReportPath -Encoding utf8
    Write-Host "Laporan ditulis: $ReportPath"
}

if ($failed.Count -eq 0) { exit 0 } else { exit 1 }
