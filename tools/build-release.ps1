# 便携版构建打包脚本：cargo build --release + 打 ZIP（exe + dlls/ 占位）。
# 本地与 CI（.github/workflows/release.yml）共用。
#
# 用法：powershell -File tools/build-release.ps1 [-Version <字符串>]
#   默认 Version=0.0.0，产物 opensteamtool-manager-<Version>.zip 于仓库根目录。

param(
    [string]$Version = "0.0.0"
)

$ErrorActionPreference = "Stop"

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
[System.IO.File]::WriteAllText(
    (Join-Path $dllDir "README.txt"),
    $placeholder,
    [System.Text.UTF8Encoding]::new($false)
)

$zip = Join-Path (Get-Location) "opensteamtool-manager-$Version.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }

Write-Host "==> packaging $zip"
Compress-Archive `
    -Path (Join-Path (Get-Location) "target\release\opensteamtool-manager.exe"), $dllDir `
    -DestinationPath $zip `
    -CompressionLevel Optimal

$sizeMB = [math]::Round((Get-Item $zip).Length / 1MB, 2)
Write-Host "==> done: $zip ($sizeMB MB)"