param(
    [string]$OutputPath = "COMPETITOR_SNAPSHOT.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$envPath = Join-Path $repoRoot ".env"

$token = $env:GITHUB_TOKEN
if ([string]::IsNullOrWhiteSpace($token)) {
    $token = $env:GH_TOKEN
}

if ([string]::IsNullOrWhiteSpace($token)) {
    if (-not (Test-Path $envPath)) {
        throw ".env not found at $envPath and no GITHUB_TOKEN/GH_TOKEN provided"
    }

    $tokenLine = Get-Content $envPath | Where-Object { $_ -match "^Personal_access_tokens=" } | Select-Object -First 1
    if (-not $tokenLine) {
        throw "Personal_access_tokens not found in .env"
    }

    $token = ($tokenLine -split "=", 2)[1].Trim().Trim('"')
}

if ([string]::IsNullOrWhiteSpace($token)) {
    throw "GitHub token is empty (.env Personal_access_tokens or env GITHUB_TOKEN)"
}

$headers = @{
    Authorization = "Bearer $token"
    "User-Agent"  = "ferroq-competitive-audit"
    Accept        = "application/vnd.github+json"
}

$repos = @(
    @{ Name = "ferroq";            FullName = "yanzhi0922/ferroq";            Type = "Gateway" },
    @{ Name = "NapCatQQ";          FullName = "NapNeko/NapCatQQ";             Type = "Protocol backend" },
    @{ Name = "go-cqhttp";         FullName = "Mrs4s/go-cqhttp";              Type = "Protocol implementation" },
    @{ Name = "Lagrange.Core";     FullName = "LagrangeDev/Lagrange.Core";    Type = "Protocol backend" },
    @{ Name = "Lagrange.onebot";   FullName = "LSTM-Kirigaya/Lagrange.onebot"; Type = "OneBot bridge" }
)

function Get-RepoInfo {
    param(
        [string]$FullName
    )
    $uri = "https://api.github.com/repos/$FullName"
    $r = Invoke-RestMethod -Method Get -Uri $uri -Headers $headers
    [pscustomobject]@{
        FullName   = $r.full_name
        HtmlUrl    = $r.html_url
        Stars      = [int]$r.stargazers_count
        Forks      = [int]$r.forks_count
        OpenIssues = [int]$r.open_issues_count
        Archived   = [bool]$r.archived
        UpdatedAt  = [datetime]$r.updated_at
        License    = if ($null -ne $r.license) { $r.license.spdx_id } else { "N/A" }
    }
}

$rows = @()
foreach ($repo in $repos) {
    $info = Get-RepoInfo -FullName $repo.FullName
    $rows += [pscustomobject]@{
        Name       = $repo.Name
        Type       = $repo.Type
        FullName   = $info.FullName
        Url        = $info.HtmlUrl
        Stars      = $info.Stars
        Forks      = $info.Forks
        OpenIssues = $info.OpenIssues
        Archived   = $info.Archived
        UpdatedAt  = $info.UpdatedAt.ToString("yyyy-MM-dd")
        License    = $info.License
    }
}

$ferroqStars = ($rows | Where-Object { $_.Name -eq "ferroq" }).Stars

$sb = [System.Text.StringBuilder]::new()
$null = $sb.AppendLine("# Competitor Snapshot")
$null = $sb.AppendLine()
$null = $sb.AppendLine("Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss zzz")")
$null = $sb.AppendLine()
$null = $sb.AppendLine("| Project | Type | Stars | Forks | Open Issues | Archived | Updated | License |")
$null = $sb.AppendLine("|---|---|---:|---:|---:|---|---|---|")
foreach ($row in $rows) {
    $nameCell = "[$($row.FullName)]($($row.Url))"
    $null = $sb.AppendLine("| $nameCell | $($row.Type) | $($row.Stars) | $($row.Forks) | $($row.OpenIssues) | $($row.Archived) | $($row.UpdatedAt) | $($row.License) |")
}

$null = $sb.AppendLine()
$null = $sb.AppendLine("## Star Gap vs ferroq")
$null = $sb.AppendLine()
$null = $sb.AppendLine("| Project | Star Gap |")
$null = $sb.AppendLine("|---|---:|")
foreach ($row in $rows | Where-Object { $_.Name -ne "ferroq" } | Sort-Object Stars -Descending) {
    $gap = $row.Stars - $ferroqStars
    $null = $sb.AppendLine("| $($row.FullName) | $gap |")
}

$outAbs = Join-Path $repoRoot $OutputPath
$sb.ToString() | Set-Content -Path $outAbs -Encoding UTF8
Write-Host "Wrote $outAbs"
