# generate-ico.ps1 — 从 assets/logo.png（深色版 LOGO）生成多尺寸 app.ico。
#
# app.ico 经 build.rs + app.rc 嵌入 exe 作为静态 shell 图标（资源管理器 / 未运行
# 快捷方式 / 锁定任务栏）；只能是一份静态资源，固定用深色版 logo.png（ADR-0016
# 图标双态：运行中窗口 / 托盘 / 主页面 LOGO 按主题切换，exe 静态图标不随主题变）。
#
# 输出标准 Windows 多尺寸 ICO（16/24/32/48/64/128/256，PNG 压缩帧，Vista+ 支持），
# 避免旧单尺寸 256 在 16/24/32 靠系统缩放变糊。依赖 .NET System.Drawing，pwsh 7 可用。
#
# 用法：pwsh -File tools/generate-ico.ps1

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$repo = Split-Path -Parent $PSScriptRoot
$src = Join-Path $repo 'assets\logo.png'
$out = Join-Path $repo 'app.ico'
$sizes = @(16, 24, 32, 48, 64, 128, 256)

if (-not (Test-Path $src)) {
    throw "source logo not found: $src"
}

$source = [System.Drawing.Bitmap]::FromFile($src)
$frames = [System.Collections.Generic.List[byte[]]]::new()
try {
    foreach ($s in $sizes) {
        $bmp = New-Object System.Drawing.Bitmap $s, $s
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
        $g.DrawImage($source, 0, 0, $s, $s)
        $g.Dispose()
        $ms = [System.IO.MemoryStream]::new()
        $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $frames.Add($ms.ToArray())
        $ms.Dispose()
        $bmp.Dispose()
    }
}
finally {
    $source.Dispose()
}

# ICONDIR(6B) + ICONDIRENTRY(16B × n) + 各帧 PNG 数据。
$ico = [System.IO.MemoryStream]::new()
$bw = [System.IO.BinaryWriter]::new($ico)
$bw.Write([uint16]0)                  # reserved
$bw.Write([uint16]1)                  # type: icon
$bw.Write([uint16]$frames.Count)
$offset = 6 + 16 * $frames.Count
for ($i = 0; $i -lt $frames.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { [byte]$sizes[$i] }
    $bw.Write([byte]$dim)             # width (0 = 256)
    $bw.Write([byte]$dim)             # height (0 = 256)
    $bw.Write([byte]0)                # palette colors
    $bw.Write([byte]0)                # reserved
    $bw.Write([uint16]1)              # planes
    $bw.Write([uint16]32)             # bit count
    $bw.Write([uint32]$frames[$i].Length)
    $bw.Write([uint32]$offset)
    $offset += $frames[$i].Length
}
foreach ($f in $frames) { $bw.Write($f) }
$bw.Flush()
[System.IO.File]::WriteAllBytes($out, $ico.ToArray())
$bw.Dispose()

Write-Host "Generated $out ($($frames.Count) frames: $($sizes -join ', '))"