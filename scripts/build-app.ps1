<#
.SYNOPSIS
  Bangun aplikasi LofiLoop: frontend dulu, lalu binary Tauri.

.DESCRIPTION
  Dijalankan dari akar repositori. Frontend dibangun dengan Vite, lalu Tauri
  meng-embed hasilnya ke dalam exe. Tanpa langkah ini, jendela bisa terbuka
  tetapi kosong karena aset frontend tidak ikut ter-embed.
#>
param([switch]$Release)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
Set-Location $root

Write-Host '== 1/2 membangun frontend (Vite) ==' -ForegroundColor Cyan
Push-Location (Join-Path $root 'app\ui')
try {
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "build frontend gagal (exit $LASTEXITCODE)" }
} finally { Pop-Location }

Write-Host '== 2/2 membangun aplikasi Tauri ==' -ForegroundColor Cyan
Push-Location (Join-Path $root 'app\src-tauri')
try {
    $cliArgs = @('build', '--no-bundle')
    if (-not $Release) { $cliArgs += '--debug' }
    & npx --yes '@tauri-apps/cli@2' @cliArgs
    if ($LASTEXITCODE -ne 0) { throw "build Tauri gagal (exit $LASTEXITCODE)" }
} finally { Pop-Location }

$profile = if ($Release) { 'release' } else { 'debug' }
$exe = Join-Path $root "target\$profile\lofiloop-app.exe"
Write-Host ''
if (Test-Path $exe) {
    $mb = [math]::Round((Get-Item $exe).Length / 1MB, 1)
    Write-Host "selesai: $exe ($mb MB)" -ForegroundColor Green
} else {
    throw "berkas hasil tidak ditemukan: $exe"
}
