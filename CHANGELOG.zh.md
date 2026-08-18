# 更新日志

PIC-Killer 的全部重要变更都记录在此。格式遵循
[Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循
[语义化版本](https://semver.org/lang/zh-CN/spec/v2.0.0.html)。

[English](CHANGELOG.md) | **中文**

## [未发布]

对 1.0.0 的工程审计发现：在六道发版门禁全绿的情况下，工具主推的三条路径是坏的。
下面的内容关闭了那些发现。报告见 [docs/AUDIT.zh.md](docs/AUDIT.zh.md)，
其中写明了每条修复是如何验证的。

### 修复

- **`strip` 现在会清除三套元数据。** 此前它只清 EXIF，把 XMP 包与 IPTC 块
  —— 连同里面的姓名、城市、位置 —— 原样留在用户刚刚「为隐私清理过」的文件里，
  还报成功。现在 EXIF、XMP、IPTC 在一次原子写入里全部清除。`--gps` 删除 EXIF 与
  XMP `exif:GPS*` 中的坐标；文字地名由不带参数的 `strip` 负责。
- **`--where` 不再静默放大范围。** 对整个待处理集合里都不存在的标签名做负向比较
  （`camera!=Canon`）会匹配每一个文件；现在在任何写入之前就被拒绝。`&&` 与 `||`
  混用会误解析成错误的文件集合；现在会报错并指出是哪一半。
- **文档写的 CSV 闭环真的能跑。** `show --csv` 输出的是 `apply` 读不了的五列转储。
  新增的 `show --csv --for-apply` 输出 `apply` 认得的三列，并把 GPS 各分量合并成一行。
- **sidecar 写入是安全的。** 主干相同的 RAW+JPEG 对会解析到同一个 `.xmp`，并行时互相
  覆盖且双双报成功；现在冲突在派发前就被拒绝。sidecar 写入改走原子替换路径，并遵守
  `--backup`。
- **`verify` 能检出它宣称要检的损坏。** 加载器把解析失败降级成「没有元数据」，于是
  一段损坏的 EXIF 被读成健康。现在「没有」与「损坏」已区分开。它也会报告坐标解析
  不出来的 GPS 标签。
- **`apply` 每个文件只写一次。** 此前最多做三次独立的原子替换，写 IPTC 那步失败时，
  EXIF 与 XMP 的改动已经落盘了。
- **不存在的路径会报错。** 此前被静默丢弃且以 0 退出，于是一个拼写错误看起来像是
  一次成功却什么都没碰的批处理。
- **文件排序改为自然序。** 字节序把 `IMG_10` 排在 `IMG_2` 之前，这让
  `time --sequential` 按错误的顺序分配时间戳。
- **`rotate --reset` 能修复越界的方向值。** 此前它把文件当作「已经是正常」跳过 ——
  而这恰恰是 `verify` 会报告、用户却无从修复的那个问题。
- **`restore` 在两种模式下都是原子的**，从备份还原途中被打断也不再留下半截文件。
- **三处用户可触发的 panic 已消除**：`--tz` 传非 ASCII 值、`--pattern` 传非法
  strftime 说明符、`--where` 传非 ASCII 标签名。
- **非交互环境下缺 `-y` 会大声失败。** 此前确认提示读到输入结束、当成「否」、
  然后什么都没做却以 0 退出。提示本身也改到 stderr，重定向 stdout 时仍然可见。
- **零匹配时输出仍然可被机器解析。** `--json` 输出 `[]`，`--csv` 只输出表头，
  给人看的提示走 stderr。
- `xmp:` 后面认不出的简称会被拒绝，而不是静默生成 `dc:<名字>` 属性；XMP 限定名在
  插值前先做校验；文档写的 `gps.clear` 字段可用了；`apply` 的未知字段错误改为指向
  确实可用的 `xmp:` 与 `iptc:` 前缀；多值分隔符在导出与导入之间已一致；分母为 0 的
  GPS 有理数不再把裸 `NaN` 写进 JSON；`report` 按显示宽度对齐中文相机名；
  `show --csv` 中和表格软件的公式前缀；临时文件名不再可预测且失败时会被清理；
  `-v` 会输出设置摘要，不再是死代码。

### 新增

- `show --csv --for-apply` —— `apply` 能读回去的三列导出。
- `docs/` —— 新手入门手册、用户手册、审计报告与功能预研，均为结构镜像的 EN/ZH 双语对，
  由 `check-parity.sh` 强制保持同步。
- `Cargo.toml` 增加 `rust-version = "1.88"`，并由一个专门用该版本编译的 CI 作业守住。本轮早先的提交曾按「edition 2024 需要 1.85」把它设成 1.85，那是错的 —— 代码使用了 1.88 才稳定的 let-chain，1.87 会以 E0658 拒绝编译。
- 针对 Cargo 与 GitHub Actions 的 Dependabot 配置。

### 变更

- `--where` 每个文件只读一次（而非每个条件读一次），并且并行筛选。
- CI 在 `windows-latest` 上运行端到端测试（`tests/functest.ps1`），并新增最低 Rust 版本作业与依赖公告作业。
- 两个工作流的 `actions/checkout` 由 v4 升到 v7，消除每个作业都会出现的 Node 20 弃用告警。两份 `action.yml` 做字节级比对只有一行不同（`using: node20` → `node24`），输入项完全一致。
- release 用到的 action 升级并已用一次真实预发布验证：`upload-artifact` v4 → v7、`download-artifact` v4 → v8、`action-gh-release` v2 → v3。带连字符的 tag 现在会被发布为预发布版。
- 测试：单元测试 51 → 73，端到端断言 59 → 103。

### 安全

- **release 工作流存在命令注入。** 打包步骤把 `workflow_dispatch` 的 tag 输入直接内插进
  shell 字符串（`version="${{ ... }}"`）。GitHub 的表达式替换发生在 shell 解析之前，
  所以 `v1.0.0"; curl … | sh; :"` 这样的输入会被执行，而该作业持有对仓库可写的
  `GITHUB_TOKEN`。现在该值改经环境变量传入。只有具备 write 权限的账号能触发
  `workflow_dispatch`，因此这是「已有写权限或账号被盗」的提权路径，不是匿名攻击面。
- **quick-xml 0.37.5 → 0.41.0**，这正是 RUSTSEC-2026-0194（重复属性名检查的平方级扫描，
  面对构造过的 XMP 可造成拒绝服务）与 RUSTSEC-2026-0195 的补丁下界。本项目自己那两处
  `attributes()` 调用点因此脱离受影响范围。**但这不等于本项目不受影响：**
  `little_exif` 0.6.23 锁着 `quick-xml ^0.37.5`，依赖树里仍有第二份，而且它可达 ——
  该 crate 自己的 `src/xmp.rs` 在 PNG 清除 EXIF 的路径上同样调用了 `attributes()`。
  二进制增加 35,840 字节（+1.0%）。上游跟进后这一份会自动消失 ——
  已提 issue：TechnikTobi/little_exif#104。
- CI 新增 `cargo audit` 作业。上述两条公告发布于 2026-06-29，在此之前两个月无人知晓，
  就是因为没有任何环节检查。两条已知无法修复的按编号忽略，**新出现**的公告仍会让构建失败。
- CI 声明 `permissions: contents: read`；此前它继承的是仓库默认授予的任何权限。
- 发布工作流不再把 `contents: write` 发给每个作业。六个构建作业要编译整棵依赖树 ——
  其中每一个 `build.rs` 与过程宏 —— 而此前它们都握着一个对仓库可写的 token。写权限
  现在只发给真正创建 release 的那一个作业。同一次运行内下载产物不需要额外权限，
  并已用一次预发布运行完整验证。

## [1.0.0] —— 2026-07-11

### 新增

- `xmp --sidecar` —— 在图片旁读写独立的 `.xmp`，因此对 EXIF 引擎解析不了的 RAW 同样有效。
- `completions` —— 生成 bash、zsh、fish、PowerShell、elvish 的补全脚本，
  以及用 `--man` 生成 roff 手册页。
- `verify` —— 检查图库的元数据问题，发现问题时以非零码退出。

## [0.4.0] —— 2026-07-11

### 新增

- 所有写入类命令支持并行处理与进度条。
- `apply` —— 从 CSV 批量导入元数据，并扩展到 XMP 与 IPTC 字段。
- `report` —— 汇总整个图库的元数据覆盖情况。
- 内置通配符展开、数值与有理数 EXIF 标签、时区偏移写入。
- `--where` 支持 `&&` 与 `||` 组合。

## [0.3.0] —— 2026-07-11

### 新增

- `restore` —— 从 `.bak` 备份撤销修改。
- 所有命令支持 `--where` 元数据筛选。
- `geotag` —— 按拍摄时间在 GPX 轨迹上插值来打 GPS 标记。

## [0.2.0] —— 2026-07-11

### 新增

- CI 工作流；代码库通过 `fmt` 与 `clippy` 检查。
- 对没有元数据容器的格式（BMP、GIF）给出友好的跳过提示。

## [0.1.0] —— 2026-07-11

首个版本：照片元数据瑞士军刀，涵盖 `time`、`show`、`set`、`gps`、`strip`、`rotate`、
`copy`、`rename`、`xmp`、`iptc`，并提供跨平台发布二进制。

[未发布]: https://github.com/adomore/pic-killer/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/adomore/pic-killer/compare/v0.4.0...v1.0.0
[0.4.0]: https://github.com/adomore/pic-killer/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/adomore/pic-killer/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/adomore/pic-killer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/adomore/pic-killer/releases/tag/v0.1.0
