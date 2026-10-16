# Builds the release sidecar binary with PyInstaller (one-file).
# Run from the repo root on each release OS. Output lands in
# src-tauri/binaries/, where tauri.conf.json picks it up as an externalBin.
# Not part of CI: release engineering only.

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$VenvPython = Join-Path $Root "sidecar\.venv\Scripts\python.exe"
if (-not (Test-Path -LiteralPath $VenvPython)) {
  $VenvPython = "python"
}

& $VenvPython -m pip install --upgrade pyinstaller 2>&1 | Out-Null

$OutDir = Join-Path $Root "src-tauri\binaries"
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

Push-Location (Join-Path $Root "sidecar")
try {
  & $VenvPython -m PyInstaller `
    --noconfirm `
    --clean `
    --onefile `
    --name endlessli-sidecar `
    --paths . `
    --distpath $OutDir `
    --workpath (Join-Path $env:TEMP "endlessli-pyinstaller") `
    --specpath (Join-Path $env:TEMP "endlessli-pyinstaller") `
    --hidden-import duckduckgo_search `
    --hidden-import bs4 `
    --collect-submodules duckduckgo_search `
    -m endlessli_sidecar
} finally {
  Pop-Location
}

Write-Host "Sidecar binary written to $OutDir"
Write-Host "Register it in src-tauri/tauri.conf.json under bundle.externalBin before running tauri build."
