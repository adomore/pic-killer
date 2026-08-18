# PIC-Killer 功能测试覆盖 —— 断言式，覆盖全部 16 个命令、横切能力与审计发现的回归
$ErrorActionPreference = "Continue"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Drawing

$exe = Join-Path $PSScriptRoot "..\target\release\pic-killer.exe"
if (-not (Test-Path $exe)) { throw "找不到 release 二进制，请先在仓库根目录 cargo build --release" }
$work = Join-Path $env:TEMP "pic-killer-functest"
if ([System.IO.Directory]::Exists($work)) { [System.IO.Directory]::Delete($work, $true) }
[System.IO.Directory]::CreateDirectory($work) | Out-Null

$script:pass = 0; $script:fail = 0; $script:fails = @()
function Assert($ok, $name) {
  if ($ok) { $script:pass++ } else { $script:fail++; $script:fails += $name; Write-Host "[FAIL] $name" -ForegroundColor Red }
}
function P($n) { Join-Path $work $n }
function MkJpg($name, $r=100) { $b=New-Object System.Drawing.Bitmap 64,48; $g=[System.Drawing.Graphics]::FromImage($b); $g.Clear([System.Drawing.Color]::FromArgb($r,120,180)); $g.Dispose(); $b.Save((P $name),[System.Drawing.Imaging.ImageFormat]::Jpeg); $b.Dispose() }
function MkPng($name) { $b=New-Object System.Drawing.Bitmap 40,30; $b.Save((P $name),[System.Drawing.Imaging.ImageFormat]::Png); $b.Dispose() }
function MkBmp($name) { $b=New-Object System.Drawing.Bitmap 20,20; $b.Save((P $name),[System.Drawing.Imaging.ImageFormat]::Bmp); $b.Dispose() }
function PixHash($p) { $b=New-Object System.Drawing.Bitmap $p; $ms=New-Object System.IO.MemoryStream; $b.Save($ms,[System.Drawing.Imaging.ImageFormat]::Bmp); $h=([System.Security.Cryptography.SHA256]::Create().ComputeHash($ms.ToArray())|%{$_.ToString("x2")}) -join ""; $b.Dispose(); $ms.Dispose(); $h }
function Sh($p) { (& $exe show $p 2>&1 | Out-String) }
function Run($argsArr) { & $exe @argsArr 2>&1 | Out-String }

# ---------- time ----------
MkJpg "t1.jpg"; $h=PixHash (P "t1.jpg")
Run @("time",(P "t1.jpg"),"--set","2020-05-01 08:30:00","-y") | Out-Null
Assert ((Sh (P "t1.jpg")) -match "DateTimeOriginal\s+2020:05:01 08:30:00") "time --set"
Assert ((PixHash (P "t1.jpg")) -eq $h) "time 无损"
Run @("time",(P "t1.jpg"),"--shift","+2h","-y") | Out-Null
Assert ((Sh (P "t1.jpg")) -match "DateTimeOriginal\s+2020:05:01 10:30:00") "time --shift"
Run @("time",(P "t1.jpg"),"--set","2023-06-15 18:00:00","--tz","+08:00","-y") | Out-Null
Assert ((Sh (P "t1.jpg")) -match "OffsetTimeOriginal\s+\+08:00") "time --tz OffsetTime"
MkJpg "s1.jpg"; MkJpg "s2.jpg"; MkJpg "s3.jpg"
Run @("time",$work,"--sequential","2021-01-01 00:00:00","--interval","+1h","--ext","jpg","-y") | Out-Null

# from-name
$fn = P "IMG_20230115_143022.jpg"; MkJpg "IMG_20230115_143022.jpg"
Run @("time",$fn,"--from-name","-y") | Out-Null
Assert ((Sh $fn) -match "DateTimeOriginal\s+2023:01:15 14:30:22") "time --from-name"

# ---------- set ----------
MkJpg "set1.jpg"; $h=PixHash (P "set1.jpg")
Run @("set",(P "set1.jpg"),"--artist","张三","--copyright","(C) 2024","--make","Canon","--model","EOS R5","-y") | Out-Null
$o = Sh (P "set1.jpg")
Assert ($o -match "Artist\s+张三") "set --artist 中文"
Assert ($o -match "Make\s+Canon") "set --make"
Assert ((PixHash (P "set1.jpg")) -eq $h) "set 无损"
Run @("set",(P "set1.jpg"),"--set-string","iso=100","--set-string","fnumber=2.8","--set-string","exposuretime=1/200","-y") | Out-Null
$o = Sh (P "set1.jpg")
Assert ($o -match "ISO\s+100") "set 数值 iso"
Assert ($o -match "FNumber\s+14/5") "set 数值 fnumber"
Assert ($o -match "ExposureTime\s+1/200") "set 数值 exposuretime"
Run @("set",(P "set1.jpg"),"--orientation","cw90","-y") | Out-Null
Assert ((Sh (P "set1.jpg")) -match "Orientation\s+6") "set --orientation cw90"
Run @("set",(P "set1.jpg"),"--remove","artist","-y") | Out-Null
Assert (-not ((Sh (P "set1.jpg")) -match "Artist\s+张三")) "set --remove"

# ---------- gps ----------
MkJpg "g1.jpg"; $h=PixHash (P "g1.jpg")
Run @("gps",(P "g1.jpg"),"--lat","39.9042","--lon","116.4074","--alt","50","-y") | Out-Null
$o = Sh (P "g1.jpg")
Assert ($o -match "位置：39\.904200, 116\.407400") "gps 设置+读回"
Assert ($o -match "GPSLongitude\s+116, 24") "gps DMS"
Assert ((PixHash (P "g1.jpg")) -eq $h) "gps 无损"
Run @("gps",(P "g1.jpg"),"--clear","-y") | Out-Null
Assert (-not ((Sh (P "g1.jpg")) -match "GPSLatitude")) "gps --clear"

# ---------- rotate（复合）----------
MkJpg "r1.jpg"
Run @("rotate",(P "r1.jpg"),"--cw","-y") | Out-Null
Assert ((Sh (P "r1.jpg")) -match "Orientation\s+6") "rotate cw (1->6)"
Run @("rotate",(P "r1.jpg"),"--cw","-y") | Out-Null
Assert ((Sh (P "r1.jpg")) -match "Orientation\s+3") "rotate cw 复合 (6->3)"
Run @("rotate",(P "r1.jpg"),"--reset","-y") | Out-Null
Assert ((Sh (P "r1.jpg")) -match "Orientation\s+1") "rotate --reset"

# ---------- copy ----------
MkJpg "ref.jpg"; MkJpg "dst.jpg"
Run @("set",(P "ref.jpg"),"--make","Sony","--artist","参考","-y") | Out-Null
Run @("gps",(P "ref.jpg"),"--lat","31.23","--lon","121.47","-y") | Out-Null
Run @("copy",(P "dst.jpg"),"--from",(P "ref.jpg"),"-y") | Out-Null
$o = Sh (P "dst.jpg")
Assert ($o -match "Make\s+Sony") "copy Make"
Assert ($o -match "位置：31\.23") "copy GPS"

# ---------- strip ----------
MkJpg "st1.jpg"
Run @("set",(P "st1.jpg"),"--artist","X","-y") | Out-Null
Run @("gps",(P "st1.jpg"),"--lat","1","--lon","2","-y") | Out-Null
Run @("strip",(P "st1.jpg"),"--gps","-y") | Out-Null
$o = Sh (P "st1.jpg")
Assert (($o -match "Artist\s+X") -and (-not ($o -match "GPSLatitude"))) "strip --gps 只删GPS"
Run @("strip",(P "st1.jpg"),"-y") | Out-Null
Assert ((Sh (P "st1.jpg")) -match "无匹配的元数据") "strip 全清"

# ---------- xmp (JPEG + PNG) ----------
MkJpg "x1.jpg"; $h=PixHash (P "x1.jpg")
Run @("xmp",(P "x1.jpg"),"--title","西湖","--rating","5","--keywords","风景,西湖","-y") | Out-Null
$o = Sh (P "x1.jpg")
Assert ($o -match "dc:title\s+西湖") "xmp title(JPEG)"
Assert ($o -match "xmp:Rating\s+5") "xmp rating"
Assert ($o -match "dc:subject\s+风景; 西湖") "xmp keywords(Bag)"
Assert ((PixHash (P "x1.jpg")) -eq $h) "xmp 无损"
MkPng "x2.png"
Run @("xmp",(P "x2.png"),"--title","PNG标题","-y") | Out-Null
Assert ((Sh (P "x2.png")) -match "dc:title\s+PNG标题") "xmp PNG(iTXt)"
Run @("xmp",(P "x1.jpg"),"--clear","-y") | Out-Null
Assert (-not ((Sh (P "x1.jpg")) -match "dc:title")) "xmp --clear"

# ---------- iptc ----------
MkJpg "i1.jpg"
Run @("iptc",(P "i1.jpg"),"--title","开幕","--city","北京","--keywords","体育,开幕","-y") | Out-Null
$o = Sh (P "i1.jpg")
Assert ($o -match "Title\s+开幕") "iptc title"
Assert ($o -match "City\s+北京") "iptc city"
Assert ($o -match "Keywords\s+体育; 开幕") "iptc keywords"
Run @("iptc",(P "i1.jpg"),"--clear","-y") | Out-Null
Assert (-not ((Sh (P "i1.jpg")) -match "(?m)^\s+Title\s+开幕")) "iptc --clear"

# ---------- 三套并存 ----------
MkJpg "co.jpg"
Run @("time",(P "co.jpg"),"--set","2022-02-02 12:00:00","-y") | Out-Null
Run @("xmp",(P "co.jpg"),"--rating","4","-y") | Out-Null
Run @("iptc",(P "co.jpg"),"--title","并存","-y") | Out-Null
Run @("set",(P "co.jpg"),"--make","Fuji","-y") | Out-Null
$o = Sh (P "co.jpg")
Assert (($o -match "DateTimeOriginal") -and ($o -match "xmp:Rating\s+4") -and ($o -match "Title\s+并存") -and ($o -match "Make\s+Fuji")) "EXIF+XMP+IPTC 并存"

# ---------- restore ----------
MkJpg "rs.jpg"; $orig=(Get-FileHash (P "rs.jpg") -Algorithm SHA256).Hash
Run @("set",(P "rs.jpg"),"--artist","临时","--backup","-y") | Out-Null
Assert ((Test-Path (P "rs.jpg.bak"))) "backup 生成 .bak"
Run @("restore",(P "rs.jpg"),"-y") | Out-Null
Assert (((Get-FileHash (P "rs.jpg") -Algorithm SHA256).Hash) -eq $orig) "restore 字节级还原"
Assert (-not (Test-Path (P "rs.jpg.bak"))) "restore 移除 .bak"

# ---------- geotag ----------
$gpx = P "track.gpx"
$gpxText = @'
<?xml version="1.0"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg>
<trkpt lat="30.0" lon="120.0"><time>2023-06-15T10:00:00Z</time></trkpt>
<trkpt lat="30.2" lon="120.2"><time>2023-06-15T10:10:00Z</time></trkpt>
</trkseg></trk></gpx>
'@
Set-Content -Path $gpx -Value $gpxText -Encoding utf8
MkJpg "gt1.jpg"; $h=PixHash (P "gt1.jpg")
Run @("time",(P "gt1.jpg"),"--set","2023-06-15 18:05:00","-y") | Out-Null
Run @("geotag",(P "gt1.jpg"),"--gpx",$gpx,"--tz","+08:00","-y") | Out-Null
Assert ((Sh (P "gt1.jpg")) -match "位置：30\.100000, 120\.100000") "geotag 插值坐标"
Assert ((PixHash (P "gt1.jpg")) -eq $h) "geotag 无损"

# ---------- rename ----------
MkJpg "rn.jpg"
Run @("time",(P "rn.jpg"),"--set","2019-07-04 09:15:00","-y") | Out-Null
Run @("rename",(P "rn.jpg"),"-y") | Out-Null
Assert (Test-Path (P "20190704_091500.jpg")) "rename 按拍摄时间"

# ---------- apply (EXIF+XMP+IPTC+数值) ----------
MkJpg "ap.jpg"
$csv = P "meta.csv"
$f = P "ap.jpg"
$csvLines = @("file,field,value","$f,artist,李四","$f,iso,400","$f,xmp:title,标题","$f,xmp:rating,3","$f,iptc:city,上海")
Set-Content -Path $csv -Value $csvLines -Encoding utf8
Run @("apply","--from",$csv,"-y") | Out-Null
$o = Sh $f
Assert (($o -match "Artist\s+李四") -and ($o -match "ISO\s+400")) "apply EXIF+数值"
Assert (($o -match "dc:title\s+标题") -and ($o -match "xmp:Rating\s+3")) "apply xmp:"
Assert ($o -match "City\s+上海") "apply iptc:"

# ---------- report ----------
$rep = Run @("report",$work,"-r")
Assert ($rep -match "元数据统计") "report 输出"
Assert ($rep -match "拍摄时间：") "report 有拍摄时间统计"

# ---------- --where 单条件 + 组合 ----------
MkJpg "w_a.jpg"; MkJpg "w_b.jpg"; MkJpg "w_c.jpg"
Run @("set",(P "w_a.jpg"),"--make","Canon","-y") | Out-Null
Run @("time",(P "w_a.jpg"),"--set","2023-01-01 10:00:00","-y") | Out-Null
Run @("set",(P "w_b.jpg"),"--make","Nikon","-y") | Out-Null
$wAnd = "no-gps && no-date"
$wOr = "make=Canon || make=Nikon"
$rA = Run @("show",$work,"--where",$wAnd,"--ext","jpg")
Assert (($rA -match "w_b\.jpg") -and ($rA -match "w_c\.jpg") -and (-not ($rA -match "w_a\.jpg"))) "--where AND"
$rO = Run @("show",$work,"--where",$wOr,"--ext","jpg")
Assert (($rO -match "w_a\.jpg") -and ($rO -match "w_b\.jpg")) "--where OR"

# ---------- 通配符 ----------
$star = Join-Path $work "w_*.jpg"
$rG = Run @("set",$star,"--software","GLOB","-y")
Assert (($rG -match "w_a\.jpg") -and ($rG -match "w_c\.jpg")) "通配符 *.jpg"

# ---------- BMP 友好跳过 ----------
MkBmp "b.bmp"
$rB = Run @("set",(P "b.bmp"),"--artist","x","-y")
Assert ($rB -match "BMP 无元数据容器") "BMP 友好提示"

# ---------- sidecar (.xmp / RAW) ----------
MkJpg "sc.jpg"; $schash = (Get-FileHash (P "sc.jpg") -Algorithm SHA256).Hash
Run @("xmp",(P "sc.jpg"),"--sidecar","--title","旁挂","--rating","5","-y") | Out-Null
Assert (Test-Path (P "sc.xmp")) "sidecar 生成 .xmp"
Assert (((Get-FileHash (P "sc.jpg") -Algorithm SHA256).Hash) -eq $schash) "sidecar 不改原图"
Assert ((Sh (P "sc.jpg")) -match "sidecar") "show 读 sidecar"
$rawf = P "raw.cr2"; [System.IO.File]::WriteAllBytes($rawf, [byte[]](1..30))
Run @("xmp",$rawf,"--sidecar","--title","RAW","-y") | Out-Null
Assert (Test-Path (P "raw.xmp")) "sidecar 支持 RAW(.cr2)"
Run @("xmp",(P "sc.jpg"),"--sidecar","--clear","-y") | Out-Null
Assert (-not (Test-Path (P "sc.xmp"))) "sidecar --clear"

# ---------- completions / man ----------
$bashc = Run @("completions","bash")
Assert (($bashc -match "geotag") -and ($bashc -match "verify")) "completions bash 含子命令"
Assert ((Run @("completions","--man")) -match "\.TH") "completions --man 手册页"

# ---------- verify ----------
MkJpg "vok.jpg"; MkJpg "vbad.jpg"
Run @("time",(P "vok.jpg"),"--set","2020-01-01 10:00:00","-y") | Out-Null
Run @("gps",(P "vbad.jpg"),"--lat","200","--lon","300","-y") | Out-Null
$vout = Run @("verify",(P "vok.jpg"),(P "vbad.jpg"))
Assert ($vout -match "GPS 坐标越界") "verify 检出 GPS 越界"
Assert ($vout -match "正常 1") "verify 正常计数"
# ---------- 回归：审计发现的缺陷（每条对应 docs/AUDIT.zh.md 的一个编号）----------

# F-01 strip 必须连 XMP 与 IPTC 一起清除，否则「保护隐私」是假的。
# 旧测试只写 EXIF 就去 strip，所以这个缺陷能在全绿套件下存活。
MkJpg "f01.jpg"
Run @("set",(P "f01.jpg"),"--artist","张三","-y") | Out-Null
Run @("gps",(P "f01.jpg"),"--lat","30.2741","--lon","120.1551","-y") | Out-Null
Run @("xmp",(P "f01.jpg"),"--title","家","--creator","张三","--city","杭州","-y") | Out-Null
Run @("iptc",(P "f01.jpg"),"--city","杭州","--creator","张三","-y") | Out-Null
$o = Sh (P "f01.jpg")
Assert (($o -match "dc:creator") -and ($o -match "(?m)^\s+City\s+杭州")) "F-01 前置：三套元数据都已写入"
Run @("strip",(P "f01.jpg"),"-y") | Out-Null
$o = Sh (P "f01.jpg")
Assert (-not ($o -match "dc:creator")) "F-01 strip 清除 XMP"
Assert (-not ($o -match "(?m)^\s+City\s+杭州")) "F-01 strip 清除 IPTC"
$raw01 = [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes((P "f01.jpg")))
Assert (-not $raw01.Contains("张三")) "F-01 strip 后文件里搜不到姓名"
Assert (-not $raw01.Contains("杭州")) "F-01 strip 后文件里搜不到城市"

# F-02 取反条件遇到不存在的标签名会匹配全部文件；必须报错而不是静默放行。
MkJpg "f02a.jpg"; MkJpg "f02b.jpg"
Run @("set",(P "f02a.jpg"),"--make","Canon","-y") | Out-Null
$r = Run @("show",$work,"--where","camera!=Canon","--ext","jpg")
Assert ($r -match "一个都没匹配到") "F-02 未知标签名的 != 被拒绝"
$r = Run @("show",$work,"--where","make!=Canon","--ext","jpg")
Assert (($r -match "筛选 --where") -and (-not ($r -match "一个都没匹配到"))) "F-02 正常的 != 仍然可用"

# F-03 && 与 || 混用必须报错，而不是按其中一种切分后静默误解析。
$r = Run @("show",$work,"--where","has-gps && make!=Canon || make=Nikon","--ext","jpg")
Assert ($r -match "不支持.{0,20}混用") "F-03 混用运算符被拒绝"

# F-04 show --csv --for-apply 的输出必须能被 apply 直接吃回去。
MkJpg "f04src.jpg"; MkJpg "f04dst.jpg"
Run @("set",(P "f04src.jpg"),"--artist","李四","--make","Canon","-y") | Out-Null
Run @("time",(P "f04src.jpg"),"--set","2023-06-15 18:05:00","-y") | Out-Null
Run @("gps",(P "f04src.jpg"),"--lat","30.2741","--lon","120.1551","-y") | Out-Null
Run @("xmp",(P "f04src.jpg"),"--title","西湖","-y") | Out-Null
Run @("iptc",(P "f04src.jpg"),"--city","杭州","-y") | Out-Null
$rt = P "roundtrip.csv"
& $exe show (P "f04src.jpg") --csv --for-apply | Set-Content -Path $rt -Encoding utf8
Assert ((Get-Content $rt -First 1) -eq "file,field,value") "F-04 三列表头"
(Get-Content $rt) -replace [regex]::Escape((P "f04src.jpg")), (P "f04dst.jpg") | Set-Content -Path $rt -Encoding utf8
Run @("apply","--from",$rt,"-y") | Out-Null
$o = Sh (P "f04dst.jpg")
Assert ($o -match "Artist\s+李四") "F-04 闭环回写 EXIF"
Assert ($o -match "位置：30\.274100") "F-04 闭环回写 GPS"
Assert ($o -match "dc:title\s+西湖") "F-04 闭环回写 XMP"
Assert ($o -match "(?m)^\s+City\s+杭州") "F-04 闭环回写 IPTC"

# F-05 RAW+JPEG 同名对会解析到同一个 .xmp，必须在派发前拦下而不是互相覆盖。
$pairDir = Join-Path $work "rawpair"
[System.IO.Directory]::CreateDirectory($pairDir) | Out-Null
$b=New-Object System.Drawing.Bitmap 16,16; $b.Save((Join-Path $pairDir "IMG_0001.JPG"),[System.Drawing.Imaging.ImageFormat]::Jpeg); $b.Dispose()
[System.IO.File]::WriteAllBytes((Join-Path $pairDir "IMG_0001.CR2"), [byte[]](1..40))
$r = Run @("xmp",$pairDir,"--ext","cr2,jpg","--sidecar","--title","X","-y")
Assert ($r -match "sidecar 路径冲突") "F-05 sidecar 冲突被拦下"
Assert (-not (Test-Path (Join-Path $pairDir "IMG_0001.xmp"))) "F-05 冲突时不产生 sidecar"

# F-06 verify 要能区分「没有元数据」与「元数据损坏」。
MkJpg "f06none.jpg"
MkJpg "f06bad.jpg"; Run @("set",(P "f06bad.jpg"),"--make","Canon","-y") | Out-Null
$bytes = [System.IO.File]::ReadAllBytes((P "f06bad.jpg"))
$at = -1
for ($i=0; $i -lt $bytes.Length-6; $i++) {
  if ($bytes[$i] -eq 0x45 -and $bytes[$i+1] -eq 0x78 -and $bytes[$i+2] -eq 0x69 -and $bytes[$i+3] -eq 0x66 -and $bytes[$i+4] -eq 0 -and $bytes[$i+5] -eq 0) { $at = $i; break }
}
if ($at -ge 0) { for ($k=$at+6; $k -lt [Math]::Min($at+46,$bytes.Length); $k++) { $bytes[$k] = 0xAB } }
[System.IO.File]::WriteAllBytes((P "f06bad.jpg"), $bytes)
Assert ((Run @("verify",(P "f06none.jpg"))) -match "正常 1") "F-06 无元数据不算问题"
Assert ((Run @("verify",(P "f06bad.jpg"))) -match "无法读取元数据") "F-06 检出损坏的元数据"

# F-07 README 文档写的 gps.clear 必须真的能用。
MkJpg "f07.jpg"; Run @("gps",(P "f07.jpg"),"--lat","1","--lon","2","-y") | Out-Null
$c7 = P "f07.csv"
Set-Content -Path $c7 -Encoding utf8 -Value @("file,field,value", ('"' + (P "f07.jpg") + '",gps.clear,1'))
Run @("apply","--from",$c7,"-y") | Out-Null
Assert (-not ((Sh (P "f07.jpg")) -match "GPSLatitude")) "F-07 gps.clear 可用"

# F-08 用户输入不该让进程 panic。
$gpx8 = P "f08.gpx"
Set-Content -Path $gpx8 -Encoding utf8 -Value @('<?xml version="1.0"?>','<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg>','<trkpt lat="30.0" lon="120.0"><time>2023-06-15T02:00:00Z</time></trkpt>','</trkseg></trk></gpx>')
Assert (-not ((Run @("geotag",(P "f07.jpg"),"--gpx",$gpx8,"--tz","😀","-y")) -match "panicked")) "F-08 --tz 非 ASCII 不 panic"
Assert (-not ((Run @("rename",(P "f07.jpg"),"--pattern","%Q","-y")) -match "panicked")) "F-08 --pattern 非法说明符不 panic"
Assert (-not ((Run @("show",(P "f07.jpg"),"--where","😀=x")) -match "panicked")) "F-08 --where 非 ASCII 不 panic"
Assert (-not ((Run @("show",(P "f07.jpg"),"--where","no:😀")) -match "panicked")) "F-08 --where no: 非 ASCII 不 panic"

# F-09 零匹配时 stdout 仍须是合法的机器可读文档。
$emptyJson = (& $exe show $work --ext "nosuchext" --json 2>$null | Out-String).Trim()
Assert ($emptyJson -eq "[]") "F-09 空结果 --json 输出 []"
$emptyCsv = (& $exe show $work --ext "nosuchext" --csv 2>$null | Out-String).Trim()
Assert ($emptyCsv -eq "file,group,name,hex,value") "F-09 空结果 --csv 只输出表头"

# F-10 apply 必须一次写入。中途某一套元数据失败时，前面的不能已经落盘。
# PNG 不支持 IPTC，于是 EXIF 成功、IPTC 失败——正好用来验证「要么全写要么不写」。
MkPng "f10.png"
$before10 = (Get-FileHash (P "f10.png") -Algorithm SHA256).Hash
$c10 = P "f10.csv"
Set-Content -Path $c10 -Encoding utf8 -Value @("file,field,value", ('"' + (P "f10.png") + '",artist,张三'), ('"' + (P "f10.png") + '",iptc:city,北京'))
$r10 = Run @("apply","--from",$c10,"-y")
Assert ($r10 -match "\[失败\]") "F-10 不支持的一段导致整条记录失败"
Assert (((Get-FileHash (P "f10.png") -Algorithm SHA256).Hash) -eq $before10) "F-10 失败时文件字节未变（无半套写入）"

# F-12 xmp: 后面认不出来的简称必须报错，而不是静默生成 dc:<名字>。
MkJpg "f12.jpg"
$c12 = P "f12.csv"
Set-Content -Path $c12 -Encoding utf8 -Value @("file,field,value", ('"' + (P "f12.jpg") + '",xmp:ttile,标题'))
$r12 = Run @("apply","--from",$c12,"-y")
Assert ($r12 -match "未知的 XMP 字段") "F-12 未知 xmp 简称被拒绝"
Assert (-not ((Sh (P "f12.jpg")) -match "dc:ttile")) "F-12 不产生 dc:ttile 垃圾属性"

# F-13 打错的路径不能被静默丢掉。
$r13 = Run @("show",(P "f12.jpg"),(P "NOSUCHFILE.jpg"))
Assert ($r13 -match "路径不存在") "F-13 不存在的路径被报出"

# F-15 XMP 限定名会被拼进标签位置，必须校验而不是原样插入。
$r15 = Run @("xmp",(P "f12.jpg"),"--set","a><evil>:x=1","-y")
Assert (($r15 -match "非法的 XMP 属性名") -or ($r15 -match "未知的命名空间前缀")) "F-15 非法限定名被拒绝"
$raw15 = [System.IO.File]::ReadAllBytes((P "f12.jpg"))
Assert (-not ([System.Text.Encoding]::UTF8.GetString($raw15)).Contains("<evil>")) "F-15 注入内容未进入文件"

# F-18 --sequential 按自然顺序递增，而不是字节序（否则 IMG_10 排在 IMG_2 前面）。
$natDir = Join-Path $work "natsort"
[System.IO.Directory]::CreateDirectory($natDir) | Out-Null
foreach ($n in 1,2,10) {
  $b=New-Object System.Drawing.Bitmap 16,16
  $b.Save((Join-Path $natDir ("IMG_{0}.jpg" -f $n)),[System.Drawing.Imaging.ImageFormat]::Jpeg); $b.Dispose()
}
Run @("time",$natDir,"--sequential","2021-01-01 00:00:00","--interval","+1h","--ext","jpg","-y") | Out-Null
Assert ((& $exe show (Join-Path $natDir "IMG_1.jpg")  2>&1 | Out-String) -match "DateTimeOriginal\s+2021:01:01 00:00:00") "F-18 IMG_1 排第一"
Assert ((& $exe show (Join-Path $natDir "IMG_2.jpg")  2>&1 | Out-String) -match "DateTimeOriginal\s+2021:01:01 01:00:00") "F-18 IMG_2 排第二"
Assert ((& $exe show (Join-Path $natDir "IMG_10.jpg") 2>&1 | Out-String) -match "DateTimeOriginal\s+2021:01:01 02:00:00") "F-18 IMG_10 排第三（自然序）"

# ---------- 汇总 ----------
$total = $script:pass + $script:fail
Write-Host ""
Write-Host ("功能测试：{0}/{1} 通过" -f $script:pass, $total) -ForegroundColor $(if ($script:fail -eq 0) {"Green"} else {"Red"})
if ($script:fail -gt 0) { Write-Host "失败项：" -ForegroundColor Red; $script:fails | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }; exit 1 }
Write-Host "全部功能测试通过 ✓" -ForegroundColor Green
exit 0
