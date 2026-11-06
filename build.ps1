# Builds Plume and produces its installer: dist\PlumeSetup.exe (+ its SHA-256 checksum).
#
# Requirements (Windows, no Visual Studio needed):
#   - Rust with the x86_64-pc-windows-gnu toolchain  (rustup default stable-x86_64-pc-windows-gnu)
#   - GNU binutils dlltool.exe + windres.exe (+ as.exe and their DLLs) from MSYS2's
#     mingw-w64-x86_64-binutils package, in $env:PLUME_BINUTILS (default: %USERPROFILE%\.mingw-binutils)
#
# Usage:  .\build.ps1
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$binutils = if ($env:PLUME_BINUTILS) { $env:PLUME_BINUTILS } else { "$env:USERPROFILE\.mingw-binutils" }
foreach ($tool in "dlltool.exe", "windres.exe") {
    if (-not (Test-Path (Join-Path $binutils $tool))) {
        Write-Host "Missing $tool in $binutils (set PLUME_BINUTILS, see README > Building)." -ForegroundColor Red
        exit 1
    }
}
$env:Path = "$binutils;$env:USERPROFILE\.cargo\bin;$env:Path"

# 1. The browser itself
cmd /c "cargo build --release 2>&1"
if ($LASTEXITCODE -ne 0) { Write-Host "Browser build failed" -ForegroundColor Red; exit 1 }

# 2. The installer, which embeds the browser + WebView2Loader.dll
$env:PLUME_CORE_EXE = (Resolve-Path target\release\plume.exe).Path
$env:PLUME_LOADER_DLL = (Get-ChildItem "$env:USERPROFILE\.cargo\registry\src\*\webview2-com-sys-*\x64\WebView2Loader.dll" | Select-Object -Last 1).FullName
if (-not $env:PLUME_LOADER_DLL) { Write-Host "WebView2Loader.dll not found in the cargo registry" -ForegroundColor Red; exit 1 }
cmd /c "cargo build --release --manifest-path launcher\Cargo.toml 2>&1"
if ($LASTEXITCODE -ne 0) { Write-Host "Installer build failed" -ForegroundColor Red; exit 1 }

# 3. Output: the installer + its checksum
New-Item -ItemType Directory -Force dist | Out-Null
Get-ChildItem dist | Remove-Item -Force -ErrorAction SilentlyContinue
Copy-Item launcher\target\release\PlumeSetup.exe dist\PlumeSetup.exe -Force
$hash = (Get-FileHash dist\PlumeSetup.exe -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$PSScriptRoot\dist\PlumeSetup.exe.sha256", "$hash  PlumeSetup.exe`n")
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
Write-Host "OK -> $PSScriptRoot\dist\PlumeSetup.exe  v$version  ($([math]::Round((Get-Item dist\PlumeSetup.exe).Length / 1KB)) KB)"
