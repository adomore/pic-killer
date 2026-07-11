# 发版门禁测试脚本

发布前手动跑的两套断言式测试（Windows / PowerShell）。与 CI 的 `cargo test` 互补：
CI 跑单元测试；这里跑**端到端功能覆盖**和**性能覆盖**，全绿才打版本标签。

| 脚本 | 覆盖 |
|------|------|
| `functest.ps1` | 全部子命令的端到端断言：无损（像素 SHA256）、EXIF/XMP/IPTC 并存、sidecar（含 RAW）、`--where` 单条件与 `&&`/`||`、通配符、BMP 友好跳过、completions、verify 等 |
| `perftest.ps1` | 500 张批量：顺序 vs 并行吞吐与加速比、高负载下的正确性、无残留临时文件 |

## 运行

前置：Windows + .NET（`System.Drawing`，用于生成测试图），并已构建 release 二进制。

```powershell
cargo build --release          # 在仓库根目录
.\tests\functest.ps1           # 期望：功能测试 N/N 通过，退出码 0
.\tests\perftest.ps1           # 期望：性能测试 4/4 通过，退出码 0
```

- 二进制路径按 `..\target\release\pic-killer.exe`（相对本目录）解析；测试用的临时图片写在
  `%TEMP%\pic-killer-functest` / `%TEMP%\pic-killer-perftest`，不污染仓库。
- 脚本以 **UTF-8 BOM** 保存：Windows PowerShell 5.1 读无 BOM 的 `.ps1` 会按 GBK 解码导致中文乱码，
  编辑后请保持 BOM。
- 有断言失败时退出码非零，可直接用于发版门禁。
