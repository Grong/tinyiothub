param(
    [string]$Tag = "latest",
    [switch]$NoBuildCache
)

$ErrorActionPreference = "Stop"

Write-Host "=== Building TinyIoTHub Docker Image ===" -ForegroundColor Cyan

$buildArgs = @("build", "-t", "tinyiothub:$Tag", "-f", "Dockerfile", ".")

if ($NoBuildCache) {
    $buildArgs += "--no-cache"
    Write-Host "Building without cache..." -ForegroundColor Yellow
}

Write-Host "Running: docker $($buildArgs -join ' ')" -ForegroundColor Gray

& docker @buildArgs

if ($LASTEXITCODE -ne 0) {
    Write-Host "Docker build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "`n=== Build Complete ===" -ForegroundColor Green
Write-Host "Image: tinyiothub:$Tag" -ForegroundColor Green
Write-Host "`nRun with: docker-compose up -d" -ForegroundColor Cyan
