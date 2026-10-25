# Compila el instalador de Windows en un PC con Windows.
#
# Requisitos:
#   1. Rust (https://rustup.rs) con el toolchain MSVC.
#   2. Node.js 20 o superior.
#   3. Microsoft Visual Studio Build Tools con "Desarrollo para el escritorio con C++".
#   4. WebView2 Runtime (Windows 10/11 lo trae; si no, el instalador lo descarga).
#
# Uso (PowerShell, desde la raíz del proyecto):
#   $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "$HOME\.tauri\nebula-launcher.key"
#   .\scripts\build-windows.ps1
#
# El instalador queda en:
#   src-tauri\target\release\bundle\nsis\
#
# La clave privada NUNCA se sube al repositorio. Para CI usa los secretos
# TAURI_SIGNING_PRIVATE_KEY y TAURI_SIGNING_PRIVATE_KEY_PASSWORD.

$ErrorActionPreference = "Stop"

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    throw "Define TAURI_SIGNING_PRIVATE_KEY antes de compilar (ver la cabecera del script)."
}

npm ci
if ($LASTEXITCODE -ne 0) { throw "npm ci falló" }

npm run tauri build -- --bundles nsis
if ($LASTEXITCODE -ne 0) { throw "La compilación falló" }

$bundle = Join-Path $PSScriptRoot "..\src-tauri\target\release\bundle\nsis"
Write-Host ""
Write-Host "Instalador y firma generados en:" -ForegroundColor Green
Get-ChildItem $bundle | Select-Object Name, Length
