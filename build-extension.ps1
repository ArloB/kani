#!/usr/bin/env pwsh
foreach ($extension in Get-ChildItem ".\kani-extensions" -Directory) {
    Write-Host "Building $($extension.Name) extension" -ForegroundColor Cyan
    
    cargo build --target wasm32-unknown-unknown --release -p $($extension.Name)
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "$($extension.Name) build failed" -ForegroundColor Red
        exit 1
    }
    
    Write-Host "$($extension.Name) build successful" -ForegroundColor Green

    $wasmDir = "wasm_sources"
    if (-not (Test-Path $wasmDir)) {
        New-Item -ItemType Directory -Path $wasmDir | Out-Null
        Write-Host "Created $wasmDir directory" -ForegroundColor Yellow
    }

    $extensionNameUnderscore = $extension.Name.Replace('-', '_')
    $source = "target\wasm32-unknown-unknown\release\$($extensionNameUnderscore).wasm"
    $dest = "$wasmDir\$($extension.Name).wasm"

    Copy-Item $source $dest -Force
    Write-Host "Copied to $dest" -ForegroundColor Green

    if (Get-Command "wasm-opt" -ErrorAction SilentlyContinue) {
        Write-Host "Running wasm-opt optimization" -ForegroundColor Cyan
        $tempWasm = "$dest.tmp"
        wasm-opt -Oz -o $tempWasm $dest --enable-bulk-memory 

        if ($LASTEXITCODE -eq 0) {
            Move-Item $tempWasm $dest -Force
            Write-Host "wasm-opt optimization successful" -ForegroundColor Green
        } else {
            Write-Host "wasm-opt failed, keeping original" -ForegroundColor Red
            if (Test-Path $tempWasm) { Remove-Item $tempWasm }
        }
    } else {
        Write-Host "wasm-opt not found, skipping extra optimization." -ForegroundColor Yellow
    }

    $size = (Get-Item $dest).Length / 1KB
    Write-Host "Built extension $($extension.Name) with size: $([math]::Round($size, 2)) KB" -ForegroundColor Cyan
}

Write-Host "Building kani-web frontend" -ForegroundColor Cyan

Set-Location "kani-web"

trunk build --release

if ($LASTEXITCODE -eq 0) {
    Write-Host "Frontend build successful" -ForegroundColor Green
} else {
    Write-Host "Frontend build failed" -ForegroundColor Red
    exit 1
}

Set-Location ..