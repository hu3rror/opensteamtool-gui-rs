# 便携版构建打包脚本：版本同步 + cargo build --release + 打 ZIP（exe + dlls/ 占位）。
# 本地与 CI（.github/workflows/release.yml）共用。
#
# 用法：pwsh -File tools/build-release.ps1 [-Version <字符串>]（需 pwsh 7+）
#   默认 Version=0.0.0（不碰 Cargo.toml），产物 opensteamtool-manager-<Version>.zip 于仓库根目录。

param(
    [string]$Version = "0.0.0"
)

$ErrorActionPreference = "Stop"

# 版本同步（#34 修订）：-Version 给定（≠ 默认占位）时把 Cargo.toml 的 version 字段
# 改为该版本（去 v 前缀）。软件版本/应用更新检查读 CARGO_PKG_VERSION——此前发布
# 从不同步导致所有发布包都显示陈旧的 0.2.4，且应用更新检查永远报「有新版本」。
if ($Version -and $Version -ne "0.0.0") {
    $v = $Version -replace "^v", ""
    $path = Join-Path (Get-Location) "Cargo.toml"
    $text = [System.IO.File]::ReadAllText($path)
    $new = [regex]::Replace($text, '^version = "[^"]*"', "version = `"$v`"", "Multiline")
    if ($new -ne $text) {
        [System.IO.File]::WriteAllText($path, $new)  # 默认无 BOM UTF-8（pwsh 7 惯例）
        Write-Host "==> Cargo.toml version -> $v"
    }
}

Write-Host "==> cargo build --release"
cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Host "build failed: $LASTEXITCODE"; exit $LASTEXITCODE }

# 便携版结构：exe + 同目录 dlls/（补丁 DLL 的存放处）。
# 仓库不含 DLL 本体；随工具分发的 DLL 由「检查更新/下载并解压」从线上拉取。
$dllDir = Join-Path (Get-Location) "dlls"
New-Item -ItemType Directory -Force -Path $dllDir | Out-Null

$placeholder = @"
本目录存放目标补丁 DLL（OpenSteamTool.dll / dwmapi.dll / xinput1_4.dll）。

程序会自动创建并使用本目录：
- 点「检查更新」+「下载并解压新版本」可自动拉取目标 DLL 到此目录；
- 也可手动解压 OpenSteamTool 发布包，把上述三个 DLL 放到这里。
"@
[System.IO.File]::WriteAllText((Join-Path $dllDir "README.txt"), $placeholder)  # 默认无 BOM UTF-8

$zip = Join-Path (Get-Location) "opensteamtool-manager-$Version.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }

Write-Host "==> packaging $zip"
Compress-Archive `
    -Path (Join-Path (Get-Location) "target\release\opensteamtool-manager.exe"), $dllDir `
    -DestinationPath $zip `
    -CompressionLevel Optimal

$sizeMB = [math]::Round((Get-Item $zip).Length / 1MB, 2)
Write-Host "==> done: $zip ($sizeMB MB)"