# Like FilmCraft - builds the combined installer (FilmCraft + LIKE Diktalas speech engine).
# (ASCII only: Windows PowerShell 5.1 misreads UTF-8 scripts without a BOM.)
#
#   powershell -ExecutionPolicy Bypass -File packaging\like\build-installer.ps1 `
#       -Diktalas "C:\Claude-Code\LIKE-Diktalas\Install\LIKE-Diktalas-....exe"
#
# Needs: cargo build --release -p filmcraft -p filmcraft-cli
# NSIS comes with Tauri (%LOCALAPPDATA%\tauri\NSIS); pass -Makensis for another location.
# Output: target\like-installer (never committed).
param(
    [string]$Diktalas = "",
    [string]$Makensis = "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe",
    [string]$Version = ""
)
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Bin = Join-Path $Root "target\release"

foreach ($exe in @("filmcraft.exe", "filmcraft-cli.exe")) {
    if (-not (Test-Path (Join-Path $Bin $exe))) { throw "Missing: $Bin\$exe - run: cargo build --release -p filmcraft -p filmcraft-cli" }
}
if (-not (Test-Path $Makensis)) { throw "NSIS not found: $Makensis" }

if ($Version -eq "") {
    $cargo = Get-Content (Join-Path $Root "Cargo.toml") -Raw
    $base = [regex]::Match($cargo, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    # the LIKE release number lives in one place: LIKE_RELEASE in crates/engine/src/like_tools.rs
    $tools = Get-Content (Join-Path $Root "crates\engine\src\like_tools.rs") -Raw
    $likeNo = [regex]::Match($tools, 'LIKE_RELEASE:\s*u32\s*=\s*(\d+)').Groups[1].Value
    if ($likeNo -eq "") { throw "LIKE_RELEASE not found in like_tools.rs" }
    $Version = "$($base)-like.$($likeNo)"
}

$OutDir = Join-Path $Root "target\like-installer"
New-Item -ItemType Directory -Force $OutDir | Out-Null
$Out = Join-Path $OutDir "Like-FilmCraft-$Version-x64-Telepito.exe"

$defs = @("/DBINDIR=$Bin", "/DROOTDIR=$Root", "/DVERSION=$Version", "/DOUTFILE=$Out")
if ($Diktalas -ne "") {
    if (-not (Test-Path $Diktalas)) { throw "Diktalas installer not found: $Diktalas" }
    $defs += "/DDIKTALAS=$((Resolve-Path $Diktalas).Path)"
}
& $Makensis /V2 /INPUTCHARSET UTF8 @defs (Join-Path $PSScriptRoot "like-filmcraft.nsi")
if ($LASTEXITCODE -ne 0) { throw "makensis failed ($LASTEXITCODE)" }
Write-Host "Done: $Out"
