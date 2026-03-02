param(
    [Parameter(Mandatory = $true)]
    [string]$CompetitorName,
    [Parameter(Mandatory = $true)]
    [string]$CompetitorUrl,
    [string]$FerroqUrl = "http://127.0.0.1:8080/onebot/v11/api/get_login_info",
    [string]$FerroqToken = "",
    [string]$CompetitorToken = "",
    [int]$Requests = 2000,
    [int]$Concurrency = 100,
    [int]$TimeoutSec = 10,
    [string]$OutputPath = "COMPARE_BENCHMARK.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw "PowerShell 7+ is required (ForEach-Object -Parallel)."
}
if ($Requests -le 0 -or $Concurrency -le 0) {
    throw "Requests and Concurrency must be > 0."
}

function Get-Percentile {
    param(
        [double[]]$Values,
        [double]$P
    )
    if ($Values.Count -eq 0) {
        return [double]::NaN
    }
    $sorted = $Values | Sort-Object
    $idx = [Math]::Floor(($sorted.Count - 1) * $P)
    return [double]$sorted[[int]$idx]
}

function Invoke-GatewayBenchmark {
    param(
        [string]$Name,
        [string]$Url,
        [string]$Token,
        [int]$Req,
        [int]$Par,
        [int]$Timeout
    )

    $latencies = [System.Collections.Concurrent.ConcurrentBag[double]]::new()
    $successLatencies = [System.Collections.Concurrent.ConcurrentBag[double]]::new()
    $codes = [System.Collections.Concurrent.ConcurrentBag[int]]::new()
    $errors = [System.Collections.Concurrent.ConcurrentBag[string]]::new()
    $headers = @{}
    if (-not [string]::IsNullOrWhiteSpace($Token)) {
        $headers["Authorization"] = "Bearer $Token"
    }
    $body = "{}"

    $swAll = [System.Diagnostics.Stopwatch]::StartNew()

    1..$Req | ForEach-Object -Parallel {
        $latRef = $using:latencies
        $okLatRef = $using:successLatencies
        $codeRef = $using:codes
        $errRef = $using:errors
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $resp = Invoke-WebRequest `
                -Method Post `
                -Uri $using:Url `
                -Headers $using:headers `
                -Body $using:body `
                -ContentType "application/json" `
                -TimeoutSec $using:Timeout `
                -SkipHttpErrorCheck

            $sw.Stop()
            $latMs = $sw.Elapsed.TotalMilliseconds
            $code = [int]$resp.StatusCode
            $latRef.Add($latMs)
            $codeRef.Add($code)
            if ($code -ge 200 -and $code -lt 300) {
                $okLatRef.Add($latMs)
            }
        } catch {
            $sw.Stop()
            $latRef.Add($sw.Elapsed.TotalMilliseconds)
            $errRef.Add($_.Exception.Message)
        }
    } -ThrottleLimit $Par

    $swAll.Stop()

    $latArray = @($latencies.ToArray())
    $okLatArray = @($successLatencies.ToArray())
    $codeArray = @($codes.ToArray())
    $errArray = @($errors.ToArray())

    $okCount = ($codeArray | Where-Object { $_ -ge 200 -and $_ -lt 300 }).Count
    $elapsedSec = [Math]::Max($swAll.Elapsed.TotalSeconds, 0.001)
    $rps = $okCount / $elapsedSec

    [pscustomobject]@{
        Name        = $Name
        Requests    = $Req
        Success     = $okCount
        Errors      = $Req - $okCount
        ErrorRate   = (($Req - $okCount) / [double]$Req) * 100.0
        ElapsedSec  = [Math]::Round($elapsedSec, 3)
        Rps         = [Math]::Round($rps, 2)
        P50Ms       = [Math]::Round((Get-Percentile -Values $okLatArray -P 0.50), 3)
        P95Ms       = [Math]::Round((Get-Percentile -Values $okLatArray -P 0.95), 3)
        P99Ms       = [Math]::Round((Get-Percentile -Values $okLatArray -P 0.99), 3)
        MaxMs       = [Math]::Round(($okLatArray | Measure-Object -Maximum).Maximum, 3)
        SampleCodes = ($codeArray | Group-Object | Sort-Object Count -Descending | Select-Object -First 3 | ForEach-Object { "$($_.Name)x$($_.Count)" }) -join ", "
        SampleError = if ($errArray.Count -gt 0) { $errArray[0] } else { "" }
    }
}

Write-Host "Running benchmark: ferroq..."
$ferroq = Invoke-GatewayBenchmark -Name "ferroq" -Url $FerroqUrl -Token $FerroqToken -Req $Requests -Par $Concurrency -Timeout $TimeoutSec
Write-Host "Running benchmark: $CompetitorName..."
$competitor = Invoke-GatewayBenchmark -Name $CompetitorName -Url $CompetitorUrl -Token $CompetitorToken -Req $Requests -Par $Concurrency -Timeout $TimeoutSec

$rows = @($ferroq, $competitor)

$winnerThroughput = if ($ferroq.Rps -ge $competitor.Rps) { "ferroq" } else { $CompetitorName }
$winnerLatency = if ($ferroq.P95Ms -le $competitor.P95Ms) { "ferroq" } else { $CompetitorName }

$md = @()
$md += "# Gateway Benchmark Comparison"
$md += ""
$md += "Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')"
$md += ""
$md += "Scenario:"
$md += "- Requests: $Requests"
$md += "- Concurrency: $Concurrency"
$md += "- TimeoutSec: $TimeoutSec"
$md += "- API: POST /onebot/v11/api/get_login_info (body: `{}`)"
$md += ""
$md += "| Gateway | Success | Errors | Error Rate | Throughput (req/s) | p50 (ms) | p95 (ms) | p99 (ms) | max (ms) | Status Codes |"
$md += "|---|---:|---:|---:|---:|---:|---:|---:|---:|---|"
foreach ($r in $rows) {
    $md += "| $($r.Name) | $($r.Success) | $($r.Errors) | $([Math]::Round($r.ErrorRate, 2))% | $($r.Rps) | $($r.P50Ms) | $($r.P95Ms) | $($r.P99Ms) | $($r.MaxMs) | $($r.SampleCodes) |"
}
$md += ""
$md += "Winners:"
$md += "- Throughput winner: **$winnerThroughput**"
$md += "- p95 latency winner: **$winnerLatency**"
if ($competitor.SampleError) {
    $md += ""
    $md += "Sample competitor error:"
    $md += "> $($competitor.SampleError)"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$outAbs = Join-Path $repoRoot $OutputPath
$md -join "`n" | Set-Content -Path $outAbs -Encoding UTF8
Write-Host "Wrote $outAbs"
