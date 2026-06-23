# RustCodeGraph standalone installer for Windows (PowerShell).
#
# Downloads a native Rust binary bundle from GitHub Releases. No Node.js or
# build tools required.
#
#   irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex
#
# Upgrade with `rustcodegraph upgrade` (or just re-run this). To uninstall: remove
# $env:LOCALAPPDATA\rustcodegraph and drop its \current\bin entry from your user PATH.
#
# Environment:
#   RUSTCODEGRAPH_VERSION release tag to install (default: latest)
#   RUSTCODEGRAPH_INSTALL_DIR  install location (default: %LOCALAPPDATA%\rustcodegraph)

$ErrorActionPreference = 'Stop'
$repo = 'hunzhiwange/rustcodegraph'
$installDir = if ($env:RUSTCODEGRAPH_INSTALL_DIR) {
  $env:RUSTCODEGRAPH_INSTALL_DIR
} else {
  Join-Path $env:LOCALAPPDATA 'rustcodegraph'
}

# 1. Detect architecture -> target matching the release archives.
$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' }
$target = "win32-$arch"
$artifactTarget = if ($arch -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }

# 2. Resolve the version (latest release unless pinned).
$version = $env:RUSTCODEGRAPH_VERSION
if (-not $version) {
  $version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
}
if (-not $version) { throw "rustcodegraph: could not resolve latest version; set RUSTCODEGRAPH_VERSION." }

# 3. Download + extract the bundle into a stable 'current' dir (overwritten on upgrade).
$url = "https://github.com/$repo/releases/download/$version/rustcodegraph-$artifactTarget.zip"
Write-Host "Installing RustCodeGraph $version ($target)..."
$tmp = Join-Path $env:TEMP ("cg-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp 'cg.zip'
Invoke-WebRequest -Uri $url -OutFile $zip

$dest = Join-Path $installDir 'current'
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Expand-Archive -Path $zip -DestinationPath $dest -Force
$binDir = Join-Path $dest 'bin'
New-Item -ItemType Directory -Force -Path $binDir | Out-Null
$exe = Join-Path $dest "rustcodegraph-$artifactTarget\bin\rustcodegraph.exe"
if (-not (Test-Path $exe)) { throw "rustcodegraph: downloaded archive did not contain rustcodegraph-$artifactTarget\bin\rustcodegraph.exe" }
Move-Item -Path $exe -Destination (Join-Path $binDir 'rustcodegraph.exe') -Force
if (-not (Test-Path (Join-Path $dest 'package.json'))) {
  @{ name = 'rustcodegraph'; version = $version.TrimStart('v') } |
    ConvertTo-Json | Set-Content -Encoding UTF8 -Path (Join-Path $dest 'package.json')
}
Remove-Item -Recurse -Force $tmp

# 4. Put the launcher dir on the user's PATH.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $binDir) {
  [Environment]::SetEnvironmentVariable('Path', "$binDir;$userPath", 'User')
  Write-Host "Added $binDir to your PATH (restart your terminal to pick it up)."
}

Write-Host "Installed to $dest"
Write-Host "Run: rustcodegraph --help"
