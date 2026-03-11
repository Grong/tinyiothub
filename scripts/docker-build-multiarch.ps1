param(
    [string]$Tag = "latest",
    [switch]$Push,
    [string]$Registry = "tinyiothub"
)

$ErrorActionPreference = "Stop"

Write-Host "=== Building TinyIoTHub Multi-Architecture Docker Image ===" -ForegroundColor Cyan

# 检查 buildx 是否可用
Write-Host "`nChecking Docker Buildx..." -ForegroundColor Yellow
$buildxCheck = docker buildx version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "Docker Buildx not available. Please install Docker Desktop or enable buildx." -ForegroundColor Red
    exit 1
}

# 创建或使用 builder
Write-Host "`nSetting up builder..." -ForegroundColor Yellow
$builderName = "tinyiothub-builder"

# 检查并清理旧的 builder
$existingBuilder = docker buildx ls 2>&1 | Select-String $builderName
if ($existingBuilder) {
    Write-Host "Removing old builder: $builderName" -ForegroundColor Gray
    docker buildx rm $builderName 2>&1 | Out-Null
}

# 创建新 builder
Write-Host "Creating builder: $builderName" -ForegroundColor Gray
docker buildx create --name $builderName --driver docker-container --use 2>&1 | Out-Null

# 启动 builder
Write-Host "Bootstrapping builder..." -ForegroundColor Gray
docker buildx inspect --bootstrap 2>&1 | Out-Null

# 构建参数
$platforms = "linux/amd64,linux/arm64"
$imageName = "${Registry}:${Tag}"

Write-Host "`nBuilding for platforms: $platforms" -ForegroundColor Cyan
Write-Host "Image name: $imageName" -ForegroundColor Cyan

$buildArgs = @(
    "buildx", "build",
    "--platform", $platforms,
    "-t", $imageName,
    "-f", "Dockerfile"
)

if ($Push) {
    $buildArgs += "--push"
    Write-Host "Will push to registry after build" -ForegroundColor Yellow
} else {
    $buildArgs += "--load"
    Write-Host "Will load to local Docker (single platform only)" -ForegroundColor Yellow
}

$buildArgs += "."

Write-Host "`nRunning: docker $($buildArgs -join ' ')" -ForegroundColor Gray
& docker @buildArgs

if ($LASTEXITCODE -ne 0) {
    Write-Host "`nDocker build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "`n=== Build Complete ===" -ForegroundColor Green
Write-Host "Image: $imageName" -ForegroundColor Green
Write-Host "Platforms: $platforms" -ForegroundColor Green

if ($Push) {
    Write-Host "`nImage pushed to registry" -ForegroundColor Green
} else {
    Write-Host "`nNote: Multi-arch images require --push to registry" -ForegroundColor Yellow
    Write-Host "Local load only supports single platform (current: $(docker version --format '{{.Server.Arch}}'))" -ForegroundColor Yellow
}

Write-Host "`nRun with: docker-compose up -d" -ForegroundColor Cyan
