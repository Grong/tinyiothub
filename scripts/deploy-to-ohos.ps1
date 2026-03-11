param(
    [Parameter(Mandatory=$true)]
    [string]$DeviceId,
    
    [switch]$BuildImage,
    [switch]$StopOld,
    [string]$ImageTag = "arm64"
)

$ErrorActionPreference = "Stop"

Write-Host "=== TinyIoTHub Deploy to OpenHarmony ===" -ForegroundColor Cyan
Write-Host "Device ID: $DeviceId" -ForegroundColor Gray

# 1. Build image if needed
if ($BuildImage) {
    Write-Host "`n[1/6] Building ARM64 image..." -ForegroundColor Yellow
    docker buildx build --platform linux/arm64 -t tinyiothub:$ImageTag -f Dockerfile . --load 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Image build failed" -ForegroundColor Red
        exit 1
    }
    Write-Host "Image built successfully" -ForegroundColor Green
} else {
    Write-Host "`n[1/6] Skip image build (using existing image)" -ForegroundColor Gray
}

# 2. Export image
Write-Host "`n[2/6] Exporting image..." -ForegroundColor Yellow
$tarFile = "tinyiothub-$ImageTag.tar"
docker save tinyiothub:$ImageTag -o $tarFile 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Image export failed" -ForegroundColor Red
    exit 1
}
Write-Host "Image exported: $tarFile" -ForegroundColor Green

# 3. Stop old container if needed
if ($StopOld) {
    Write-Host "`n[3/6] Stopping old container..." -ForegroundColor Yellow
    hdc -t $DeviceId shell "docker stop tinyiothub 2>/dev/null; docker rm tinyiothub 2>/dev/null" 2>&1 | Out-Null
    Write-Host "Old container stopped" -ForegroundColor Green
} else {
    Write-Host "`n[3/6] Skip stopping old container" -ForegroundColor Gray
}

# 4. Create directories
Write-Host "`n[4/6] Preparing device directories..." -ForegroundColor Yellow
hdc -t $DeviceId shell "mkdir -p /data/tinyiothub/data /data/tinyiothub/logs" 2>&1 | Out-Null
Write-Host "Directories created" -ForegroundColor Green

# 5. Transfer image
Write-Host "`n[5/6] Transferring image to device..." -ForegroundColor Yellow
hdc -t $DeviceId file send $tarFile /data/tinyiothub/ 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Image transfer failed" -ForegroundColor Red
    exit 1
}
Write-Host "Image transferred" -ForegroundColor Green

# 6. Load and start
Write-Host "`n[6/6] Loading image and starting container..." -ForegroundColor Yellow

# Load image
$loadCmd = "cat /data/tinyiothub/$tarFile | docker load"
hdc -t $DeviceId shell $loadCmd 2>&1 | Out-Null

# Start container
$runCmd = "docker run -d --name tinyiothub --restart unless-stopped -p 3002:3002 -v /data/tinyiothub/data:/app/data -v /data/tinyiothub/logs:/app/logs -e RUST_LOG=info -e TZ=Asia/Shanghai tinyiothub:$ImageTag"
hdc -t $DeviceId shell $runCmd 2>&1 | Out-Null

if ($LASTEXITCODE -ne 0) {
    Write-Host "Container start failed" -ForegroundColor Red
    exit 1
}
Write-Host "Container started" -ForegroundColor Green

# Cleanup local file
Remove-Item $tarFile -Force 2>&1 | Out-Null

# Verify deployment
Write-Host "`n=== Deployment Complete ===" -ForegroundColor Green
Write-Host "`nVerify deployment:" -ForegroundColor Cyan
Start-Sleep -Seconds 2
hdc -t $DeviceId shell "docker ps | grep tinyiothub"

Write-Host "`nView logs:" -ForegroundColor Cyan
Write-Host "hdc -t $DeviceId shell 'docker logs tinyiothub --tail 20'" -ForegroundColor Gray

Write-Host "`nGet device IP:" -ForegroundColor Cyan
Write-Host "hdc -t $DeviceId shell 'ifconfig | grep inet'" -ForegroundColor Gray
