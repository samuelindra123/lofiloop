<#
.SYNOPSIS
  Ukur RAM idle aplikasi GUI: working set proses utama + proses anak (mis. WebView2).

.EXAMPLE
  powershell -NoProfile -File scripts\spike\measure_ram.ps1 -Exe spike-tauri.exe -Label tauri -Seconds 25
#>
param(
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][string]$Label,
    [int]$Seconds = 25,
    [string]$OutDir
)

if (-not $OutDir) { $OutDir = Join-Path $PSScriptRoot '..\..\work\logs' }
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

# Snapshot proses WebView2 yang sudah ada SEBELUM aplikasi jalan, supaya hanya anak baru yang dihitung.
$wv2Before = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)

$proc = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds $Seconds   # diamkan dulu, biar startup selesai dan ukurannya stabil

function Get-Tree {
    param([int]$RootId)
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    $tree = @($RootId)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($p in $all) {
            if (($tree -contains $p.ParentProcessId) -and ($tree -notcontains $p.ProcessId)) {
                $tree += $p.ProcessId
                $changed = $true
            }
        }
    }
    return $tree
}

$ids = @(Get-Tree -RootId $proc.Id | Sort-Object -Unique)
# WebView2 muncul sebagai proses terpisah yang bukan anak langsung, jadi disaring pakai snapshot awal.
# Proses yang sudah masuk $ids dilewati supaya tidak dihitung dua kali.
$wv2Now = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue |
        Where-Object { $wv2Before -notcontains $_.Id -and $ids -notcontains $_.Id })
$rows = @()
foreach ($id in $ids) {
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($p) {
        $rows += [pscustomobject]@{ pid = $id; name = $p.ProcessName; working_mb = [math]::Round($p.WorkingSet64 / 1MB, 1); private_mb = [math]::Round($p.PrivateMemorySize64 / 1MB, 1) }
    }
}
foreach ($p in $wv2Now) {
    $rows += [pscustomobject]@{ pid = $p.Id; name = 'msedgewebview2 (bukan anak)'; working_mb = [math]::Round($p.WorkingSet64 / 1MB, 1); private_mb = [math]::Round($p.PrivateMemorySize64 / 1MB, 1) }
}

$total = ($rows | Measure-Object -Property working_mb -Sum).Sum
$result = [pscustomobject]@{
    label        = $Label
    has_window   = ($proc.MainWindowHandle -ne 0)
    processes    = $rows.Count
    total_mb     = [math]::Round($total, 1)
    breakdown    = ($rows | Sort-Object working_mb -Descending | ForEach-Object { "$($_.name):$($_.working_mb)" }) -join ' '
    measured_after_s = $Seconds
}

# Tutup semua proses milik aplikasi (termasuk anak WebView2), jangan sentuh milik proses lain.
foreach ($id in $ids) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
foreach ($p in $wv2Now) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }

$result | ConvertTo-Json -Compress | Tee-Object -FilePath (Join-Path $OutDir "ram-$Label.json")
