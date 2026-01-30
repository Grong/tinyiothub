#!/usr/bin/env pwsh
# TinyIoTHub - Docker Image Build and Export Script (Windows PowerShell)
# Build ARM64 Docker images and export as tar files

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# Configuration
$API_IMAGE_NAME = "tinyiothub-api"
$WEB_IMAGE_NAME = "tinyiothub-web"
$TAG = "arm64"
$PLATFORM = "linux/arm64"

Write-Host "========================================"
Write-Host "TinyIoTHub - Docker Build"
Write-Host "========================================"
Write-Host ""

# Check required tools
Write-Host "[1/5] Checking required tools..."

# Check Docker
if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: docker command not found" -ForegroundColor Red
    Write-Host "Please install Docker Desktop: https://www.docker.com/products/docker-desktop" -ForegroundColor Red
    exit 1
}

# Check if Docker is running
try {
    docker ps 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Docker not running"
    }
} catch {
    Write-Host "ERROR: Docker is not running" -ForegroundColor Red
    Write-Host "Please start Docker Desktop" -ForegroundColor Red
    exit 1
}

# Check pnpm
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: pnpm command not found" -ForegroundColor Red
    Write-Host "Please install pnpm: npm install -g pnpm" -ForegroundColor Red
    exit 1
}

Write-Host "OK: All required tools are ready" -ForegroundColor Green
Write-Host ""

# Build backend Docker image
Write-Host "[2/5] Building backend Docker image..."
Write-Host "Image: ${API_IMAGE_NAME}:${TAG}"
Write-Host "This may take several minutes, please wait..."
Write-Host ""

try {
    docker build --platform $PLATFORM -t "${API_IMAGE_NAME}:${TAG}" -f Dockerfile .
    if ($LASTEXITCODE -ne 0) {
        throw "Backend image build failed"
    }
    Write-Host ""
    Write-Host "OK: Backend image built successfully" -ForegroundColor Green
} catch {
    Write-Host "ERROR: Backend image build failed" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}
Write-Host ""

# Export backend image
Write-Host "[3/5] Exporting backend image..."
$API_TAR = "${API_IMAGE_NAME}-${TAG}.tar"

try {
    docker save "${API_IMAGE_NAME}:${TAG}" -o $API_TAR
    if ($LASTEXITCODE -ne 0) {
        throw "Backend image export failed"
    }
    $apiSize = [math]::Round((Get-Item $API_TAR).Length / 1MB, 2)
    Write-Host "OK: Backend image exported: $API_TAR ($apiSize MB)" -ForegroundColor Green
} catch {
    Write-Host "ERROR: Backend image export failed" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}
Write-Host ""

# Build frontend Docker image
Write-Host "[4/5] Building frontend Docker image..."
Write-Host "Image: ${WEB_IMAGE_NAME}:${TAG}"
Write-Host "This may take several minutes, please wait..."
Write-Host ""

Push-Location web
try {
    docker build --platform $PLATFORM -t "${WEB_IMAGE_NAME}:${TAG}" -f Dockerfile .
    if ($LASTEXITCODE -ne 0) {
        throw "Frontend image build failed"
    }
    Write-Host ""
    Write-Host "OK: Frontend image built successfully" -ForegroundColor Green
} catch {
    Write-Host "ERROR: Frontend image build failed" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    Pop-Location
    exit 1
} finally {
    Pop-Location
}
Write-Host ""

# Export frontend image
Write-Host "[5/5] Exporting frontend image..."
$WEB_TAR = "${WEB_IMAGE_NAME}-${TAG}.tar"

try {
    docker save "${WEB_IMAGE_NAME}:${TAG}" -o $WEB_TAR
    if ($LASTEXITCODE -ne 0) {
        throw "Frontend image export failed"
    }
    $webSize = [math]::Round((Get-Item $WEB_TAR).Length / 1MB, 2)
    Write-Host "OK: Frontend image exported: $WEB_TAR ($webSize MB)" -ForegroundColor Green
} catch {
    Write-Host "ERROR: Frontend image export failed" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}
Write-Host ""

# Complete
Write-Host "========================================"
Write-Host "Build completed successfully!" -ForegroundColor Green
Write-Host "========================================"
Write-Host ""
Write-Host "Generated files:"
Write-Host "  - $API_TAR ($apiSize MB)"
Write-Host "  - $WEB_TAR ($webSize MB)"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Transfer images to device:"
Write-Host "     hdc file send $API_TAR /data/tinyiothub/"
Write-Host "     hdc file send $WEB_TAR /data/tinyiothub/"
Write-Host ""
Write-Host "  2. Load images on device:"
Write-Host "     hdc shell `"cd /data/tinyiothub && docker load < $API_TAR`""
Write-Host "     hdc shell `"cd /data/tinyiothub && docker load < $WEB_TAR`""
Write-Host ""
Write-Host "  3. Start services:"
Write-Host "     hdc shell `"cd /data/tinyiothub && ./start-containers.sh`""
Write-Host ""
Write-Host "For detailed deployment steps, see: docker/README.md"
Write-Host ""
