# Ticket-12 milestone gate: index a ~5k mixed corpus with `tessera watch`,
# run the known-answer queries, and record throughput, latency, memory and the
# fallback OCR figure. Every number comes from the release binary and Windows
# process counters; nothing is estimated.
#
#   python scripts/make-gate-corpus.py --out $env:TEMP\tessera-gate ...
#   scripts/measure-gate.ps1 -Corpus $env:TEMP\tessera-gate\corpus `
#       -Data $env:TEMP\tessera-gate\index -OutJson $env:TEMP\tessera-gate\result.json
#
# The script starts from an empty index directory, writes only under the temp
# paths it is given, and kills the watcher when the first drain finishes.

param(
    [Parameter(Mandatory = $true)][string]$Corpus,
    [Parameter(Mandatory = $true)][string]$Data,
    [string]$ModelsDir = "",
    [string]$Exe = "",
    [string]$OutJson = "",
    [int]$Repetitions = 8,
    [int]$DrainTimeoutSeconds = 21600
)

$ErrorActionPreference = 'Stop'

if (-not $ModelsDir) { $ModelsDir = Join-Path $env:LOCALAPPDATA "Tessera\models" }
if (-not $Exe) { $Exe = Join-Path (Split-Path -Parent $PSScriptRoot) "target\release\tessera.exe" }
if (-not $OutJson) { $OutJson = Join-Path (Split-Path -Parent $Data) "result.json" }
if (-not (Test-Path $Exe)) { throw "release binary not found at $Exe" }
if (-not (Test-Path (Join-Path $Corpus "images"))) { throw "corpus not found at $Corpus" }
if (Test-Path $Data) { Remove-Item -Recurse -Force $Data }
New-Item -ItemType Directory -Path $Data -Force | Out-Null
$root = Split-Path -Parent $Data
$stdout = Join-Path $root "watch-stdout.txt"
$stderr = Join-Path $root "watch-stderr.txt"

function Now { [Diagnostics.Stopwatch]::GetTimestamp() / [Diagnostics.Stopwatch]::Frequency }
function Wait-Line {
    param([string]$Path, [string]$Needle, [double]$Timeout = 180, [int]$AtLeast = 1)
    $deadline = (Now) + $Timeout
    while ((Now) -lt $deadline) {
        if (Test-Path $Path) {
            $hit = @(Select-String -Path $Path -Pattern ([regex]::Escape($Needle)) -ErrorAction SilentlyContinue)
            if ($hit.Count -ge $AtLeast) { return (Now) }
        }
        Start-Sleep -Milliseconds 200
    }
    throw "timed out waiting for '$Needle' in $Path"
}
function Percentile {
    param([double[]]$Values, [double]$P)
    if (-not $Values -or $Values.Count -eq 0) { return $null }
    $sorted = @($Values | Sort-Object)
    $rank = [math]::Ceiling($P / 100.0 * $sorted.Count) - 1
    if ($rank -lt 0) { $rank = 0 }
    return [math]::Round($sorted[$rank], 1)
}

$tStart = Now
$p = Start-Process -FilePath $Exe `
    -ArgumentList @("--data-dir", "`"$Data`"", "--models-dir", "`"$ModelsDir`"",
                    "watch", "--schedule", "always", "`"$Corpus`"") `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -WindowStyle Hidden

$tBanner = Wait-Line $stdout "file(s) need indexing" 300
$banner = ((Get-Content $stdout |
    Select-String -Pattern ([regex]::Escape("file(s) need indexing")) |
    Select-Object -First 1).Line)

# Sample RSS/CPU while the drain runs, so the peak is a measurement rather than
# a number read after the models have been released.
$peakWs = 0
$peakPrivate = 0
$samples = 0
$tCaught = $null
$deadline = (Now) + $DrainTimeoutSeconds
while ((Now) -lt $deadline) {
    try {
        $proc = Get-Process -Id $p.Id -ErrorAction Stop
        $proc.Refresh()
        $peakWs = [math]::Max($peakWs, $proc.WorkingSet64)
        $peakPrivate = [math]::Max($peakPrivate, $proc.PrivateMemorySize64)
        $samples++
    } catch { }
    $line = $null
    if (Test-Path $stdout) {
        $line = (Get-Content $stdout | Select-String -Pattern "caught up").Line
    }
    if ($line) { $tCaught = Now; break }
    Start-Sleep -Seconds 2
}
if (-not $tCaught) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue; throw "the drain did not finish in $DrainTimeoutSeconds s" }

$caughtLine = ((Get-Content $stdout | Select-String -Pattern "caught up").Line)
$tCaught = Now
$drainSeconds = [math]::Round($tCaught - $tBanner, 2)
$wallSeconds = [math]::Round($tCaught - $tStart, 2)
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# Parse "caught up: N indexed, M moved, K skipped, F failed, A awaiting OCR, E chunk(s) embedded"
$indexed = 0; $moved = 0; $skipped = 0; $failed = 0; $awaiting = 0; $embedded = 0
if ($caughtLine -match 'caught up: (\d+) indexed, (\d+) moved, (\d+) skipped, (\d+) failed, (\d+) awaiting OCR, (\d+) chunk') {
    $indexed = [int]$Matches[1]; $moved = [int]$Matches[2]; $skipped = [int]$Matches[3]
    $failed = [int]$Matches[4]; $awaiting = [int]$Matches[5]; $embedded = [int]$Matches[6]
}
$processed = $indexed + $moved + $skipped + $failed + $awaiting
$corpusFiles = (Get-ChildItem $Corpus -Recurse -File).Count
# Throughput counts corpus files, not outcome events: a file retried after a
# transient failure must not be counted twice and make the number look better.
$throughput = if ($drainSeconds -gt 0) { [math]::Round($corpusFiles / $drainSeconds, 1) } else { $null }
$eventRate = if ($drainSeconds -gt 0) { [math]::Round($processed / $drainSeconds, 1) } else { $null }
$indexedPerSecond = if ($drainSeconds -gt 0) { [math]::Round($indexed / $drainSeconds, 1) } else { $null }

$indexBytes = (Get-ChildItem $Data -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
$diskPer1k = if ($corpusFiles -gt 0) { [math]::Round($indexBytes / ($corpusFiles / 1000.0) / 1MB, 1) } else { $null }

# ---- known-answer queries -------------------------------------------------
function Invoke-Search {
    param([string[]]$Arguments)
    $outFile = Join-Path $root ("search-" + [guid]::NewGuid().ToString("N").Substring(0, 8) + ".txt")
    $errFile = "$outFile.err"
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $Exe `
        -ArgumentList (@("--data-dir", "`"$Data`"", "--models-dir", "`"$ModelsDir`"", "search") + $Arguments) `
        -RedirectStandardOutput $outFile -RedirectStandardError $errFile -PassThru -NoNewWindow -Wait
    $stopwatch.Stop()
    $out = if (Test-Path $outFile) { Get-Content $outFile -Raw } else { "" }
    $err = if (Test-Path $errFile) { Get-Content $errFile -Raw } else { "" }
    $engineMs = $null
    if ($err -match 'searched in (\d+) ms') { $engineMs = [int]$Matches[1] }
    $keywordMs = $null; $imageMs = $null; $meaningMs = $null
    if ($err -match 'keyword (\d+) ms') { $keywordMs = [int]$Matches[1] }
    if ($err -match 'image (\d+) ms') { $imageMs = [int]$Matches[1] }
    if ($err -match 'meaning (\d+) ms') { $meaningMs = [int]$Matches[1] }
    Remove-Item $outFile, $errFile -ErrorAction SilentlyContinue
    [pscustomobject]@{
        ExitCode = $process.ExitCode
        Output = $out
        EngineMs = $engineMs
        ProcessMs = [math]::Round($stopwatch.Elapsed.TotalMilliseconds, 0)
        KeywordMs = $keywordMs
        ImageMs = $imageMs
        MeaningMs = $meaningMs
    }
}

$known = @(
    @{ Name = "ocr-screenshot"; Args = @("NORTHWIND LEDGER RECONCILIATION 5521"); Expect = "screenshot-0017.png" },
    @{ Name = "image-description"; Args = @("--describe", "`"a comic book superhero in a red suit`""); Expect = "pictures-wallpapers" },
    @{ Name = "pdf-phrase"; Args = @("Zephyrine codicil 7Q4X"); Expect = "report-0007.pdf" },
    @{ Name = "docx-phrase"; Args = @("Kestrel amendment 8M2V"); Expect = "memo-0042.docx" },
    @{ Name = "fused-mixed"; Args = @("northwind ledger reconciliation"); Expect = "screenshot-0017.png" }
)

$knownResults = @()
foreach ($case in $known) {
    $run = Invoke-Search -Arguments $case.Args
    $passed = $run.Output -match [regex]::Escape($case.Expect)
    $knownResults += [pscustomobject]@{
        Name = $case.Name
        Query = ($case.Args -join " ")
        Expected = $case.Expect
        Passed = $passed
        EngineMs = $run.EngineMs
        Top30 = (($run.Output -split "`n" | Select-Object -First 12) -join "`n")
    }
}

# ---- latency: 5 queries, warm-up once, then Repetitions passes ------------
$latencyQueries = @(
    @("northwind ledger reconciliation"),
    @("Zephyrine codicil"),
    @("invoice supplier portal"),
    @("quarterly operations report"),
    @("--describe", "`"a comic book superhero in a red suit`"")
)
foreach ($query in $latencyQueries) { $null = Invoke-Search -Arguments $query }
$engineSamples = @()
$processSamples = @()
$keywordSamples = @()
$imageSamples = @()
$meaningSamples = @()
foreach ($pass in 1..$Repetitions) {
    foreach ($query in $latencyQueries) {
        $run = Invoke-Search -Arguments $query
        if ($null -ne $run.EngineMs) { $engineSamples += [double]$run.EngineMs }
        # The CLI prints "searched in N ms (keyword a ms, meaning b ms, image c
        # ms)"; keeping the arms apart is what says which one costs the query.
        if ($null -ne $run.KeywordMs) { $keywordSamples += [double]$run.KeywordMs }
        if ($null -ne $run.ImageMs) { $imageSamples += [double]$run.ImageMs }
        if ($null -ne $run.MeaningMs) { $meaningSamples += [double]$run.MeaningMs }
        $processSamples += [double]$run.ProcessMs
    }
}

# ---- save -> searchable ---------------------------------------------------
# Two probes, because there are two numbers a person experiences: the first
# save after an idle spell pays for loading the models, and every save after it
# does not. Both are measured from the write to the line the watcher prints
# after the row is committed, and the first is confirmed through `search`.
$probeOne = Join-Path $Corpus "save-probe-1.txt"
$probeTwo = Join-Path $Corpus "save-probe-2.txt"
Remove-Item $probeOne, $probeTwo -ErrorAction SilentlyContinue
$stdout2 = Join-Path $root "savelatency-stdout.txt"
$stderr2 = Join-Path $root "savelatency-stderr.txt"
$saveWatch = Start-Process -FilePath $Exe `
    -ArgumentList @("--data-dir", "`"$Data`"", "--models-dir", "`"$ModelsDir`"",
                    "watch", "--schedule", "always", "`"$Corpus`"") `
    -RedirectStandardOutput $stdout2 -RedirectStandardError $stderr2 -PassThru -WindowStyle Hidden
$null = Wait-Line $stdout2 "file(s) need indexing" 600

function Measure-Save {
    param([string]$Path, [string]$Phrase)
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    Set-Content -Path $Path -Value "Quillon receipt $Phrase posted to the ledger. This line exists only to measure save latency." -Encoding UTF8
    $name = Split-Path -Leaf $Path
    $deadline = (Now) + 180
    while ((Now) -lt $deadline) {
        if ((Test-Path $stdout2) -and (Select-String -Path $stdout2 -Pattern ("^indexed\s+.*" + [regex]::Escape($name)) -Quiet)) {
            return [math]::Round($stopwatch.Elapsed.TotalMilliseconds, 0)
        }
        Start-Sleep -Milliseconds 20
    }
    return $null
}

$firstSaveMs = Measure-Save -Path $probeOne -Phrase "4K9T"
$firstProbeSearch = Invoke-Search -Arguments @("Quillon receipt 4K9T")
$warmSaveMs = Measure-Save -Path $probeTwo -Phrase "7M2Q"
Stop-Process -Id $saveWatch.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300
Remove-Item $probeOne, $probeTwo -ErrorAction SilentlyContinue

# ---- fallback OCR on real screenshots ------------------------------------
# `tessera ocr` prints the timing on stdout, as the last line:
#   windows: 5 line(s), 41 character(s), 59.4 ms
$ocrImages = @(Get-ChildItem (Join-Path $Corpus "images\real\pictures-screenshots") -File | Select-Object -First 5)
$onnxMs = @()
$windowsMs = @()
foreach ($image in $ocrImages) {
    foreach ($expected in @("onnx", "windows")) {
        $outFile = Join-Path $root ("ocr-" + [guid]::NewGuid().ToString("N").Substring(0, 8) + ".txt")
        $errFile = "$outFile.err"
        $arguments = @("--data-dir", "`"$Data`"", "--models-dir", "`"$ModelsDir`"", "ocr")
        if ($expected -eq "onnx") { $arguments += @("--engine", "onnx") }
        $arguments += "`"$($image.FullName)`""
        $null = Start-Process -FilePath $Exe -ArgumentList $arguments `
            -RedirectStandardOutput $outFile -RedirectStandardError $errFile -PassThru -NoNewWindow -Wait
        $out = if (Test-Path $outFile) { Get-Content $outFile -Raw } else { "" }
        if ($out -match "(?m)^$expected`: \d+ line\(s\), \d+ character\(s\), ([\d.]+) ms") {
            if ($expected -eq "onnx") { $onnxMs += [double]$Matches[1] } else { $windowsMs += [double]$Matches[1] }
        }
        Remove-Item $outFile, $errFile -ErrorAction SilentlyContinue
    }
}

$result = [pscustomobject]@{
    Corpus = $Corpus
    CorpusFiles = $corpusFiles
    DataDir = $Data
    Banner = $banner
    CaughtUp = $caughtLine
    WallSeconds = $wallSeconds
    DrainSeconds = $drainSeconds
    Processed = $processed
    Indexed = $indexed
    Moved = $moved
    Skipped = $skipped
    Failed = $failed
    AwaitingOcr = $awaiting
    ChunksEmbedded = $embedded
    FilesPerSecond = $throughput
    OutcomeEventsPerSecond = $eventRate
    IndexedPerSecond = $indexedPerSecond
    PeakWorkingSetMb = [math]::Round($peakWs / 1MB, 1)
    PeakPrivateMb = [math]::Round($peakPrivate / 1MB, 1)
    Samples = $samples
    IndexBytes = $indexBytes
    IndexMb = [math]::Round($indexBytes / 1MB, 1)
    DiskMbPer1k = $diskPer1k
    FusedP50Ms = Percentile $engineSamples 50
    FusedP95Ms = Percentile $engineSamples 95
    FusedSamples = $engineSamples.Count
    KeywordP50Ms = Percentile $keywordSamples 50
    KeywordP95Ms = Percentile $keywordSamples 95
    ImageP50Ms = Percentile $imageSamples 50
    ImageP95Ms = Percentile $imageSamples 95
    MeaningP50Ms = Percentile $meaningSamples 50
    MeaningP95Ms = Percentile $meaningSamples 95
    ArmSamples = $keywordSamples.Count
    ProcessP50Ms = Percentile $processSamples 50
    ProcessP95Ms = Percentile $processSamples 95
    FirstSaveMs = $firstSaveMs
    FirstSaveSearchMs = $firstProbeSearch.ProcessMs
    FirstSaveFound = ($firstProbeSearch.Output -match [regex]::Escape("Quillon receipt 4K9T"))
    WarmSaveMs = $warmSaveMs
    OnnxOcrP50Ms = Percentile $onnxMs 50
    OnnxOcrSamples = $onnxMs
    WindowsOcrP50Ms = Percentile $windowsMs 50
    WindowsOcrSamples = $windowsMs
    KnownAnswers = $knownResults
}

$result | ConvertTo-Json -Depth 6 | Set-Content -Path $OutJson -Encoding UTF8
$result | ConvertTo-Json -Depth 6
