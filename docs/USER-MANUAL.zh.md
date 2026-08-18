# PIC-Killer · 用户手册

> 每一个命令、选项、字段名与保证的完整参考。没用过 PIC-Killer 请先读[新手入门手册](GETTING-STARTED.zh.md)。

[English](USER-MANUAL.md) | **中文**

---

## 1. 关于本手册

本手册对应 PIC-Killer 1.0.0。所有内容都对着源码和实际运行的二进制核对过；行为反直觉的地方，按它实际怎样写，而不是按它应该怎样写。

交叉引用一律用章节号（§5、§8.3）而不是链接，因为中英两版的章节号完全一致。

全文约定：

- 示例路径用 PowerShell 风格（`.\photos`）。Linux 与 macOS 上写成 `./photos`。
- 语法说明里的 `<角括号>` 表示你要填的值，`[方括号]` 表示可选。
- 「写入类命令」指会修改图片文件的十个命令：`time`、`set`、`gps`、`strip`、`rotate`、`copy`、`xmp`、`iptc`、`geotag`、`apply`。

---

## 2. 核心概念

**无损编辑。** JPEG 是一串段的序列，EXIF 存在 APP1 段里，图像本身是独立的一块压缩扫描数据。PIC-Killer 删掉旧的元数据段、插入新的，扫描数据从头到尾没被读过、解过、也没重编过，所以不存在代际损失。改前改后各解码一次，像素逐比特相同。

**三套元数据体系。** 一个 JPEG 可以同时携带 EXIF、XMP 和 IPTC-IIM。它们存在不同的段里，描述的东西有重叠但并不相同。PIC-Killer 用各自的命令来编辑（EXIF 用 `set`/`time`/`gps`，XMP 用 `xmp`，IPTC 用 `iptc`），写其中一套时绝不破坏另外两套。

**原子写入。** 没有任何命令是就地改文件的。每次写入都先生成一个完整的新文件，再原子地替换原件。详见 §13。

**宁可跳过，不去猜。** 当文件不满足命令的前提条件时 —— 没有可偏移的拍摄时间、文件名里没有日期、附近没有轨迹点 —— PIC-Killer 跳过它并说明原因，绝不编造一个值。

---

## 3. 安装

[Releases 页](https://github.com/adomore/pic-killer/releases/latest)提供六个目标平台的预编译二进制。每个压缩包里是一个自包含的可执行文件，外加 README 与许可证。

| 平台 | 压缩包 |
|------|--------|
| Windows x64 | `pic-killer-<版本>-x86_64-pc-windows-msvc.zip` |
| macOS（Apple Silicon） | `pic-killer-<版本>-aarch64-apple-darwin.tar.gz` |
| macOS（Intel） | `pic-killer-<版本>-x86_64-apple-darwin.tar.gz` |
| Linux x64（glibc） | `pic-killer-<版本>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux x64（静态） | `pic-killer-<版本>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `pic-killer-<版本>-aarch64-unknown-linux-gnu.tar.gz` |

先用发布的校验和验证下载，确有需要再从源码构建：

```bash
sha256sum -c SHA256SUMS.txt
git clone https://github.com/adomore/pic-killer.git
cd pic-killer
cargo build --release
cargo install --path .
```

crate 声明了 `edition = "2024"`，需要 Rust 1.85 或更新的版本。

---

## 4. 命令行结构

```
pic-killer <command> [options] <paths...>
```

每个命令都有自己的选项；此外按类别不同，还接受 §5 的文件选择选项和 §7 的写入选项。

| 类别 | 命令 | 接受 §5 | 接受 §7 |
|------|------|---------|---------|
| 写入 | `time` `set` `gps` `strip` `rotate` `copy` `xmp` `iptc` `geotag` | 是 | 是 |
| 写入，不吃路径 | `apply` | 否 | 是 |
| 只读 | `show` `report` `verify` | 是 | 否 |
| 文件操作 | `rename` `restore` | 是 | 部分 —— 只有 `-n`、`-y`、`-v` |
| 辅助 | `completions` | 否 | 否 |

---

## 5. 选择文件

以下选项被所有接受路径的命令支持。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `<paths...>` | 文件、目录或通配符 | — | 必填，可给多个 |
| `-r`, `--recursive` | — | 关 | 递归进入子目录 |
| `--ext` | 逗号分隔列表 | `jpg,jpeg,png,tif,tiff,webp` | 从目录里收取哪些扩展名 |
| `--where` | 表达式 | — | 按元数据筛选，见 §6 |

选择规则：

- 指向**文件**的路径一定会被处理，哪怕它的扩展名不在 `--ext` 里。明确点名一个文件就视为意图明确。
- 指向**目录**的路径默认展开一层，加 `-r` 则完全递归。只收取扩展名在 `--ext` 中的文件。
- 含 `*` 或 `?` 的路径由 PIC-Killer 自己展开，因为 `cmd` 与 PowerShell 不会替外部程序展开通配符。展开只覆盖单层目录，并且忽略 `--ext`，因为模式本身就是筛选条件。
- 所有路径的结果合并、去重，并按**自然顺序**排序：连续数字按数值比较，所以 `IMG_2.jpg` 排在 `IMG_10.jpg` 前面。正是这个排序让 `time --sequential` 既可复现、对编号照片又是对的。
- 你明确写出的路径若不存在，会直接报错而不是静默跳过：命令在动任何文件之前就停下，并列出这些路径。通配符匹配为空不算错误，因为模式匹配到零个文件是正常的。

```powershell
pic-killer show .\photo.jpg
pic-killer show .\photos -r --ext jpg,jpeg,heic
pic-killer show ".\photos\IMG_*.jpg"
pic-killer show .\a.jpg .\b.jpg .\more-photos
```

---

## 6. `--where` 条件筛选

`--where` 把命令限制在元数据满足条件的文件上。标签查找横跨 EXIF、XMP 和 IPTC。名称匹配不区分大小写，且按**子串**匹配，所以 `make` 既匹配 `Make` 也匹配 `LensMake`。

| 语法 | 匹配 |
|------|------|
| `has-gps` / `no-gps` | 有 / 没有 GPS 坐标 |
| `has-date` / `no-date` | 有 / 没有拍摄时间 |
| `has-xmp` / `no-xmp` | 有 / 没有 XMP 包 |
| `has:名称` / `no:名称` | 名称含该关键字的标签存在 / 不存在 |
| `名称=值` | 某个匹配到的标签值等于该值 |
| `名称!=值` | 没有任何匹配到的标签值等于该值 |
| `名称~值` | 某个匹配到的标签值包含该值 |
| `名称!~值` | 没有任何匹配到的标签值包含该值 |

条件用 `&&`（都要满足）或 `||`（满足任一）组合：

```powershell
pic-killer show .\photos -r --where "no-gps && make=Canon"
pic-killer show .\photos -r --where "make=Canon || make=Nikon"
pic-killer geotag .\photos -r --gpx .\t.gpx --tz +08:00 --where no-gps
```

> **`&&` 与 `||` 不能混用。** 解析器只按其中一种运算符切分，因此同时含两者的表达式会被拒绝，并指出是哪一半有问题。请拆成两条命令。早期版本会接受这类表达式并静默返回错误的文件集合。

> **对不存在的标签名做负向比较会被拒绝。** `!=`、`!~` 与 `no:` 都是取反语义，一个匹配不到任何标签的名称会对每个文件都为真 —— `--where "camera!=Canon"` 本会选中全部文件，因为根本没有叫 *camera* 的标签。筛选之前，PIC-Killer 会拿这类名称在整个待处理集合里核对一遍，一个都匹配不到就中止并指出可疑的名称。而「名称存在、只是这个文件没有」仍按常规处理：`make!=Canon` 既匹配厂商不同的文件，也匹配根本没有 Make 的文件。

---

## 7. 写入类命令的通用选项

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--backup` | — | 关 | 写入前把每个文件复制为 `<文件名>.bak` |
| `-n`, `--dry-run` | — | 关 | 报告将要发生的改动，不写入 |
| `-y`, `--yes` | — | 关 | 跳过确认提示 |
| `-v`, `--verbose` | — | 关 | 批处理开跑前打印一段设置摘要 |
| `-j`, `--jobs` | 整数 | `0` | 工作线程数：`0` = 每个 CPU 核一个，`1` = 顺序 |

逐条说明：

- `--backup` 绝不覆盖已存在的 `.bak`。带备份改过两次之后，`restore` 把文件还原到的是**最初的原件**，而不是第一次修改后的状态。这是有意为之 —— 第一份备份才是唯一保证没被 PIC-Killer 动过的。
- `-n` 在写入路径的内部短路，位置在所有解析和元数据读取之后，所以预览跑的是和真实执行同一套代码，报告出的跳过与失败也一致。
- 不加 `-y` 时，写入类命令会从标准输入读取确认，提示写到标准错误，因此重定向标准输出时仍然看得见。**标准输入不是终端时——脚本、管道、CI——命令直接报错而不是发问。** 自动化场景请加 `-y`，或用 `-n` 预览。早期版本会读到输入结束、当成「否」、以 0 退出且什么都没改，让坏掉的自动化看起来是成功的。
- `-j` 在取值不是 `0` 和 `1` 时会另建一个专用线程池。因为每个文件各写各的临时文件，并行写入是安全的。
- `rename` 与 `restore` 接受 `-n`、`-y`、`-v`，但不接受 `--backup` 和 `-j`。

---

## 8. 命令参考

### 8.1 `time` —— 修改拍摄时间

设定、偏移、序列化或从文件名推导拍摄时间。

```
pic-killer time [options] <--set <time>|--shift <delta>|--sequential <start>|--from-name> <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--set` | 日期时间 | — | 设为绝对时间，如 `"2024-01-01 12:00:00"` |
| `--shift` | 偏移量 | — | 在原时间上偏移，如 `+2h`、`-3d`、`+1y2mo` |
| `--sequential` | 日期时间 | — | 首张设为该时间，其余按 `--interval` 递增 |
| `--interval` | 偏移量 | `+1s` | 序列模式下每张之间的步长 |
| `--from-name` | — | 关 | 从文件名里解析日期 |
| `--tags` | 逗号分隔 | `original,digitized,modify` | 写入哪些时间字段 |
| `--tz` | 时区偏移 | — | 一并写入 `OffsetTime*` 标签，如 `+08:00` |
| `--also-file-time` | — | 关 | 同时设置文件系统修改时间 |

**行为**

- `--set`、`--shift`、`--sequential`、`--from-name` 必须且只能选一个，互斥。
- `--tags` 到 EXIF 的映射：`original` → `DateTimeOriginal`，`digitized` → `CreateDate`，`modify` → `ModifyDate`。
- `--shift` 会跳过没有可解析拍摄时间的文件 —— 没有基准就无从偏移。
- `--from-name` 会跳过文件名里认不出日期的文件。只有日期没有时间的，按零点处理。
- `--sequential` 按 §5 的自然排序顺序遍历文件，所以编号照片会按人眼预期的顺序递增。
- 偏移单位为 `y` 年、`mo` 月、`w` 周、`d` 天、`h` 时、`m` 分、`s` 秒，见 §9.6。月与年按日历运算。

**示例**

```powershell
pic-killer time .\photos --set "2024-01-01 12:00:00"
pic-killer time .\photos -r --shift "+2h"
pic-killer time .\photos --shift "-1d12h30m"
pic-killer time .\photos --sequential "2021-01-01 08:00:00" --interval "+1m"
pic-killer time .\photos --from-name --where no-date
pic-killer time .\photo.jpg --set "2023-06-15 18:00:00" --tz +08:00 --also-file-time
```

> `m` 是分钟，`mo` 才是月。`--shift "+6m"` 让照片往后走六分钟，不是六个月。

### 8.2 `show` —— 查看元数据

只读。打印或导出 PIC-Killer 能读到的一切。

```
pic-killer show [options] <paths...>
```

**选项** —— 另可用 §5。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--json` | — | 关 | 输出 JSON，与 `--csv` 冲突 |
| `--csv` | — | 关 | 输出 CSV，每个标签一行 |
| `--for-apply` | — | 关 | 配合 `--csv`，改为输出 `apply` 能读的三列，而不是完整转储 |
| `--filter` | 关键字 | — | 只显示名称含该关键字的标签，不区分大小写 |

**行为**

- 默认输出是每个文件一段表格：若有 GPS 先给一行换算好的十进制坐标，然后是 EXIF 标签，然后是 `--- XMP ---` 段，然后是 `--- IPTC ---` 段。
- 什么都读不到的文件打印 `(无匹配的元数据)`。
- 零匹配时 `--json` 输出 `[]`，`--csv` 只输出表头；给人看的提示走标准错误，保证输出始终可被解析。
- 单用 `--csv` 是**转储**格式 —— 它带着 IFD 分组与十六进制码，还把 GPS 拆成了分量标签，因此喂不回 `apply`。要做闭环请用 `--csv --for-apply`，见 §10。
- 因为转储就是拿去表格软件打开的，以 `=`、`+`、`-`、`@` 或制表符开头的值会被加上一个前导单引号，免得被当成公式求值。`--for-apply` **有意不做**这件事：它是交换格式，而南半球的 GPS 值本来就以 `-` 开头。
- 旁挂的 `.xmp` 会被自动读取并显示，包括那些自身容器解析不了的文件。
- `--filter` 会收窄三套元数据各自的标签列表。那行换算后的 GPS 坐标**不**受筛选影响 —— 只要文件有坐标就会打印。
- 输出格式的规格见 §11。

**示例**

```powershell
pic-killer show .\photo.jpg
pic-killer show .\photo.jpg --filter gps
pic-killer show .\photos -r --json > meta.json
pic-killer show .\photos -r --csv  > meta.csv
pic-killer show .\photos -r --where no-gps
```

### 8.3 `set` —— 设置常见标签

```
pic-killer set [options] <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--artist` | 文本 | — | `Artist` |
| `--copyright` | 文本 | — | `Copyright` |
| `--description` | 文本 | — | `ImageDescription` |
| `--software` | 文本 | — | `Software` |
| `--make` | 文本 | — | `Make` |
| `--model` | 文本 | — | `Model` |
| `--lens-model` | 文本 | — | `LensModel` |
| `--user-comment` | 文本 | — | `UserComment` |
| `--owner` | 文本 | — | `OwnerName` |
| `--orientation` | 关键字或 1–8 | — | 绝对方向码，见 §9.5 |
| `--set-string` | `名称=值` | — | 按名称设置任意受支持标签，可重复 |
| `--remove` | 名称 | — | 按名称删除标签，可重复 |

**行为**

- `--set-string` 先按 §9.1 的字符串标签匹配，再按 §9.2 的数值与有理数标签匹配。有理数既接受 `1/200` 也接受小数。
- 标签名会被归一化：大小写、连字符、下划线和空格都忽略，所以 `lens-model`、`LensModel`、`lens model` 是同一个名字。
- `--remove` 只对能映射到已知标签的名称有效；未知名称会报错，而不是静默地什么都不做。
- `--orientation` 是绝对设定。要相对于当前值旋转请用 `rotate`（§8.6）。

**示例**

```powershell
pic-killer set .\photos --artist "Zhang San" --copyright "(C) 2024 Zhang San"
pic-killer set .\photos --make "Canon" --model "EOS R5" --lens-model "RF 24-70"
pic-killer set .\photo.jpg --set-string "iso=100" --set-string "fnumber=2.8"
pic-killer set .\photo.jpg --set-string "exposuretime=1/200"
pic-killer set .\photo.jpg --remove artist --remove copyright
```

### 8.4 `gps` —— 设置或清除定位

```
pic-killer gps [options] <--lat <deg>|--clear> <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--lat` | 十进制度 | — | 纬度，北纬为正、南纬为负 |
| `--lon` | 十进制度 | — | 经度，东经为正、西经为负 |
| `--alt` | 米 | — | 海拔，可选 |
| `--clear` | — | 关 | 删除所有 GPS 标签 |

**行为**

- `--lat` 与 `--clear` 至少要有一个。`--lat` 与 `--lon` 互相依赖，必须同时给。
- 十进制度会换算成 EXIF 的度/分/秒有理数形式，半球写进对应的 `Ref` 标签。
- `--clear` 删除每一个 GPS 标签，包括 `GPSVersionID`。
- 坐标按给定值原样写入，这里不校验范围；越界的值由 `verify`（§8.15）报出来。

**示例**

```powershell
pic-killer gps .\photo.jpg --lat 39.9042 --lon 116.4074
pic-killer gps .\photo.jpg --lat 30.2741 --lon 120.1551 --alt 12
pic-killer gps .\photos -r --clear
```

### 8.5 `strip` —— 清除元数据

```
pic-killer strip [options] <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--gps` | — | 关 | 只删 GPS；默认删除全部 |

**行为**

- 不加 `--gps` 时，**三套元数据在一次原子写入里全部清除**：EXIF 段、XMP 包、IPTC 块。每个文件那行会报出实际清掉了哪几套，如 `已清除 EXIF + XMP + IPTC`。
- 加 `--gps` 时只删*坐标* —— EXIF 的 GPS 标签与 XMP 的 `exif:GPS*` 属性。像 `photoshop:City` 或 IPTC `City` 这种文字地名不是坐标，会被保留；要删它们请用不带参数的 `strip`。
- 本来就没有元数据的文件仍然报 `[OK]` 而不是跳过；加 `--gps` 时，哪里都没有坐标的文件会被跳过。

**示例**

```powershell
pic-killer strip .\to-publish -r
pic-killer strip .\to-publish -r --gps
pic-killer strip .\photos -r --where has-gps --backup
```

### 8.6 `rotate` —— 复合方向变换

在 EXIF 方向标记的当前值上做相对调整。像素不动。

```
pic-killer rotate [options] <--cw|--ccw|--r180|--flip-h|--flip-v|--reset> <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--cw` | — | — | 顺时针旋转 90° |
| `--ccw` | — | — | 逆时针旋转 90° |
| `--r180` | — | — | 旋转 180° |
| `--flip-h` | — | — | 水平镜像 |
| `--flip-v` | — | — | 垂直镜像 |
| `--reset` | — | — | 方向重置为正常，并修复越界的方向值 |

**行为**

- 必须且只能给一个操作。
- 操作与现有方向复合。对已标记为顺时针 90° 的照片再 `--cw`，结果是 180° 而不是 90°。
- 复合在八个 EXIF 方向码上构成一个封闭的群：`--cw` 四次是恒等，`--cw` 接 `--ccw` 是恒等，任何镜像做两次都是恒等。
- 没有方向标签的文件按正常（码 1）处理。

**示例**

```powershell
pic-killer rotate .\photo.jpg --cw
pic-killer rotate .\photo.jpg --ccw
pic-killer rotate .\photos -r --r180
pic-killer rotate .\photos -r --reset
```

> `set --orientation` 是绝对写码，`rotate` 是在已有值上复合。`set --orientation normal` 和 `rotate --reset` 可以互换，但千万别以为 `rotate --cw` 是幂等的。

### 8.7 `copy` —— 从参考照片复制元数据

```
pic-killer copy [options] --from <reference> <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--from` | 文件 | — | 必填，从这张照片读元数据 |
| `--time` | — | 关 | 只复制拍摄时间相关字段 |
| `--gps` | — | 关 | 只复制 GPS |
| `--all` | — | 开 | 复制全部可复制的元数据 |

**行为**

- 不给任何选择器时，默认就是 `--all`。
- `--time` 与 `--gps` 可以同时给，表示只复制这两类。
- 与具体图像绑定的标签 —— 像素尺寸、方向之类 —— 会被有意跳过，因为它们描述的是另一张图。
- 参考文件只读一次，然后应用到每个目标。

**示例**

```powershell
pic-killer copy ".\burst\*.jpg" --from .\reference.jpg
pic-killer copy .\photos -r --from .\ref.jpg --time
pic-killer copy .\photos -r --from .\ref.jpg --gps
pic-killer copy .\photos -r --from .\ref.jpg --time --gps
```

### 8.8 `rename` —— 按拍摄时间重命名

```
pic-killer rename [options] <paths...>
```

**选项** —— 另可用 §5；接受 §7 里的 `-n`、`-y`、`-v`，但不接受 `--backup` 和 `-j`。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--pattern` | strftime 模板 | `%Y%m%d_%H%M%S` | 不含扩展名的文件名 |

**行为**

- 没有拍摄时间的文件会被跳过；绝不会用猜出来的名字去改名。
- 扩展名原样保留，模板只管主干部分。
- 如果两个文件会算出同一个名字，第二个加 `_1`，第三个加 `_2`，依此类推。
- 这是 `time --from-name`（§8.1）的逆操作。

**示例**

```powershell
pic-killer rename .\photos -r -n
pic-killer rename .\photos -r
pic-killer rename .\photos --pattern "%Y-%m-%d_%H.%M.%S"
pic-killer rename .\photos -r --where has-date
```

### 8.9 `xmp` —— 读写 XMP

相机、Lightroom 和手机把标题、评分、关键词、权利信息存在 XMP 里。现代的 IPTC Core 也是基于 XMP 的。

```
pic-killer xmp [options] <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--title` | 文本 | — | `dc:title` |
| `--description` | 文本 | — | `dc:description` |
| `--creator` | 文本 | — | `dc:creator`，可重复 |
| `--rights` | 文本 | — | `dc:rights` |
| `--rating` | 0–5 | — | `xmp:Rating` |
| `--label` | 文本 | — | `xmp:Label` |
| `--keywords` | 逗号分隔 | — | `dc:subject` |
| `--city` | 文本 | — | `photoshop:City` |
| `--country` | 文本 | — | `photoshop:Country` |
| `--set` | `前缀:名称=值` | — | 任意属性，可重复 |
| `--remove` | `前缀:名称` | — | 删除属性，可重复 |
| `--clear` | — | 关 | 删除整个 XMP 包 |
| `--sidecar` | — | 关 | 写 `<主干>.xmp`，完全不碰原图 |

**行为**

- 支持的容器是 JPEG（APP1）与 PNG（iTXt）。TIFF、WebP、HEIC 的 XMP 不支持。
- 写入会保留包里所有你没点名的既有属性，只增改你指定的那几个。
- `--rating` 校验范围 0–5。`--set` 会拿 §9.3 的表校验命名空间前缀，未知前缀直接报错。
- `--keywords` 按逗号切分。注意 §10 的 CSV 导入路径切分的是 `;` 或 `|`。
- `--sidecar` 在图片旁边写一个独立的 `.xmp`。因为压根不解析原图，所以对 EXIF 引擎读不了的 RAW（CR2、NEF、ARW）同样有效。`--sidecar --clear` 删除该 sidecar，且遵守 `--backup`。
- sidecar 名字按主干推导，于是 `IMG_0001.CR2` 与 `IMG_0001.JPG` 会同时指向 `IMG_0001.xmp`。这类冲突会在动任何文件之前被检出并报错拒绝；请用 `--ext` 分两次跑。sidecar 写入与其它写入走同一条原子替换路径。

**示例**

```powershell
pic-killer xmp .\photo.jpg --title "West Lake" --rating 5 --keywords "landscape,sunrise"
pic-killer xmp .\photo.jpg --creator "Zhang San" --creator "Li Si" --rights "(C) 2024"
pic-killer xmp .\photo.jpg --set "photoshop:Headline=Front page" --remove dc:description
pic-killer xmp .\photos -r --clear
pic-killer xmp .\shot.CR2 --sidecar --title "Sunrise" --rating 5
```

> `show` 会自动读 sidecar，所以用完 `--sidecar` 之后直接 `pic-killer show shot.CR2` 就能验证。

### 8.10 `iptc` —— 读写 IPTC-IIM

存在 JPEG APP13 / Photoshop 8BIM 块里的旧版 IPTC-IIM，至今仍是新闻与图库工作流的标准。

```
pic-killer iptc [options] <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--title` | 文本 | — | 2:05 Object Name |
| `--description` | 文本 | — | 2:120 Caption |
| `--keywords` | 逗号分隔 | — | 2:25 Keywords |
| `--creator` | 文本 | — | 2:80 By-line，可重复 |
| `--headline` | 文本 | — | 2:105 Headline |
| `--city` | 文本 | — | 2:90 City |
| `--state` | 文本 | — | 2:95 Province/State |
| `--country` | 文本 | — | 2:101 Country |
| `--copyright` | 文本 | — | 2:116 Copyright Notice |
| `--credit` | 文本 | — | 2:110 Credit |
| `--source` | 文本 | — | 2:115 Source |
| `--instructions` | 文本 | — | 2:40 Special Instructions |
| `--set` | `名称=值` 或 `2:105=值` | — | 任意数据集，可重复 |
| `--remove` | 名称或 `记录:数据集` | — | 删除数据集，可重复 |
| `--clear` | — | 关 | 删除整个 IPTC 块 |

**行为**

- 仅 JPEG。其它格式会被跳过。
- 值以 UTF-8 写入，并加上字符集标记，读取方才能正确解释。
- 其它 8BIM 资源块 —— 缩略图、色彩配置、路径 —— 原样保留。
- 数据集名称接受 §9.4 列出的别名，也接受显式的 `记录:数据集` 写法。

**示例**

```powershell
pic-killer iptc .\photo.jpg --title "Opening" --city "Beijing" --keywords "sport,opening"
pic-killer iptc .\photo.jpg --creator "Reporter A" --credit "Agency" --copyright "(C) Agency"
pic-killer iptc .\photo.jpg --set "2:105=Front page" --remove keywords
pic-killer iptc .\photos -r --clear
```

### 8.11 `restore` —— 从备份撤销

```
pic-killer restore [options] <paths...>
```

**选项** —— 另可用 §5；接受 §7 里的 `-n`、`-y`、`-v`。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--keep-backup` | — | 关 | 还原后保留 `.bak` |

**行为**

- 把 `<文件名>.bak` 字节级地覆盖回 `<文件名>`；备份就是原件的完整副本。
- 没有 `.bak` 的文件在处理前就被排除，而不是标为跳过；抬头会报告在多少个文件里找到了多少份备份。
- 还原成功后会删除 `.bak`，除非给了 `--keep-backup`。
- 因为 `--backup` 绝不覆盖已存在的 `.bak`，所以还原总是回到**第一次**带备份修改之前的状态。

**示例**

```powershell
pic-killer set .\photo.jpg --artist "Zhang San" --backup
pic-killer restore .\photo.jpg
pic-killer restore .\photos -r --dry-run
pic-killer restore .\photos -r --keep-backup
```

### 8.12 `geotag` —— 用 GPX 轨迹补 GPS

```
pic-killer geotag [options] --gpx <file> <paths...>
```

**选项** —— 另可用 §5 与 §7。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--gpx` | 文件 | — | 必填，GPX 轨迹 |
| `--tz` | 时区偏移 | 系统本地 | 相机时钟所在时区，如 `+08:00` |
| `--offset` | 偏移量 | — | 额外修正相机时钟误差，如 `-5m` |
| `--max-gap` | 秒 | `600` | 最近轨迹点在时间上超出此值就不标记 |

**行为**

- 每张照片的拍摄时间被换算成 UTC，然后在轨迹上用前后两个点线性插值定位；轨迹带海拔时海拔也插值。
- `--tz` 是最关键的参数：拍摄时间是不带时区的本地墙上时间，GPX 时间是 UTC。不给 `--tz` 就按系统本地时区解释。
- 没有拍摄时间的照片会被跳过。
- 时间落在轨迹之外、或落在宽于 `--max-gap` 的空档里的照片会被跳过，而不是近似处理。
- `--offset` 在查找之前加到照片时间上，用来补偿走快或走慢的相机时钟。

**示例**

```powershell
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00 --where no-gps
pic-killer geotag .\photos --gpx .\track.gpx --offset "+30s"
pic-killer geotag .\photos --gpx .\track.gpx --tz +08:00 --max-gap 120
```

> 配合 `--where no-gps` 使用，这样带 GPS 的相机本来就拍到位置的照片不会被动。

### 8.13 `apply` —— 从 CSV 导入

读三列 CSV 并把值写回去。配合 `show --csv` 就构成「导出 → 编辑 → 写回」的闭环。

```
pic-killer apply [options] --from <csv>
```

**选项** —— 只有 §7。本命令不吃路径，所以 §5 不适用，`--where` 也不适用。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--from` | 文件 | — | 必填，要读的 CSV |

**行为**

- CSV 格式、字段名与值的语法见 §10。
- 同一个文件的所有行会被合并，所以每个文件针对每套元数据只打开并写入一次，而不是每个字段写一次。
- 同一个文件的行可以同时指定 EXIF、XMP（`xmp:` 前缀）和 IPTC（`iptc:` 前缀）字段，每套体系各写一次。
- 指向不存在文件的行会被记为失败，命令以 2 退出。

**示例**

```powershell
pic-killer show .\photos -r --csv > meta.csv
pic-killer apply --from .\edited.csv -n
pic-killer apply --from .\edited.csv --backup -y
```

### 8.14 `report` —— 图库统计

只读。回答「我手上这批到底是什么货色」。

```
pic-killer report [options] <paths...>
```

**选项** —— 只有 §5。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| — | — | — | 无命令专属选项 |

**行为**

- 报告总数、有与没有拍摄时间各多少、有与没有 GPS 各多少、最早与最晚的拍摄时间、以及相机厂商与型号的分布。
- 没有相机信息的照片归到 `(无相机信息)` 一组。
- 处理是顺序的，没有并行。

**示例**

```powershell
pic-killer report .\photos -r
pic-killer report .\photos -r --where no-gps
pic-killer report .\photos -r --ext jpg,jpeg,heic
```

### 8.15 `verify` —— 元数据体检

只读的健康检查。为脚本与发版门禁设计。

```
pic-killer verify [options] <paths...>
```

**选项** —— 只有 §5。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| — | — | — | 无命令专属选项 |

**行为**

分两级检查：

- **问题** —— 元数据损坏（EXIF 块存在但解析不了，会附上底层原因）；GPS 纬度超出 ±90 或经度超出 ±180；拍摄时间无法解析；方向码不在 1–8 内。
- **警告** —— 拍摄时间在未来；`DateTimeOriginal` 与 `CreateDate` 不一致。
- 单纯没有 EXIF 的照片算健康，不算问题 —— 只有「块在但解析不了」才计为损坏。
- BMP 与 GIF 不算问题，因为它们本来就存不了元数据。
- 查出至少一个问题时以 2 退出；只有警告不改变退出码。

**示例**

```powershell
pic-killer verify .\photos -r
pic-killer verify .\photos -r --where has-gps
pic-killer verify .\incoming -r --ext jpg,jpeg,png,heic
```

### 8.16 `completions` —— Shell 补全与 man 手册页

```
pic-killer completions [--man] [shell]
```

**选项** —— §5 与 §7 都不适用。

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `[shell]` | `bash` `zsh` `fish` `powershell` `elvish` | — | 目标 shell |
| `--man` | — | 关 | 改为输出 roff 格式的 man 手册页 |

**行为**

- 生成的脚本写到标准输出，自己重定向到 shell 期望的位置。
- 必须给出 shell 或 `--man` 其中之一；两个都不给会报错。

**示例**

```powershell
pic-killer completions powershell | Out-String | Invoke-Expression
pic-killer completions bash > /etc/bash_completion.d/pic-killer
pic-killer completions zsh > ~/.zfunc/_pic-killer
pic-killer completions --man > pic-killer.1
```

---

## 9. 元数据字段参考

### 9.1 EXIF 字符串标签

可用于 `set --set-string`、`set --remove` 与 CSV `apply`。名称忽略大小写、连字符、下划线与空格。

| 名称与别名 | EXIF 标签 |
|-----------|-----------|
| `artist` | Artist |
| `copyright` | Copyright |
| `description`、`imagedescription` | ImageDescription |
| `software` | Software |
| `make` | Make |
| `model` | Model |
| `lensmake` | LensMake |
| `lensmodel` | LensModel |
| `owner`、`ownername` | OwnerName |
| `serial`、`serialnumber` | SerialNumber |
| `imageid`、`imageuniqueid` | ImageUniqueID |

### 9.2 EXIF 数值与有理数标签

有理数既接受分数（`1/200`）也接受小数（`0.005`）。

| 名称与别名 | EXIF 标签 | 值 |
|-----------|-----------|-----|
| `iso`、`isospeed`、`isospeedratings` | ISO | 0–65535 整数 |
| `fnumber`、`aperture` | FNumber | 无符号有理数 |
| `exposuretime`、`shutter`、`shutterspeed` | ExposureTime | 无符号有理数 |
| `focallength` | FocalLength | 无符号有理数 |
| `focallengthin35mmformat`、`focallength35` | FocalLengthIn35mmFormat | 整数 |
| `exposurecompensation`、`exposurecomp`、`ev` | ExposureCompensation | 有符号有理数 |
| `meteringmode` | MeteringMode | 整数 |
| `whitebalance` | WhiteBalance | 整数 |
| `flash` | Flash | 整数 |
| `exposureprogram` | ExposureProgram | 整数 |
| `colorspace` | ColorSpace | 整数 |
| `contrast` | Contrast | 整数 |
| `saturation` | Saturation | 整数 |
| `sharpness` | Sharpness | 整数 |

### 9.3 XMP 命名空间与属性

`xmp --set` 与 `xmp --remove` 接受的命名空间前缀：

| 前缀 | URI |
|------|-----|
| `rdf` | `http://www.w3.org/1999/02/22-rdf-syntax-ns#` |
| `dc` | `http://purl.org/dc/elements/1.1/` |
| `xmp` | `http://ns.adobe.com/xap/1.0/` |
| `photoshop` | `http://ns.adobe.com/photoshop/1.0/` |
| `lr` | `http://ns.adobe.com/lightroom/1.0/` |
| `xmpRights` | `http://ns.adobe.com/xap/1.0/rights/` |
| `Iptc4xmpCore` | `http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/` |

专用选项写入的属性，以及它们的 XMP 值形态：

| 选项 | 属性 | 形态 |
|------|------|------|
| `--title` | `dc:title` | 语言备选 |
| `--description` | `dc:description` | 语言备选 |
| `--rights` | `dc:rights` | 语言备选 |
| `--creator` | `dc:creator` | 有序序列 |
| `--keywords` | `dc:subject` | 无序集合 |
| `--rating` | `xmp:Rating` | 简单值 |
| `--label` | `xmp:Label` | 简单值 |
| `--city` | `photoshop:City` | 简单值 |
| `--country` | `photoshop:Country` | 简单值 |

### 9.4 IPTC-IIM 数据集

`iptc --set`、`iptc --remove` 与 CSV `iptc:` 字段接受的名称。显式的 `记录:数据集`（如 `2:105`）任何时候都接受。

| 名称与别名 | 数据集 | 显示为 |
|-----------|--------|--------|
| `title`、`objectname` | 2:05 | Title |
| `category` | 2:15 | Category |
| `keywords`、`keyword` | 2:25 | Keywords |
| `instructions` | 2:40 | Instructions |
| `datecreated` | 2:55 | DateCreated |
| `creator`、`byline`、`author` | 2:80 | Creator |
| `city` | 2:90 | City |
| `sublocation` | 2:92 | Sublocation |
| `state`、`province` | 2:95 | Province/State |
| `country` | 2:101 | Country |
| `headline` | 2:105 | Headline |
| `credit` | 2:110 | Credit |
| `source` | 2:115 | Source |
| `copyright` | 2:116 | Copyright |
| `caption`、`description` | 2:120 | Caption |
| `captionwriter` | 2:122 | CaptionWriter |

### 9.5 方向码

`set --orientation` 与 CSV `orientation` 接受的取值。

| 码 | 关键字 | 含义 |
|----|--------|------|
| 1 | `normal`、`top-left`、`tl` | 正常 |
| 2 | `mirror-h`、`flip-h` | 水平镜像 |
| 3 | `180`、`rotate-180`、`bottom-right`、`br` | 旋转 180° |
| 4 | `mirror-v`、`flip-v` | 垂直镜像 |
| 5 | `mirror-h-cw` | 先水平镜像，再顺时针旋转 90° |
| 6 | `cw`、`cw90`、`90`、`rotate-cw` | 顺时针旋转 90° |
| 7 | `mirror-h-ccw` | 先水平镜像，再逆时针旋转 90° |
| 8 | `ccw`、`ccw90`、`270`、`rotate-ccw` | 逆时针旋转 90° |

### 9.6 时间偏移单位

用于 `time --shift`、`time --interval` 与 `geotag --offset`。单位可自由组合、不区分大小写；开头的正负号作用于整个表达式。

| 单位 | 含义 |
|------|------|
| `y` | 年，按日历运算 |
| `mo` | 月，按日历运算 |
| `w` | 周 |
| `d` | 天 |
| `h` | 时 |
| `m` | 分 |
| `s` | 秒 |

---

## 10. CSV 导入格式

`apply` 读取三列 CSV：文件、字段、值。`show --csv --for-apply`（§8.2）输出的正是这个布局，因此导出、编辑、写回可以干净地闭环：

```powershell
pic-killer show .\photos -r --csv --for-apply > edit.csv
pic-killer apply --from .\edit.csv -n
```

```csv
file,field,value
photo1.jpg,artist,Zhang San
photo1.jpg,datetimeoriginal,2023-03-03 12:00:00
photo1.jpg,gps,"31.23,121.47,15"
photo1.jpg,iso,200
photo1.jpg,xmp:title,West Lake
photo1.jpg,xmp:rating,5
photo1.jpg,iptc:city,Hangzhou
```

解析规则：

- 表头行会被自动识别并跳过。
- 容忍 UTF-8 字节序标记，所以 Excel 存出来的文件能直接用。
- 含逗号的值必须用双引号包裹；引号内的字面双引号写成 `""`。
- 同一个文件的所有行会被合并后一起应用。

字段名分三组。

| 前缀 | 写入 | 字段名 |
|------|------|--------|
| 无 | EXIF | 见下表 |
| `xmp:` | XMP | `title` `description` `creator` `rights` `rating` `label` `keywords` `city` `country`，或任意 `前缀:名称` |
| `iptc:` | IPTC-IIM | §9.4 里的任意名称或 `记录:数据集` |

不带前缀时接受的 EXIF 字段名：

| 字段 | 值 |
|------|-----|
| `datetimeoriginal`、`date`、`original` | 日期时间 |
| `createdate`、`digitized`、`datetimedigitized` | 日期时间 |
| `modifydate`、`modify`、`datetime` | 日期时间 |
| `alldates`、`all` | 日期时间，三个字段一起写 |
| `gps` | `纬度,经度[,海拔]` |
| `gpsclear`、`cleargps`、`gps.clear` | 任意值，删除 GPS |
| `orientation` | §9.5 的码或关键字 |
| `usercomment` | 文本 |
| §9.1 里的任意名称 | 文本 |
| §9.2 里的任意名称 | 数值或有理数 |

> 有两个坑要知道。第一，多值字段按 **`;` 或 `|`** 切分，**不是**逗号 —— 这与 §8.9 那个按逗号切分的 `--keywords` 选项不一致。手写 CSV 时写 `"a,b,c"` 得到的是一个关键词而不是三个，应写成 `"a;b;c"`。`--for-apply` 导出的就是 `;` 形式，所以导出的文件能正确闭环。这条对 `xmp:keywords`、`xmp:creator`、`iptc:keywords`、`iptc:creator` 都适用。第二，`xmp:` 后面的简称必须是本手册列出的那几个；认不出来会直接报错并列出可用简称，所以像 `xmp:ttile` 这样的笔误会大声失败，而不是悄悄生成一个 `dc:ttile` 属性。要写其它属性，请给完整限定名，如 `xmp:photoshop:Headline`。

---

## 11. 输出格式

`show` 有三种输出模式，三种都写到标准输出，可以安全重定向；`--where` 的筛选提示和进度条走标准错误。

默认表格对每个文件打印一段：

```
=== C:\photos\beach.jpg ===
  位置：30.274100, 120.155100，海拔 12.0m
  Make              Canon
  Model             EOS R5
  DateTimeOriginal  2023:06:15 18:05:00
  GPSLatitude       30, 16, 2676/100 (26.7600)
  --- XMP ---
  dc:title          West Lake
  xmp:Rating        5
  --- IPTC ---
  City              Hangzhou
```

`--json` 输出一个文件对象数组。`latitude` 与 `longitude` 只在文件有 GPS 时出现，`xmp` 与 `iptc` 只在对应体系存在时出现：

```json
[
  {
    "file": "C:\\photos\\beach.jpg",
    "latitude": 30.274100,
    "longitude": 120.155100,
    "tags": [
      {"group": "IFD0", "name": "Make", "hex": "0x010F", "value": "Canon"},
      {"group": "EXIF", "name": "DateTimeOriginal", "hex": "0x9003", "value": "2023:06:15 18:05:00"}
    ],
    "xmp": { "dc:title": "West Lake" },
    "iptc": { "City": "Hangzhou" }
  }
]
```

`--csv` 每个标签输出一行，表头固定。XMP 与 IPTC 行的 `hex` 留空，含逗号的值加引号：

```csv
file,group,name,hex,value
C:\photos\beach.jpg,IFD0,Make,0x010F,Canon
C:\photos\beach.jpg,GPS,GPSLatitude,0x0002,"30, 16, 2676/100 (26.7600)"
C:\photos\beach.jpg,XMP,dc:title,,West Lake
C:\photos\beach.jpg,IPTC,City,,Hangzhou
```

`group` 列是该标签所属的 EXIF IFD —— `IFD0`、`EXIF`、`GPS` —— 另外两套体系则是 `XMP` / `IPTC`。

---

## 12. 退出码

| 码 | 含义 |
|----|------|
| `0` | 成功，也包括「什么都没匹配到」和「用户拒绝了确认」 |
| `1` | 命令在处理之前就失败了：参数错误、`--where` 解析失败、GPX 或 CSV 读不了 |
| `2` | 处理跑完了但至少有一个文件失败；`verify` 则是查出了至少一个问题 |

因为拒绝确认也是 0 退出，脚本必须加 `-y`，见 §7。

---

## 13. 安全模型

每次写入都走同一套流程，只实现了一份，所有写入类命令共用：

1. 若是 `--dry-run`，到此为止，只报告本来会发生什么。
2. 若给了 `--backup` 且 `<文件名>.bak` 尚不存在，把原件复制过去。
3. 记下文件当前的修改时间与访问时间。
4. 把完整的新文件写到同目录下的 `.<文件名>.pkick.tmp`。
5. 刷盘，然后原子地重命名覆盖原件。
6. 还原记下的时间戳，除非 `--also-file-time` 另有要求。

值得知道的后果：

- 批处理被打断 —— 断电、Ctrl-C、磁盘写满 —— 都不可能留下写了一半的照片。要么是旧文件，要么是完整的新文件。
- 临时文件与目标同目录，所以重命名不跨文件系统，是真正的原子操作。
- 因为文件是重写而不是就地打补丁，文件系统时间戳本来会变；这里特意还原，免得按文件日期排序的工具被打乱。
- 并行处理是安全的，因为每个文件有各自唯一的临时文件名，两个工作线程不会碰同一个文件。

---

## 14. 格式支持矩阵

| 格式 | EXIF | XMP | IPTC | 说明 |
|------|------|-----|------|------|
| JPEG | 支持 | 支持 | 支持 | 完整支持，三套体系可并存 |
| PNG | 支持 | 支持 | 不支持 | XMP 存在 iTXt chunk 里 |
| TIFF | 支持 | 不支持 | 不支持 | 仅 EXIF |
| WebP | 支持 | 不支持 | 不支持 | 仅无损与扩展格式的 WebP |
| HEIC | 支持 | 不支持 | 不支持 | 引擎支持，成熟度不及 JPEG |
| AVIF | 支持 | 不支持 | 不支持 | 引擎支持，成熟度不及 JPEG |
| JXL | 支持 | 不支持 | 不支持 | 引擎支持，成熟度不及 JPEG |
| RAW（CR2、NEF、ARW…） | 不支持 | sidecar | 不支持 | 用 `xmp --sidecar`，RAW 文件根本不会被打开 |
| BMP、GIF | 不支持 | 不支持 | 不支持 | 没有元数据容器，PIC-Killer 会明说并跳过 |

---

## 15. 输出与错误消息

每个文件的结果标记：

| 标记 | 含义 |
|------|------|
| `[OK]` | 文件已修改，或在 `--dry-run` 下将会被修改 |
| `[跳过]` | 有意跳过，括号里是原因 |
| `[失败]` | 该文件处理不了，括号里是错误 |

你大概率会碰到的消息：

| 消息 | 含义与处理 |
|------|-----------|
| `未找到符合条件的图片文件。` | 路径、`--ext` 或 `--where` 什么都没匹配到。检查拼写、放宽筛选 |
| `已取消。` | 确认提示被回答为否 —— 或者根本无法作答。加 `-y` |
| `无可解析的原始拍摄时间，偏移模式跳过` | `--shift` 需要一个已有时间作基准。改用 `--set` 或 `--from-name` |
| `BMP 无元数据容器，建议先转成 PNG 再处理` | BMP 与 GIF 根本存不了元数据 |
| `不支持的文件类型：<路径>` | 引擎解析不了这个容器，或者文件不存在 |
| `错误：无法解析 --where 条件 ...` | 条件语法无效，见 §6 |
| `错误：--where 条件为空` | 表达式里没有可用的条件 |
| `未知方向 ...` | 不是 §9.5 里的关键字或码 |
| `--rating 需在 0-5 之间` | `xmp --rating` 只接受 0 到 5 |
| `未知的命名空间前缀 ...` | `xmp --set` 要求用 §9.3 里的前缀 |
| `未知或暂不支持的字段 ...` | `apply` 不认识的 CSV 字段名，见 §10 |

---

## 16. 已知限制

- **冷门标签可能在重写中丢失。** EXIF 引擎认识数十个常见标签并原样保留。少见的厂商标签，特别是 MakerNote 的一部分，在元数据块重建时可能被丢掉。像素数据永远不受影响。
- **XMP 仅支持 JPEG 与 PNG。** TIFF、WebP、HEIC 无法通过 PIC-Killer 携带 XMP；需要的话用 `--sidecar`。
- **IPTC-IIM 仅支持 JPEG。**
- **RAW 文件无法直接写入。** 用 `xmp --sidecar`，反正 Lightroom 和 darktable 读的就是它。
- **不支持替换缩略图**，因为那需要重写 IFD1 的数据偏移。
- **HEIC、AVIF、JXL 不如 JPEG 经过考验。** 大批量之前先拿样本试 `--backup` 或 `--dry-run`。
- **`--where` 不能混用 `&&` 与 `||`。** 混用、以及对未知标签名做负向比较，现在都会报错而不是被静默误用，但文法仍不支持括号与优先级。见 §6。
- **只读命令是顺序执行的。** `show`、`report`、`verify` 不使用线程池，在大图库上比写入类命令慢。`--where` 筛选本身是并行的。
- **XMP 与 IPTC 写入受限于单个 JPEG 段。** 超过约 64 KB 的包写不进去；多段的 ExtendedXMP 尚未实现。

---

## 17. Shell 补全与 man 手册页

生成一次补全脚本，然后从 shell 配置里加载：

```powershell
pic-killer completions powershell | Out-String | Invoke-Expression
```

```bash
pic-killer completions bash > /etc/bash_completion.d/pic-killer
pic-killer completions zsh > ~/.zfunc/_pic-killer
pic-killer completions fish > ~/.config/fish/completions/pic-killer.fish
pic-killer completions --man > pic-killer.1
```

支持的 shell 是 `bash`、`zsh`、`fish`、`powershell`、`elvish`。man 手册页是 roff 格式，可直接 `man -l pic-killer.1`，也可安装到 `man1` 目录。
