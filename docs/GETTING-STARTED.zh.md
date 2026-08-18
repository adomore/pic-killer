# PIC-Killer · 新手入门手册

> 给从没用过 PIC-Killer 的人的 20 分钟入门。读完一遍，之后把[用户手册](USER-MANUAL.zh.md)放在手边当参考。

[English](GETTING-STARTED.md) | **中文**

---

## 1. PIC-Killer 是做什么的

PIC-Killer 在命令行里批量修改照片的**元数据** —— 拍摄时间、作者、版权、相机、镜头、GPS、方向、标题、关键词、评分。

它从不重新编码图像。它只删掉旧的元数据段、插入新的，压缩后的像素数据逐字节不动。改前改后各解码一次，得到的像素完全一致。

一张照片可以同时携带三套互相独立的元数据。PIC-Killer 三套都能读写，并保证动其中一套绝不破坏另外两套：

| 体系 | 存放位置 | 通常装什么 |
|------|----------|-----------|
| EXIF | JPEG APP1，以及 PNG/TIFF/WebP/HEIC 容器 | 拍摄时间、相机、镜头、曝光参数、GPS、方向 |
| XMP | JPEG APP1、PNG iTXt，或旁挂的 `.xmp` 文件 | 标题、描述、评分、关键词、颜色标签、城市 |
| IPTC-IIM | JPEG APP13（Photoshop 8BIM），仅 JPEG | 新闻与图库工作流字段：图注、作者署名、提供者 |

按你想做什么来挑命令：

| 我想…… | 命令 |
|--------|------|
| 看看这张照片现在有哪些元数据 | `show` |
| 修正错误的拍摄时间 | `time` |
| 设置作者、版权、相机、镜头 | `set` |
| 添加或删除 GPS 坐标 | `gps` |
| 分享前清除元数据 | `strip` |
| 不重新编码地摆正躺倒的照片 | `rotate` |
| 把一张照片的元数据复制给一批 | `copy` |
| 按拍摄时间重命名文件 | `rename` |
| 编辑标题、评分、关键词 | `xmp` |
| 编辑新闻 / 图库字段 | `iptc` |
| 撤销一次修改 | `restore` |
| 用记录的轨迹补 GPS | `geotag` |
| 从表格批量导入修改 | `apply` |
| 统计整个图库 | `report` |
| 检查图库有没有元数据问题 | `verify` |
| 装上 shell 的 Tab 补全 | `completions` |

> PIC-Killer 不做格式转换、不缩放、不裁剪、不重新压缩，只重写元数据。遇到根本没地方存元数据的格式（BMP、GIF），它会明确告诉你并跳过，而不是把文件弄坏。

---

## 2. 安装

最省事的是下预编译二进制。到 [Releases 页](https://github.com/adomore/pic-killer/releases/latest)下载对应平台的压缩包解压，里面就是一个可执行文件，没有任何运行时依赖。

每次发布都附带 `SHA256SUMS.txt`，可以校验下载完整性：

```bash
sha256sum -c SHA256SUMS.txt
```

想从源码构建，需要一套支持 edition 2024 的 Rust 工具链：

```bash
git clone https://github.com/adomore/pic-killer.git
cd pic-killer
cargo build --release
```

在对真实照片动手之前，先确认二进制能跑：

```bash
pic-killer --version
pic-killer --help
```

> 旧版 Windows 控制台里中文可能显示成乱码。执行一次 `chcp 65001` 切到 UTF-8 即可。

---

## 3. 动手之前先看一眼

每次开工都该从 `show` 开始。它是只读的，弄不坏任何东西。

```powershell
pic-killer show .\photo.jpg
```

真实输出长这样：

```
=== C:\photos\beach.jpg ===
  位置：30.274100, 120.155100，海拔 12.0m
  Make              Canon
  Model             EOS R5
  DateTimeOriginal  2023:06:15 18:05:00
  CreateDate        2023:06:15 18:05:00
  GPSLatitudeRef    N
  GPSLatitude       30, 16, 2676/100 (26.7600)
  --- XMP ---
  dc:title          West Lake
  xmp:Rating        5
  --- IPTC ---
  City              Hangzhou
```

从上往下读：照片若含 GPS，第一行是换算好的十进制坐标；接着是 EXIF 标签；如果存在，再依次是 `--- XMP ---` 段和 `--- IPTC ---` 段。文件完全没有元数据时会显示 `(无匹配的元数据)`。

想让长列表短一点，或者想摸一遍整个目录而不是单张：

```powershell
pic-killer show .\photo.jpg --filter gps
pic-killer report .\photos -r
```

`report` 回答的是「我手上这批到底是什么货色」——多少张有拍摄时间、多少张有 GPS、时间跨度多长、都是什么相机拍的。

---

## 4. 三层安全网

PIC-Killer 有三层互相独立的保护。第一次写入之前先把它们弄明白。

**第一层 —— 预览。** `-n`（或 `--dry-run`）把将要发生的改动完整打出来，一个字节都不写：

```powershell
pic-killer set .\photos -r --artist "Zhang San" -n
```

```
PIC-Killer · 设置标签
  设置：Artist
  文件：3 个
  模式：预览（不写入）

[OK]   C:\photos\a.jpg
[OK]   C:\photos\b.jpg
[OK]   C:\photos\c.jpg

将修改 3，共 3 个文件。
（预览模式，未写入任何文件；去掉 -n/--dry-run 即可执行）
```

**第二层 —— 备份与撤销。** `--backup` 在动手之前把每个文件复制一份到 `<文件名>.bak`，`restore` 再把它字节级地还原回去：

```powershell
pic-killer set .\photo.jpg --artist "Zhang San" --backup
pic-killer restore .\photo.jpg
```

**第三层 —— 原子写入。** 每次写入都先写到同目录下的临时文件，刷盘之后再原子地替换原件。批处理中途被打断，也不可能留下一个写了一半的照片。除非你另行要求，原文件的修改时间会被保留。

> `--backup` 有两点会让人意外。第一，已存在的 `.bak` **绝不会**被覆盖 —— 所以带 `--backup` 改两次之后，`restore` 把你带回的是**最初的原件**，而不是第一次修改后的状态。第二，`restore` 还原完会把 `.bak` 删掉，除非加 `--keep-backup`。

写入类命令在没有 `-y` 时会要求确认。脚本和管道里没人能回答，所以命令会**直接报错**而不是发问 —— **自动化场景一律加 `-y`**，或者用 `-n` 预览。

---

## 5. 五个真实任务

### 5.1 拍摄时间不对

三种不同的问题，对应三个不同的选项。相机时钟差了固定值用 `--shift`，知道真实时间用 `--set`，文件名里本来就带日期用 `--from-name`。

```powershell
pic-killer time .\photos -r --shift "+2h"
pic-killer time .\photo.jpg --set "2024-01-01 12:00:00"
pic-killer time .\photos --from-name
```

偏移单位可以自由组合：`y` 年、`mo` 月、`w` 周、`d` 天、`h` 时、`m` 分、`s` 秒。注意 `m` 是**分钟**、`mo` 才是**月** —— 这是最常见的一个错。

```powershell
pic-killer time .\photos --shift "-1d12h30m"
```

### 5.2 给整场拍摄盖上作者和版权

```powershell
pic-killer set .\shoot -r --artist "Zhang San" --copyright "(C) 2024 Zhang San"
```

同一条命令也能设相机和镜头字段，这对扫描件和胶片很有用 —— 那些照片相机根本没写过任何信息：

```powershell
pic-killer set .\scans -r --make "Nikon" --model "FM2" --lens-model "50mm f/1.4"
```

### 5.3 分享前抹掉 GPS

`strip --gps` 只删定位，其余全部保留。不带参数的 `strip` 清除全部元数据。

```powershell
pic-killer strip .\to-publish -r --gps
pic-killer strip .\to-publish -r
```

上传之前先确认它真的没了：

```powershell
pic-killer show .\to-publish -r --filter gps
```

### 5.4 用记录的轨迹补 GPS

如果你用手机或手表录了 GPX 轨迹，PIC-Killer 可以按每张照片的拍摄时刻在轨迹上插值出位置。

```powershell
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00
```

这里最要紧的是 `--tz`。拍摄时间是不带时区的本地墙上时间，GPX 时间戳是 UTC。你得告诉 PIC-Killer 相机时钟当时在哪个时区，否则每张照片都会落到轨迹上的错误位置。没有拍摄时间的、或落在超过 `--max-gap`（默认 600 秒）空档里的照片会被跳过，而不是硬猜。

### 5.5 按拍摄时间重命名

```powershell
pic-killer rename .\photos -r -n
pic-killer rename .\photos -r
```

默认模板产出 `20230115_143022.jpg`。没有拍摄时间的文件会被跳过；如果两张照片会算出同一个名字，第二张变成 `20230115_143022_1.jpg`。

---

## 6. 选择要处理的文件

除 `apply` 和 `completions` 之外，每个命令都接受一个或多个路径。路径可以是文件、目录或通配符。

```powershell
pic-killer show .\photo.jpg
pic-killer show .\photos
pic-killer show .\photos -r
pic-killer show ".\photos\IMG_*.jpg"
```

| 形式 | 含义 |
|------|------|
| 单个文件 | 一定会处理，哪怕它的扩展名不在 `--ext` 里 |
| 目录 | 默认只处理一层，加 `-r` 递归 |
| 通配符 | 由 PIC-Killer 自己展开，单层目录，支持 `*` 和 `?` |
| 多个路径 | 合并、去重、排序 |

从目录里只会捞出扩展名在 `--ext` 中的文件。默认列表是 `jpg,jpeg,png,tif,tiff,webp`，需要时自己放宽：

```powershell
pic-killer show .\photos -r --ext jpg,jpeg,heic,avif
```

> Windows 上记得给通配符加引号。PowerShell 和 `cmd` 不会替外部程序展开 `*`，所以 PIC-Killer 自己展开 —— 但前提是 shell 把没展开的模式原样交过来。

---

## 7. 只处理需要处理的文件

`--where` 按元数据筛选，于是你可以说「只处理缺 GPS 的那些」，而不用手工挑。所有接受路径的命令都支持它。

```powershell
pic-killer show .\photos -r --where no-gps
pic-killer time .\photos -r --where no-date --from-name
pic-killer set .\photos -r --where make=Canon --artist "Zhang San"
```

| 条件 | 匹配 |
|------|------|
| `has-gps` / `no-gps` | 有 / 没有 GPS 坐标的照片 |
| `has-date` / `no-date` | 有 / 没有拍摄时间的照片 |
| `has-xmp` / `no-xmp` | 有 / 没有 XMP 包的照片 |
| `has:名称` / `no:名称` | 某个具名标签存在 / 不存在，跨 EXIF、XMP、IPTC 查找 |
| `名称=值` / `名称!=值` | 标签值等于 / 不等于 |
| `名称~值` / `名称!~值` | 标签值包含 / 不包含 |

用 `&&`（都要满足）或 `||`（满足任一）组合条件：

```powershell
pic-killer show .\photos -r --where "no-gps && make=Canon"
pic-killer show .\photos -r --where "make=Canon || make=Nikon"
```

> **不要在一个表达式里混用 `&&` 和 `||`。** 解析器只会按其中一种切分，因此同时含两者的表达式会被直接报错拒绝。请拆成两条命令跑。

> **用 `!=` 或 `!~` 之前先检查拼写。** 这两个是取反语义，一个匹配不到任何标签的名称本会匹配*每一个*文件。PIC-Killer 会拒绝这种筛选 —— `camera!=Canon` 会中止，因为根本没有叫 *camera* 的标签。而真实存在、只是某些文件没有的名称是正常的：`make!=Canon` 会匹配其它厂商以及没有厂商信息的照片。即便如此，跑破坏性批处理前先用同样的 `--where` 过一遍 `show` 确认数量，也不费什么事。

---

## 8. 读懂输出

写入类命令会打印一个抬头、每个文件一行、最后一段汇总。

```
[OK]   C:\photos\a.jpg
[跳过] C:\photos\b.jpg  (无可解析的原始拍摄时间，偏移模式跳过)
[失败] C:\photos\c.jpg  (不支持的文件类型)

已修改 1，跳过 1，失败 1，共 3 个文件。
```

`[OK]` 表示已修改，`[跳过]` 表示有意跳过、括号里是原因，`[失败]` 表示这个文件处理不了。跳过是正常且安全的，通常意味着该文件不满足这条命令的前提条件。退出码告诉脚本发生了什么：

| 退出码 | 含义 |
|--------|------|
| `0` | 全部成功，或本来就没什么要做的 |
| `1` | 命令本身失败 —— 参数不对、GPX 读不了、`--where` 解析不了 |
| `2` | 命令跑完了，但至少有一个文件失败（`verify` 则表示至少查出一个问题） |

结果走标准输出；`--where` 的筛选提示和进度条走标准错误。所以 `show --json` 和 `show --csv` 可以直接重定向到文件而不会被污染。

---

## 9. 常见问题排查

| 现象 | 原因 | 处理 |
|------|------|------|
| 控制台里中文是乱码 | 控制台不是 UTF-8 | 执行 `chcp 65001` |
| `未找到符合条件的图片文件。` | 路径、扩展名过滤或 `--where` 没匹配到任何文件 | 检查路径拼写；放宽 `--ext`；先用 `show` 试筛选条件 |
| `错误：以下路径不存在` | 你写的某个路径在磁盘上不存在 | 检查拼写。命令在动任何文件之前就停了，不会处理到一半 |
| 命令打印了 `已取消。` 什么也没做 | 没加 `-y`，又没有终端可以回答确认 | 加上 `-y` |
| `BMP 无元数据容器，建议先转成 PNG 再处理` | BMP 和 GIF 存不了元数据 | 先转成 PNG 或 JPEG |
| RAW 文件（CR2、NEF、ARW）写不进去 | EXIF 引擎不解析 RAW 容器 | 用 `pic-killer xmp <文件> --sidecar …`，它写独立的 `.xmp`，完全不碰 RAW |

---

## 10. 下一步

[用户手册](USER-MANUAL.zh.md)记录了每一个命令、每一个选项、完整的 `--where` 文法、元数据字段表、CSV 导入格式，以及确切的安全保证。

有两件事值得一次性配好：

```bash
pic-killer completions powershell | Out-String | Invoke-Expression
pic-killer completions --man > pic-killer.1
```

还有一个值得保持的习惯：任何没跑过的批处理，都分三步走 —— 先 `show --where …` 确认选中的文件，再用同样的条件加 `-n`，最后带 `--backup` 真跑。
