# 分析截图各行像素颜色，定位黑色区域范围。用法: powershell -File tools/analyze_shot.ps1 [-Png xxx.png]
param([string]$Png = "shot_baseline.png")
Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile((Join-Path (Get-Location) $Png))
$w = $bmp.Width; $h = $bmp.Height
Write-Output "size ${w}x${h}"
# 每 20 行采样一行，统计该行像素: 纯黑占比/平均亮度
for ($y = 0; $y -lt $h; $y += 20) {
    $black = 0; $sum = 0; $n = 0
    for ($x = 0; $x -lt $w; $x += 4) {
        $c = $bmp.GetPixel($x, $y)
        $luma = ($c.R + $c.G + $c.B) / 3
        if ($luma -lt 20) { $black++ }
        $sum += $luma; $n++
    }
    $pct = [math]::Round(100.0 * $black / $n, 1)
    $avg = [math]::Round($sum / $n, 0)
    Write-Output ("y={0,4}  black%={1,5}  avgLuma={2,3}" -f $y, $pct, $avg)
}
# 底部 40 行是否全黑
$bottomBlack = 0; $bottomN = 0
for ($y = $h - 40; $y -lt $h; $y++) {
    for ($x = 0; $x -lt $w; $x += 2) {
        $c = $bmp.GetPixel($x, $y)
        if (($c.R + $c.G + $c.B) / 3 -lt 20) { $bottomBlack++ }
        $bottomN++
    }
}
Write-Output ("bottom-40px black ratio: {0}%" -f [math]::Round(100.0 * $bottomBlack / $bottomN, 1))
$bmp.Dispose()