# Update the scoop manifest for opensteamtool-gui-rs: bump version/url/hash.
# Shared between local runs and CI (release.yml "Update scoop manifest" step).
#
# Usage:
#   pwsh -File tools/update-scoop-manifest.ps1 -Version v0.6.3 -ManifestPath <json> [-ZipPath <zip>]
# - Version may be with or without the leading v.
# - ZipPath: hash a local zip (the exact release artifact in CI). When omitted,
#   the zip is downloaded from the release URL and hashed (needs network).
# Idempotent: when version and hash are both unchanged, the file is not written.

param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$ManifestPath,
    [string]$ZipPath = ""
)

$ErrorActionPreference = "Stop"

$ver = $Version.TrimStart('v')
$url = "https://github.com/hu3rror/opensteamtool-gui-rs/releases/download/v$ver/opensteamtool-manager-v$ver.zip"

# 1. SHA256 of the zip: prefer the local artifact (identical to what the release uploaded).
if ($ZipPath -and (Test-Path $ZipPath)) {
    $hash = (Get-FileHash $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
} else {
    $tmp = Join-Path $env:TEMP "ost-scoop-$ver.zip"
    Invoke-WebRequest -Uri $url -OutFile $tmp
    try { $hash = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToLowerInvariant() }
    finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}

# 2. Idempotent short-circuit: nothing to do when version and hash are both unchanged.
$manifest = Get-Content $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.version -eq $ver -and $manifest.hash -eq $hash) {
    Write-Host "unchanged (v$ver / $hash)"
    exit 0
}

# 3. Bump version/url/hash and write back (BOM-less UTF-8, trailing newline).
# ConvertTo-Json indents 2 spaces per level; the manifest uses 4 — double leading
# whitespace so the first bump does not reindent the whole file.
$manifest.version = $ver
$manifest.url = $url
$manifest.hash = $hash
$json = ($manifest | ConvertTo-Json -Depth 10) -replace '(?m)^(\s*)', '$1$1'
[System.IO.File]::WriteAllText($ManifestPath, $json + [Environment]::NewLine)

Write-Host "updated $ManifestPath -> v$ver ($hash)"
