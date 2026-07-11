# PIC-Killer 性能测试覆盖 —— 大批量并行 vs 顺序，验证高负载下正确性与吞吐
$ErrorActionPreference = "Continue"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Drawing

$exe = Join-Path $PSScriptRoot "..\target\release\pic-killer.exe"
$work = Join-Path $env:TEMP "pic-killer-perftest"
if ([System.IO.Directory]::Exists($work)) { [System.IO.Directory]::Delete($work, $true) }
[System.IO.Directory]::CreateDirectory($work) | Out-Null

$N = 500
$pass = 0; $fail = 0
function Assert($ok, $name) { if ($ok) { $script:pass++; Write-Host "[PASS] $name" -ForegroundColor Green } else { $script:fail++; Write-Host "[FAIL] $name" -ForegroundColor Red } }

Write-Host ("CPU 核数: {0}；生成 {1} 张 JPEG..." -f $env:NUMBER_OF_PROCESSORS, $N)
for ($i = 1; $i -le $N; $i++) {
  $fn = Join-Path $work ("img{0:D4}.jpg" -f $i)
  $b = New-Object System.Drawing.Bitmap 96,72
  $g = [System.Drawing.Graphics]::FromImage($b); $g.Clear([System.Drawing.Color]::FromArgb(($i % 256),100,150)); $g.Dispose()
  $b.Save($fn, [System.Drawing.Imaging.ImageFormat]::Jpeg); $b.Dispose()
}

# 顺序 (-j 1)
$t1 = Measure-Command { & $exe set $work --artist "SEQ" -j 1 -y 2>$null | Out-Null }
$seqMs = [math]::Round($t1.TotalMilliseconds)
Write-Host ("顺序 (-j 1)   : {0} ms  ({1:N0} 张/秒)" -f $seqMs, ($N / $t1.TotalSeconds))

# 并行 (默认)
$t2 = Measure-Command { & $exe set $work --copyright "PAR" -y 2>$null | Out-Null }
$parMs = [math]::Round($t2.TotalMilliseconds)
Write-Host ("并行 (默认)   : {0} ms  ({1:N0} 张/秒)" -f $parMs, ($N / $t2.TotalSeconds))
Write-Host ("加速比: {0:N2}x" -f ($t1.TotalMilliseconds / $t2.TotalMilliseconds))

# 指定线程数
$t3 = Measure-Command { & $exe set $work --software "J4" -j 4 -y 2>$null | Out-Null }
Write-Host ("并行 (-j 4)   : {0} ms" -f [math]::Round($t3.TotalMilliseconds))

# 正确性：高负载并行后，全部文件都应含 3 个标签（artist/copyright/software）
$okCount = 0
Get-ChildItem $work -Filter *.jpg | ForEach-Object {
  $o = & $exe show $_.FullName 2>$null | Out-String
  if (($o -match "Artist\s+SEQ") -and ($o -match "Copyright\s+PAR") -and ($o -match "Software\s+J4")) { $okCount++ }
}
Write-Host ("正确性：{0}/{1} 个文件三标签齐全" -f $okCount, $N)
Assert ($okCount -eq $N) "并行高负载下全部文件正确写入"
Assert ($parMs -le $seqMs) "并行不慢于顺序"

# read-heavy：report 扫描 500 张
$t4 = Measure-Command { & $exe report $work 2>$null | Out-Null }
Write-Host ("report 扫描 {0} 张: {1} ms" -f $N, [math]::Round($t4.TotalMilliseconds))
Assert ($t4.TotalSeconds -lt 30) "report 大批量在合理时间内完成"

# 无残留临时文件
$tmp = (Get-ChildItem $work -Filter "*.pkick.tmp" -Force | Measure-Object).Count
Assert ($tmp -eq 0) "无残留临时文件"

$total = $pass + $fail
Write-Host ""
Write-Host ("性能测试：{0}/{1} 通过" -f $pass, $total) -ForegroundColor $(if ($fail -eq 0) {"Green"} else {"Red"})
if ($fail -gt 0) { exit 1 }
Write-Host "全部性能测试通过 ✓" -ForegroundColor Green
exit 0
