param(
    [string]$Tag = "latest",
    [string]$Platform = "linux/amd64",
    [switch]$NoCache
)

$ErrorActionPreference = "Stop"

Write-Host "=== Fast Docker Build with Cache ===" -ForegroundColor Cyan
Write-Host "Platform: $Platform" -ForegroundColor Gray
Write-Host "Tag: tinyiothub:$Tag" -ForegroundColor Gray

# 构建参数
$buildArgs = @(
    "buildx", "build",
    "--platform", $Platform,
    "-t", "tinyiothub:$Tag",
    "-f", "Dockerfile"
)

if ($NoCache) {
    $buildArgs += "--no-cache"
    Write-Host "Building without cache..." -ForegroundColor Yellow
} else {
    Write-Host "Using build cache for faster builds..." -ForegroundColor Green
}

# 加载到本地 Docker
$buildArgs += "--load"
$buildArgs += "."

Write-Host "`nBuilding..." -ForegroundColor Yellow
$startTime = Get-Date

& docker @buildArgs

if ($LASTEXITCODE -ne 0) {
    Write-Host "`nBuild failed!" -ForegroundColor Red
    exit 1
}

$duration = (Get-Date) - $startTime
Write-Host "`n=== Build Complete ===" -ForegroundColor Green
Write-Host "Time: $($duration.TotalSeconds.ToString('0.0'))s" -ForegroundColor Green
Write-Host "Image: tinyiothub:$Tag" -ForegroundColor Green
Write-Host "`nTip: Subsequent builds will be faster due to layer caching" -ForegroundColor Cyan
