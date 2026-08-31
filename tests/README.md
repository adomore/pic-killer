# Release gate test scripts

**English** | [中文](README.zh.md)

Two assertion-based suites run before a release (Windows / PowerShell). They complement CI's `cargo test`:
`cargo test` covers the unit tests; these cover **end-to-end behaviour** and **performance**, and a version tag is only cut when both are green.

| Script | Coverage |
|--------|----------|
| `functest.ps1` | End-to-end assertions across all 16 subcommands: losslessness (pixel SHA256), EXIF/XMP/IPTC side by side, sidecars (RAW included), `--where` single conditions and `&&`/`\|\|`, wildcards, the friendly BMP skip, completions, verify, plus a set of regression assertions named after the audit findings (F-01…F-10, F-12, F-13, F-15, F-18). 103 assertions in total |
| `perftest.ps1` | 500-file batch: sequential vs parallel throughput and speedup, correctness under load, no leftover temporary files. 4 assertions in total |

`functest.ps1` runs in CI (the `e2e` job in `.github/workflows/ci.yml`, on `windows-latest`)
and is the only automated regression protection the `commands.rs` orchestration layer has. `perftest.ps1` is sensitive to machine load, so it is still run by hand locally.

## Running them

Prerequisites: Windows plus .NET (`System.Drawing`, used to generate test images), and a release binary already built.

```powershell
cargo build --release          # from the repository root
.\tests\functest.ps1           # expected: functional tests 103/103, exit code 0
.\tests\perftest.ps1           # expected: performance tests 4/4, exit code 0
```

If you see `无法加载文件……在此系统上禁止运行脚本`, that is the machine's execution policy, not the tests.
To work around it without changing a global setting:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tests\functest.ps1
```

- The binary path resolves to `..\target\release\pic-killer.exe`, relative to this directory; temporary test
  images are written to `%TEMP%\pic-killer-functest` / `%TEMP%\pic-killer-perftest` and never pollute the repository.
- The scripts are stored with a **UTF-8 BOM**: Windows PowerShell 5.1 decodes a BOM-less `.ps1` as GBK, which
  mangles the Chinese text. Keep the BOM when editing.
- A failed assertion exits non-zero, so the scripts can be used as a release gate directly.
