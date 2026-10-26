# Measure `tessera watch` on this machine: drain throughput, save -> searchable
# latency, idle RAM after the models are released, and CPU while held.
#
# Everything is measured from outside the process: the release binary, its
# stdout, and the Windows process counters. Nothing here is estimated. This is
# the harness behind the "Watch mode — measured" table in docs/performance.md;
# run it from the repository root with a release build in place:
#
#   cargo build --release
#   scripts/measure-watch.ps1
#
# It writes only under a fresh temp directory and prints the root so the run can
# be inspected or deleted.
param(
    [int]$Files = 400,
    [string]$ModelsDir = "",
    [string]$Exe = ""
)

$ErrorActionPreference = 'Continue'

if (-not $ModelsDir) {
    $ModelsDir = Join-Path $env:LOCALAPPDATA "Tessera\models"
}
if (-not $Exe) {
    $Exe = Join-Path (Split-Path -Parent $PSScriptRoot) "target\release\tessera.exe"
}

function Now { [Diagnostics.Stopwatch]::GetTimestamp() / [Diagnostics.Stopwatch]::Frequency }
function Wait-Line {
    param([string]$Path, [string]$Needle, [double]$Timeout = 180, [int]$AtLeast = 1)
    $deadline = (Now) + $Timeout
    while ((Now) -lt $deadline) {
        if (Test-Path $Path) {
            $hit = @(Select-String -Path $Path -Pattern ([regex]::Escape($Needle)) -ErrorAction SilentlyContinue)
            if ($hit.Count -ge $AtLeast) { return (Now) }
        }
        Start-Sleep -Milliseconds 10
    }
    throw "timed out waiting for '$Needle' in $Path"
}
function Line-Count {
    param([string]$Path, [string]$Needle)
    if (-not (Test-Path $Path)) { return 0 }
    @(Select-String -Path $Path -Pattern $Needle -ErrorAction SilentlyContinue).Count
}

$root = Join-Path $env:TEMP ("tessera-watch-measure-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
$corpus = Join-Path $root "corpus"
$data = Join-Path $root "index"
$pausedData = Join-Path $root "index-paused"
New-Item -ItemType Directory -Path $corpus, $data, $pausedData -Force | Out-Null

for ($i = 0; $i -lt $Files; $i++) {
    $text = "document number $i about the invoice from the airline, reference INV-$i"
    [IO.File]::WriteAllText((Join-Path $corpus ("doc-{0:000}.txt" -f $i)), $text)
}

$stdout = Join-Path $root "stdout.txt"
$stderr = Join-Path $root "stderr.txt"
$pausedStdout = Join-Path $root "paused-stdout.txt"
$pausedStderr = Join-Path $root "paused-stderr.txt"

$t0 = Now
$p = Start-Process -FilePath $Exe `
    -ArgumentList @("--data-dir", "`"$data`"", "--models-dir", "`"$ModelsDir`"", "watch", "`"$corpus`"", "--schedule", "always") `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -WindowStyle Hidden
$tBanner = Wait-Line $stdout "file(s) need indexing"
$tCaught = Wait-Line $stdout "caught up"
$drain = $tCaught - $tBanner
$indexed = Line-Count $stdout "indexed "
$caughtLine = ((Get-Content $stdout | Select-String -Pattern "caught up").Line)

Start-Sleep -Seconds 2
$proc = Get-Process -Id $p.Id -ErrorAction Stop
$idleRamMb = [math]::Round($proc.WorkingSet64 / 1MB, 1)
$idlePrivateMb = [math]::Round($proc.PrivateMemorySize64 / 1MB, 1)
$cpuBefore = $proc.TotalProcessorTime.TotalSeconds
Start-Sleep -Seconds 5
$proc.Refresh()
$idleCpu = [math]::Round($proc.TotalProcessorTime.TotalSeconds - $cpuBefore, 3)

# Save a file the way an editor does, wait for its row, then confirm a real
# query finds it: the acceptance criterion is the search, not the log line.
$fresh = Join-Path $corpus "fresh.txt"
$tSave = Now
[IO.File]::WriteAllText($fresh, "the quarterly zebra unicorn invoice, saved while the watcher ran")
$tFreshLine = Wait-Line $stdout "fresh.txt" 60
$latencyToRow = $tFreshLine - $tSave

$found = $false
$tSearch = Now
while (-not $found -and ((Now) - $tSave) -lt 30) {
    $out = (& $Exe --data-dir $data --models-dir $ModelsDir search "zebra unicorn") 2>&1 | Out-String
    if ($out -match "fresh\.txt") { $found = $true }
}
$tSearchable = Now
$searchLatency = $tSearchable - $tSave
$searchWall = $tSearchable - $tSearch
# Wait for the *second* "caught up": the first one is the initial drain, already
# in the file, so matching it would return at once and time the save, not a
# drain. A drain that has not happened is no drain at all.
$caughtBefore = Line-Count $stdout "caught up"
$secondCaught = Wait-Line $stdout "caught up" 60 -AtLeast ($caughtBefore + 1)

Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue

# The held-queue case: a backlog the schedule refuses to dispatch must not burn
# a core. Started paused, so every file stays queued for the whole measurement.
$pp = Start-Process -FilePath $Exe `
    -ArgumentList @("--data-dir", "`"$pausedData`"", "--models-dir", "`"$ModelsDir`"", "watch", "`"$corpus`"", "--schedule", "paused") `
    -RedirectStandardOutput $pausedStdout -RedirectStandardError $pausedStderr -PassThru -WindowStyle Hidden
$null = Wait-Line $pausedStdout "file(s) need indexing"
Start-Sleep -Milliseconds 500
$held = Get-Process -Id $pp.Id -ErrorAction Stop
$heldBefore = $held.TotalProcessorTime.TotalSeconds
Start-Sleep -Seconds 5
$held.Refresh()
$heldCpu = [math]::Round($held.TotalProcessorTime.TotalSeconds - $heldBefore, 3)
$heldRamMb = [math]::Round($held.WorkingSet64 / 1MB, 1)
$pausedLine = ((Get-Content $pausedStdout | Select-String -Pattern "paused").Line)
Stop-Process -Id $pp.Id -Force -ErrorAction SilentlyContinue

[pscustomobject]@{
    Files              = $Files
    DrainSeconds       = [math]::Round($drain, 2)
    FilesPerSecond     = [math]::Round($indexed / [math]::Max($drain, 0.001), 0)
    IndexedLines       = $indexed
    CaughtLine         = $caughtLine
    IdleRamMb          = $idleRamMb
    IdlePrivateMb      = $idlePrivateMb
    IdleCpuFiveSeconds = $idleCpu
    SaveToRowMs        = [math]::Round($latencyToRow * 1000, 0)
    SaveToSearchableMs = [math]::Round($searchLatency * 1000, 0)
    SearchProcessMs    = [math]::Round($searchWall * 1000, 0)
    FoundBySearch      = $found
    SecondDrainSeconds = [math]::Round($secondCaught - $tSave, 2)
    PausedLine         = $pausedLine
    HeldBacklogCpuFive = $heldCpu
    HeldRamMb          = $heldRamMb
    Root               = $root
} | Format-List
