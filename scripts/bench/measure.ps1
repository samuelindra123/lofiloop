<#
.SYNOPSIS
  Jalankan satu proses, ukur waktu dinding, waktu CPU, dan RAM puncak (peak working set).

.EXAMPLE
  powershell -NoProfile -File scripts\spike\measure.ps1 -Exe ffmpeg -Args '-i in.mp4 -c copy out.mp4' -Label video-1h
#>
param(
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][string]$ArgLine,
    [string]$Label = 'run',
    [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
    [string]$LogDir
)

if (-not $LogDir) { $LogDir = Join-Path (Split-Path -Parent (Split-Path -Parent $PSCommandPath)) '..\work\logs' }

$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$stderr = Join-Path $LogDir "$Label.stderr.txt"
$stdout = Join-Path $LogDir "$Label.stdout.txt"

$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Exe -ArgumentList $ArgLine -NoNewWindow -PassThru `
    -RedirectStandardError $stderr -RedirectStandardOutput $stdout
$null = $p.Handle   # simpan handle supaya ExitCode dan TotalProcessorTime terbaca setelah proses selesai
try { $p.PriorityClass = $Priority } catch { }

# Pegang handle, ambil puncak selama berjalan (PeakWorkingSet64 tetap bisa dibaca setelah exit).
$peak = 0
while (-not $p.HasExited) {
    try { $p.Refresh(); if ($p.PeakWorkingSet64 -gt $peak) { $peak = $p.PeakWorkingSet64 } } catch { }
    Start-Sleep -Milliseconds 500
}
$p.WaitForExit()
$sw.Stop()
try { $p.Refresh(); if ($p.PeakWorkingSet64 -gt $peak) { $peak = $p.PeakWorkingSet64 } } catch { }
$cpu = $null
try { $cpu = $p.TotalProcessorTime.TotalSeconds } catch { }

$result = [pscustomobject]@{
    label      = $Label
    exit       = $p.ExitCode
    wall_s     = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    cpu_s      = if ($cpu -ne $null) { [math]::Round($cpu, 1) } else { $null }
    peak_mb    = [math]::Round($peak / 1MB, 1)
    priority   = $Priority
}
$result | ConvertTo-Json -Compress | Tee-Object -FilePath (Join-Path $LogDir "$Label.result.json")
exit $p.ExitCode
