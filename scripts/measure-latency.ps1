# Query latency against an index that already exists: the fused total and each
# arm separately, at p50 and p95.
#
# The milestone gate (scripts/measure-gate.ps1) measures the same queries during
# its run, but the per-arm timings are what say *which* arm costs the query, and
# re-reading them should not cost another 11-minute drain. This script runs the
# identical query set against a finished index with the identical parse:
#
#   scripts/measure-latency.ps1 -Data $env:TEMP\tessera-gate\index3
#
# The number it reports ("searched in N ms") is measured after the embedders
# are built, so it is the search, not the model loading a fresh process pays.
param(
    [Parameter(Mandatory = $true)][string]$Data,
    [string]$ModelsDir = "",
    [string]$Exe = "",
    [int]$Repetitions = 8,
    [string]$OutJson = ""
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $Data)) { throw "no index at $Data - run scripts/measure-gate.ps1 first, or point -Data at one" }
if (-not $ModelsDir) { $ModelsDir = Join-Path $env:LOCALAPPDATA "Tessera\models" }
if (-not $Exe) { $Exe = Join-Path (Split-Path -Parent $PSScriptRoot) "target\release\tessera.exe" }

$root = Split-Path -Parent $Data

function Percentile {
    param([double[]]$Values, [double]$P)
    if (-not $Values -or $Values.Count -eq 0) { return $null }
    $sorted = @($Values | Sort-Object)
    $rank = [math]::Ceiling($P / 100.0 * $sorted.Count) - 1
    if ($rank -lt 0) { $rank = 0 }
    return [math]::Round($sorted[$rank], 1)
}

function Invoke-Search {
    param([string[]]$Arguments)
    $outFile = Join-Path $root ("latency-" + [guid]::NewGuid().ToString("N").Substring(0, 8) + ".txt")
    $errFile = "$outFile.err"
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $Exe `
        -ArgumentList (@("--data-dir", "`"$Data`"", "--models-dir", "`"$ModelsDir`"", "search") + $Arguments) `
        -RedirectStandardOutput $outFile -RedirectStandardError $errFile -PassThru -NoNewWindow -Wait
    $stopwatch.Stop()
    $err = if (Test-Path $errFile) { Get-Content $errFile -Raw } else { "" }
    $engineMs = $null; $keywordMs = $null; $imageMs = $null; $meaningMs = $null
    if ($err -match 'searched in (\d+) ms') { $engineMs = [int]$Matches[1] }
    if ($err -match 'keyword (\d+) ms') { $keywordMs = [int]$Matches[1] }
    if ($err -match 'image (\d+) ms') { $imageMs = [int]$Matches[1] }
    if ($err -match 'meaning (\d+) ms') { $meaningMs = [int]$Matches[1] }
    Remove-Item $outFile, $errFile -ErrorAction SilentlyContinue
    [pscustomobject]@{
        ExitCode = $process.ExitCode
        EngineMs = $engineMs
        KeywordMs = $keywordMs
        ImageMs = $imageMs
        MeaningMs = $meaningMs
        ProcessMs = [math]::Round($stopwatch.Elapsed.TotalMilliseconds, 0)
    }
}

# The gate's query set, unchanged: one fused phrase, two single-phrase lexical
# looks, one ordinary sentence, and one image description.
$queries = @(
    @("northwind ledger reconciliation"),
    @("Zephyrine codicil"),
    @("invoice supplier portal"),
    @("quarterly operations report"),
    @("--describe", "`"a comic book superhero in a red suit`"")
)

foreach ($query in $queries) { $null = Invoke-Search -Arguments $query }

$engine = @(); $process = @(); $keyword = @(); $image = @(); $meaning = @()
$runs = @()
foreach ($pass in 1..$Repetitions) {
    foreach ($query in $queries) {
        $run = Invoke-Search -Arguments $query
        if ($null -ne $run.EngineMs) { $engine += [double]$run.EngineMs }
        if ($null -ne $run.KeywordMs) { $keyword += [double]$run.KeywordMs }
        if ($null -ne $run.ImageMs) { $image += [double]$run.ImageMs }
        if ($null -ne $run.MeaningMs) { $meaning += [double]$run.MeaningMs }
        $process += [double]$run.ProcessMs
        $runs += [pscustomobject]@{
            Query = ($query -join " ")
            EngineMs = $run.EngineMs
            KeywordMs = $run.KeywordMs
            ImageMs = $run.ImageMs
            MeaningMs = $run.MeaningMs
            ProcessMs = $run.ProcessMs
        }
    }
}

$result = [pscustomobject]@{
    Data = $Data
    Repetitions = $Repetitions
    Queries = ($queries | ForEach-Object { $_ -join " " })
    FusedP50Ms = Percentile $engine 50
    FusedP95Ms = Percentile $engine 95
    KeywordP50Ms = Percentile $keyword 50
    KeywordP95Ms = Percentile $keyword 95
    ImageP50Ms = Percentile $image 50
    ImageP95Ms = Percentile $image 95
    MeaningP50Ms = Percentile $meaning 50
    MeaningP95Ms = Percentile $meaning 95
    Samples = $engine.Count
    ProcessP50Ms = Percentile $process 50
    ProcessP95Ms = Percentile $process 95
    Runs = $runs
}

if ($OutJson) { $result | ConvertTo-Json -Depth 6 | Set-Content -Path $OutJson -Encoding UTF8 }
$result | Select-Object Data, Repetitions, Samples, FusedP50Ms, FusedP95Ms, KeywordP50Ms, KeywordP95Ms, ImageP50Ms, ImageP95Ms, MeaningP50Ms, MeaningP95Ms, ProcessP50Ms, ProcessP95Ms | Format-List | Out-String
