# PIC-Killer

[![CI](https://github.com/adomore/pic-killer/actions/workflows/ci.yml/badge.svg)](https://github.com/adomore/pic-killer/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/adomore/pic-killer?logo=github)](https://github.com/adomore/pic-killer/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/adomore/pic-killer/total?logo=github)](https://github.com/adomore/pic-killer/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**照片元数据瑞士军刀** —— 用 Rust 实现的命令行工具。

无损批量修改照片的 EXIF 元数据：拍摄时间、作者版权、相机镜头、GPS 定位、方向，以及查看与清除。

只改写元数据段，**不重新编码图像**，照片的压缩像素数据逐比特不变，真正无损。

支持格式：JPEG / PNG / TIFF / WebP / HEIC / AVIF / JXL。

---

## 功能一览

| 子命令 | 作用 |
|--------|------|
| [`time`](#time--修改拍摄时间) | 修改拍摄时间：固定值 / 相对偏移 / 序列递增 / 从文件名提取 |
| [`show`](#show--查看元数据) | 查看或导出元数据（表格 / JSON / CSV） |
| [`set`](#set--设置标签) | 设置常见标签：作者、版权、描述、相机、镜头、方向等 |
| [`gps`](#gps--定位) | 设置或清除 GPS 定位 |
| [`strip`](#strip--清除元数据) | 清除元数据保护隐私：全部 / 仅 GPS |
| [`rotate`](#rotate--无损旋转) | 无损旋转标记：在现有方向上叠加旋转/镜像 |
| [`copy`](#copy--复制元数据) | 从一张参考照片复制元数据到一批照片 |
| [`rename`](#rename--按时间重命名) | 按拍摄时间批量重命名文件（`--from-name` 的逆操作） |
| [`xmp`](#xmp--读写-xmp) | 读写 XMP：标题/描述/作者/评分/关键词/城市等（JPEG 与 PNG） |
| [`iptc`](#iptc--读写-iptc-iim) | 读写旧版 IPTC-IIM：标题/说明/关键词/作者/城市/版权等（JPEG） |
| [`restore`](#restore--从备份还原) | 从 `.bak` 备份还原文件，撤销之前的修改 |

## 下载安装

### 预编译二进制（推荐）

前往 **[Releases 页](https://github.com/adomore/pic-killer/releases/latest)** 下载对应平台的压缩包，解压即得单个可执行文件：

| 平台 | 下载文件 |
|------|----------|
| Windows x64 | `pic-killer-<版本>-x86_64-pc-windows-msvc.zip` |
| macOS (Apple Silicon) | `pic-killer-<版本>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `pic-killer-<版本>-x86_64-apple-darwin.tar.gz` |
| Linux x64 | `pic-killer-<版本>-x86_64-unknown-linux-gnu.tar.gz`（静态版：`…-musl`） |
| Linux ARM64 | `pic-killer-<版本>-aarch64-unknown-linux-gnu.tar.gz` |

每次发布都附带 `SHA256SUMS.txt`，可校验下载完整性：

```bash
sha256sum -c SHA256SUMS.txt
```

### 从源码构建

需要 Rust 1.88+（edition 2024）：

```bash
git clone https://github.com/adomore/pic-killer.git
cd pic-killer
cargo build --release          # 产物：target/release/pic-killer
cargo install --path .         # 或直接安装到 PATH
```

## 通用说明

所有子命令都接受一个或多个**文件或目录**作为处理对象：

- `-r, --recursive` 递归子目录（目录默认只处理一层）
- `--ext <列表>` 指定处理的扩展名（默认 `jpg,jpeg,png,tif,tiff,webp`）

会写入的子命令（`time`/`set`/`gps`/`strip`）还支持：

- `-n, --dry-run` 仅预览，不写入
- `--backup` 处理前备份为 `<文件名>.bak`
- `-y, --yes` 跳过确认提示
- `-v, --verbose` 详细输出

---

## `time` · 修改拍摄时间

```powershell
# 设为固定时间
pic-killer time .\photos --set "2024-01-01 12:00:00"

# 相对偏移（修相机时钟/时区）——支持 +1y2mo3d4h5m6s 任意组合
pic-killer time .\photos --shift "+2h" -r
pic-killer time .\photos --shift "-3d"

# 序列递增（强制排序），首张起每张 +1 分钟
pic-killer time .\photos --sequential "2021-01-01 08:00:00" --interval "+1m"

# 从文件名提取日期（IMG_20230115_143022.jpg、2022-07-04 09.15.00.jpg 等）
pic-killer time .\photos --from-name
```

偏移单位（大小写不敏感、可组合）：`y`年 `mo`月 `w`周 `d`天 `h`时 `m`分 `s`秒。
注意 `m` 是**分钟**、`mo` 是**月**；月/年按日历运算。

其它选项：
- `--tags <列表>` 写入哪些时间字段：`original,digitized,modify`（默认全部）
- `--also-file-time` 同时把文件系统修改时间设为该拍摄时间

## `show` · 查看元数据

```powershell
# 人类可读表格
pic-killer show .\photo.jpg

# 只看名称含 gps 的标签
pic-killer show .\photo.jpg --filter gps

# 导出为 JSON 或 CSV（便于脚本 / 表格处理）
pic-killer show .\photos -r --json > meta.json
pic-killer show .\photos -r --csv  > meta.csv
```

若含 GPS，会额外打印一行十进制经纬度和海拔。

## `set` · 设置标签

```powershell
pic-killer set .\photos --artist "张三" --copyright "© 2024 张三"
pic-killer set .\photos --make "Canon" --model "EOS R5" --lens-model "RF 24-70"
pic-killer set .\photo.jpg --description "海边日落" --user-comment "备注"

# 方向（无损旋转标记）：normal / cw90 / ccw90 / 180 / mirror-h / mirror-v 或 1-8
pic-killer set .\photo.jpg --orientation cw90

# 通用设置任意字符串标签（可重复）
pic-killer set .\photo.jpg --set-string "OwnerName=Zhang" --set-string "Software=PicKiller"

# 删除指定标签（可重复）
pic-killer set .\photo.jpg --remove artist --remove copyright
```

常用标签的专用选项：`--artist` `--copyright` `--description` `--software`
`--make` `--model` `--lens-model` `--user-comment` `--owner` `--orientation`。

## `gps` · 定位

```powershell
# 设置坐标（十进制度：北纬/东经为正，南纬/西经为负），海拔可选
pic-killer gps .\photo.jpg --lat 39.9042 --lon 116.4074 --alt 50

# 清除 GPS
pic-killer gps .\photos -r --clear
```

## `strip` · 清除元数据

```powershell
# 清除全部元数据（保护隐私）
pic-killer strip .\photos -r

# 只清除 GPS 定位，保留其它信息
pic-killer strip .\photos -r --gps
```

## `rotate` · 无损旋转

在照片**现有**方向基础上叠加旋转/镜像（会与已有方向正确复合，而非简单覆盖）。
这是无损的：只改 EXIF 方向标记，不动像素。

```powershell
pic-killer rotate .\photo.jpg --cw        # 顺时针 90°
pic-killer rotate .\photo.jpg --ccw       # 逆时针 90°
pic-killer rotate .\photo.jpg --r180      # 旋转 180°
pic-killer rotate .\photo.jpg --flip-h    # 水平镜像
pic-killer rotate .\photo.jpg --flip-v    # 垂直镜像
pic-killer rotate .\photos -r --reset     # 重置为正常
```

> `set --orientation` 是**绝对**设定方向码；`rotate` 是在当前方向上**相对**叠加，
> 例如对已顺时针 90° 的照片再 `--cw` 会变成 180°。

## `copy` · 复制元数据

从一张参考照片把元数据复制到一批照片（如整组连拍统一时间/地点）。

```powershell
# 默认复制全部可复制的元数据（自动跳过尺寸/方向等与具体图像绑定的字段）
pic-killer copy .\burst\*.jpg --from .\reference.jpg

# 只复制拍摄时间，或只复制 GPS
pic-killer copy .\photos -r --from .\ref.jpg --time
pic-killer copy .\photos -r --from .\ref.jpg --gps
```

## `rename` · 按时间重命名

按拍摄时间批量重命名（`time --from-name` 的逆操作）。同名自动加序号，无拍摄时间则跳过。

```powershell
# 默认模板 %Y%m%d_%H%M%S → 20230115_143022.jpg
pic-killer rename .\photos -r

# 自定义模板（strftime 语法，不含扩展名）
pic-killer rename .\photos --pattern "%Y-%m-%d_%H.%M.%S"

# 先预览
pic-killer rename .\photos -r --dry-run
```

## `xmp` · 读写 XMP

读写 XMP 元数据——相机、Lightroom、手机等常把标题、评分、关键词、版权存在这里
（现代 **IPTC Core** 也是基于 XMP 的）。**支持 JPEG 与 PNG**（PNG 存于 iTXt chunk）。

XMP 是独立于 EXIF 的元数据块，两者互不影响；写 XMP 会**保留所有未识别的既有属性**，
只增改指定的那几个。

```powershell
# 设置标题/描述/作者/评分/关键词
pic-killer xmp .\photo.jpg --title "西湖日出" --description "清晨的西湖" `
  --creator "张三" --creator "李四" --rating 5 --keywords "风景,西湖,日出"

# 城市/国家、颜色标签
pic-killer xmp .\photo.jpg --city 杭州 --country 中国 --label 红色

# 通用设置任意属性（前缀:名称=值），删除属性
pic-killer xmp .\photo.jpg --set "photoshop:Headline=头条" --remove dc:description

# 清除整个 XMP 包
pic-killer xmp .\photos -r --clear
```

专用选项：`--title` `--description` `--creator`(可重复) `--rights` `--rating`(0-5)
`--label` `--keywords`(逗号分隔) `--city` `--country`。查看用 `show`（会额外列出 XMP 段）。

## `iptc` · 读写 IPTC-IIM

读写旧版 **IPTC-IIM** 元数据（存于 JPEG 的 APP13 / Photoshop 8BIM 块，新闻/图库工作流常用）。
**仅支持 JPEG**。写入以 UTF-8 编码并保留其它 8BIM 资源块（缩略图、色彩配置等）。

```powershell
# 设置标题/说明/关键词/作者/城市/版权
pic-killer iptc .\photo.jpg --title "开幕式" --description "现场" `
  --keywords "体育,开幕" --creator "记者甲" --city 北京 --copyright "© 新华社"

# 通用设置（字段名或 记录:数据集）、删除、清除
pic-killer iptc .\photo.jpg --set "2:105=头条" --remove keywords
pic-killer iptc .\photos -r --clear
```

专用选项：`--title` `--description` `--keywords` `--creator`(可重复) `--headline`
`--city` `--state` `--country` `--copyright` `--credit` `--source` `--instructions`。

> EXIF、XMP、IPTC 三套元数据彼此独立，可并存于同一张 JPEG，本工具保证互不破坏。

## `restore` · 从备份还原

与 `--backup` 配套：把之前用 `--backup` 生成的 `<文件名>.bak` 还原回去，一键撤销修改。

```powershell
# 先带备份修改
pic-killer set .\photo.jpg --artist 张三 --backup

# 反悔了，一键还原（默认还原后移除 .bak）
pic-killer restore .\photo.jpg

# 保留 .bak 以便反复还原
pic-killer restore .\photos -r --keep-backup

# 先预览哪些文件有备份可还原
pic-killer restore .\photos -r --dry-run
```

没有 `.bak` 的文件会被跳过；还原是字节级的（`.bak` 就是原件的完整副本）。

---

## 无损原理

JPEG 由若干「段」组成：EXIF 信息放在 APP1 段里，真正的图像像素是独立的压缩扫描数据。
本工具只删除旧的元数据段、插入新的，**完全不触碰扫描数据**，因此不存在重新压缩、
不会有任何画质损失。用任意图像库解码改前 / 改后的照片，得到的像素完全一致。

## 安全设计

- **原子写入**：先写临时文件，`fsync` 后再原子重命名覆盖，批量处理中途中断也不会损坏原文件。
- **默认保护文件时间戳**：写入会新建文件，但默认恢复原文件系统 mtime/atime，除非 `--also-file-time`。
- **可预览可备份**：`--dry-run` 先看再做，`--backup` 留底。

## 已知限制

- **元数据往返**：底层 EXIF 库（`little_exif`）识别数十个常见标签（时间、Make、Model、
  Artist、GPS、光圈快门等），这些都会完整保留；但极少数它不认识的冷门标签（部分厂商
  MakerNote）在重写时可能丢失。图像像素永远无损。
- **HEIC / AVIF / JXL**：底层库支持，但成熟度不及 JPEG，建议先用 `--backup` 或 `--dry-run` 试跑。
- **WebP**：仅支持无损与扩展格式的 WebP。
- **XMP**：支持 JPEG 与 PNG；TIFF/WebP/HEIC 的 XMP 尚未做。
- **IPTC-IIM**：支持 JPEG（APP13）；写入按 UTF-8 编码。
- **缩略图替换**暂未支持（涉及 IFD1 数据偏移，风险较高）。
- 旧版 Windows 控制台里中文若显示为乱码，先执行 `chcp 65001` 切到 UTF-8。

## 许可

MIT
