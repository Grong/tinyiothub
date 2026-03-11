param(
    [string]$Target = "",
    [switch]$Release
)

$ErrorActionPreference = "Stop"

Write-Host "=== Build Single Binary ===" -ForegroundColor Cyan

Write-Host "`n[1/3] Building frontend..." -ForegroundColor Yellow
Push-Location web
try {
    pnpm run build > $null
    if ($LASTEXITCODE -ne 0) {
        throw "Frontend build failed"
    }
    Write-Host "OK Frontend built" -ForegroundColor Green
} finally {
    Pop-Location
}

Write-Host "`n[2/3] Preparing frontend files..." -ForegroundColor Yellow
if (Test-Path "api/wwwroot") {
    $retries = 3
    for ($i = 1; $i -le $retries; $i++) {
        try {
            Remove-Item -Recurse -Force "api/wwwroot" -ErrorAction Stop
            break
        } catch {
            if ($i -lt $retries) { Start-Sleep -Seconds 1 }
        }
    }
}

# 复制静态导出的 out 目录内容
New-Item -ItemType Directory -Path "api/wwwroot" -Force | Out-Null
Copy-Item -Recurse "web/out/*" "api/wwwroot/" -Force

Write-Host "OK Frontend files ready" -ForegroundColor Green

Write-Host "`n[3/3] Building backend..." -ForegroundColor Yellow
Push-Location api
try {
    $buildArgs = @("build")
    
    if ($Release) {
        $buildArgs += "--release"
    }
    
    if ($Target) {
        $buildArgs += "--target", $Target
    }
    
    Write-Host "Running: cargo $($buildArgs -join ' ')" -ForegroundColor Gray
    & cargo @buildArgs > $null
    
    if ($LASTEXITCODE -ne 0) {
        throw "Backend build failed"
    }
    
    Write-Host "OK Backend built" -ForegroundColor Green
} finally {
    Pop-Location
}

Write-Host "`n=== Build Complete ===" -ForegroundColor Cyan

if ($Release) {
    $profile = "release"
} else {
    $profile = "debug"
}

if ($Target) {
    $binaryPath = "api/target/$Target/$profile/tinyiothub.exe"
} else {
    $binaryPath = "api/target/$profile/tinyiothub.exe"
}

if (Test-Path $binaryPath) {
    $size = (Get-Item $binaryPath).Length / 1MB
    Write-Host "Binary: $binaryPath" -ForegroundColor Green
    Write-Host "Size: $([math]::Round($size, 2)) MB" -ForegroundColor Green
}

Write-Host "`nRun with: $binaryPath" -ForegroundColor Cyan
