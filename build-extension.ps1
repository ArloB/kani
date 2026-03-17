#!/usr/bin/env pwsh
param(
    [Parameter(Position = 0)]
    [string]$Extension = "",

    [Parameter(Position = 1)]
    [string]$Directory = ".\kani-extensions"
)

foreach ($ext in Get-ChildItem "$Directory" -Directory) {  
    if ($ext.Name -eq "kani-example" -or ($Extension -ne "" -and $ext.Name -ne $Extension)) {
        continue
    }

    Write-Host "Building $($ext.Name) extension" -ForegroundColor Cyan

    cargo build --target wasm32-unknown-unknown --profile wasm-release -p $($ext.Name)
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "$($ext.Name) build failed" -ForegroundColor Red
        exit 1
    }
    
    Write-Host "$($ext.Name) build successful" -ForegroundColor Green

    $wasmDir = "wasm_sources"
    if (-not (Test-Path $wasmDir)) {
        New-Item -ItemType Directory -Path $wasmDir | Out-Null
        Write-Host "Created $wasmDir directory" -ForegroundColor Yellow
    }

    $extNameUnderscore = $ext.Name.Replace('-', '_')
    $source = "target\wasm32-unknown-unknown\wasm-release\$($extNameUnderscore).wasm"
    $dest = "$wasmDir\$($ext.Name).wasm"

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

    if (Get-Command "wasm-tools" -ErrorAction SilentlyContinue) {
        Write-Host "Converting to WASM Component" -ForegroundColor Cyan
        $componentWasm = "$dest.component.wasm"
        # Create component from the (potentially optimized) core module
        # -o specifies output
        wasm-tools component new $dest -o $componentWasm

        if ($LASTEXITCODE -eq 0) {
            Move-Item $componentWasm $dest -Force
            Write-Host "Component creation successful" -ForegroundColor Green
        }
        else {
            Write-Host "Failed to create WASM component" -ForegroundColor Red
            exit 1
        }
    }
    else {
        Write-Host "wasm-tools not found! Extension will likely fail to load." -ForegroundColor Red
    }

    $size = (Get-Item $dest).Length / 1KB
    Write-Host "Built extension $($ext.Name) with size: $([math]::Round($size, 2)) KB" -ForegroundColor Cyan
    Write-Host ""
    Write-Host ""
}