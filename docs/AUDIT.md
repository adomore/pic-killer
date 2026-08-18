# PIC-Killer — Audit Report

> Engineering audit of PIC-Killer 1.0.0: architecture, correctness, security, documentation consistency and test coverage. Findings are reproducible; every claim states how it was checked.

**English** | [中文](AUDIT.zh.md)

---

## 1. Scope and method

The audit covers commit `e30adbf` on `main`, working tree clean: 11 Rust source files, two PowerShell test suites, two GitHub Actions workflows, and `README.md`.

Three methods were used, in this order:

- **Static reading.** Every source file was read; 41 analysis agents extracted per-command specifications and audited four dimensions — correctness and data safety, security and robustness, performance, and command-line UX. Every candidate finding at medium severity or above was then given to an independent adversarial reviewer instructed to refute it. Three findings were refuted and dropped.
- **Build and gate execution.** The project was built and the full release gate run on Windows 11, Rust 1.91.1.
- **Live-fire probing.** Findings that predicted a user-visible behaviour were reproduced against the release binary on real image files. Findings marked *reproduced* below were confirmed this way; the rest rest on source reading alone.

The release gate was run in full and **passes completely**:

| Gate | Result |
|------|--------|
| `cargo test --locked` | 51 passed, 0 failed |
| `cargo fmt --all --check` | clean |
| `cargo clippy --all-targets --all-features` | clean, no warnings |
| `cargo build --release` | succeeds |
| `tests/functest.ps1` | 59/59 assertions pass |
| `tests/perftest.ps1` | 4/4 assertions pass |

Everything below was found in a codebase where all six of those gates are green. That is the central point of this report: the gates do not cover what breaks.

**Remediation.** The findings were then fixed. Each fix was verified by reproducing the original failure against the rebuilt binary and confirming it no longer occurs, and each is covered by a regression assertion named after its finding ID. The end-to-end suite grew from 59 to 103 assertions and now runs in CI on `windows-latest`; unit tests grew from 51 to 73, the new ones concentrated in `commands.rs`, which previously had five. Findings carry a **Status** line below; the description of each finding is left as it was written, so this document remains a record of what was found rather than a description of the current code.

---

## 2. Verdict

The engineering fundamentals are strong. The architecture is cleanly layered with no cycles, all writes funnel through a single atomic-replace choke point, there is no `unsafe` and no `TODO`, and the hand-written byte-level parsers for 8BIM, APP13, APP1 and PNG chunks are correctly bounds-checked — an adversarial security pass confirmed they carry no memory-safety exposure, and `quick-xml` gives no entity-expansion attack surface.

The defects are concentrated somewhere else: **in the gap between what the tool promises and what it does.** Three of them matter more than everything else combined.

- **`strip` does not strip.** The command documented as "clear all metadata to protect privacy" removes only EXIF. The XMP packet and the IPTC block survive untouched, along with any name, city or location they hold. The command reports success. This is a privacy tool failing at its privacy function (§6, F-01).
- **`--where` can silently select every file.** A negative comparison against a tag name that does not exist matches everything, and mixing `&&` with `||` misparses without an error. Both turn a narrow filter into a wide one, silently, on commands that delete metadata (§6, F-02, F-03).
- **The advertised CSV round trip does not work.** `show --csv` emits five columns; `apply` reads three. Feeding one to the other fails on every row (§6, F-04).

None of these are exotic edge cases. All three sit on the tool's main advertised paths, and all six release gates pass anyway — because the end-to-end suite tests `strip` on a file that has only EXIF, and never tests the round trip at all.

Grade as audited: **B−.** The code was better than the product.

**After remediation: A−.** Every critical and high finding, plus the three panics and the vacuous assertion, has been fixed and covered by a regression assertion. `strip` now clears all three metadata systems in one atomic write; `--where` refuses both the mixed-operator and unknown-tag-name traps instead of silently widening; `show --csv --for-apply` makes the documented round trip real; sidecar collisions are refused and sidecar writes are atomic; `verify` distinguishes absent metadata from corrupt metadata. Every medium finding has since been closed as well, except the I/O-sharing half of F-17. What still holds the grade below A: `commands.rs` remains 41% of production code, and although it went from five unit tests to twenty-two, those cover its pure helpers rather than the command bodies themselves, which are still exercised only end to end.

---

## 3. Project metrics

| Metric | Value |
|--------|-------|
| Production code | 5,687 lines across 11 modules |
| Unit test code | 753 lines, 51 tests, 9 modules |
| End-to-end scripts | 290 lines, 63 assertions |
| Documentation | 462 lines before this audit |
| Commands | 16 subcommands, 90 command-line flags |
| Dependencies | 11 direct, 86 including transitive |
| `unsafe` blocks | 0 |
| `TODO` / `FIXME` | 0 |
| `unwrap`/`expect` in production paths | 3, all provably safe |
| Test-to-production ratio | 18.3% overall; 1.8% for `commands.rs` |

Measured throughput, 500 JPEGs on a 24-core machine: sequential 1,111 ms, parallel 519 ms, `-j 4` 399 ms, `report` scan 238 ms.

---

## 4. Architecture review

The module graph is a clean five-layer stack with strictly downward dependencies and no cycles: entry point → declarative CLI → orchestration → three metadata engines → pure-logic utilities.

Two structural decisions deserve credit, because they are what keeps a 5,700-line tool coherent:

- **One write exit.** Every write in the entire program passes through `exif::commit_raw`, which owns backup, atomic replacement and timestamp preservation. A new command inherits all three guarantees for free and cannot accidentally opt out. The one place that bypasses it is sidecar writing — and that is exactly where a data-corruption defect appears (F-05).
- **One batch skeleton.** `run_batch` owns parallelism, the progress bar, the three-state tally and the summary. Ten write commands share a single implementation of all of it.

The weakness is distribution of mass. `commands.rs` is 2,336 lines — 41% of production code — and holds sixteen command functions plus their argument-to-edit mapping helpers. It has five unit tests. Everything hard to test by construction has accumulated in one file, and that file is where most of the findings below live.

The three metadata engines are the opposite: `whereexpr.rs`, `timeop.rs`, `gpx.rs` and `namedate.rs` are small, pure and tested at 36–108% density. This inverse correlation between module size and test density is the shape of the whole project.

---

## 5. Documentation and script consistency

Sixty-two documentation and script inconsistencies were identified against 191 individually verified-correct claims. The README is largely accurate; the failures cluster where documentation describes a workflow rather than a flag.

| Where | Claim | Reality |
|-------|-------|---------|
| `README.md:344-347`, `:34` | `show --csv` exports, you edit, `apply` writes back | `show --csv` emits `file,group,name,hex,value`; `apply` reads `file,field,value`. The round trip fails on every row. See F-04 |
| `README.md:371` | `gps.clear` clears GPS via CSV | Field normalisation strips `-`, `_` and space but not `.`; only `gpsclear`, `cleargps`, `gps-clear`, `gps_clear` work. See F-07 |
| `README.md:193-197` | `strip` clears all metadata for privacy | Clears EXIF only. See F-01 |
| `README.md:100-101` | `&&` and `||` combine conditions | True separately, but mixing them misparses silently rather than erroring. See F-03 |
| `README.md:61` | Requires Rust 1.88+ | Edition 2024 requires 1.85+; the stated floor is higher than the real one |
| `README.md:14`, `:75`, `:428` | Supported formats include TIFF and WebP | True for EXIF; XMP is JPEG and PNG only, yet `--ext` defaults include `tif,tiff,webp`, so a first `xmp` run on a TIFF folder returns nothing but skips |
| `README.md:50` | Five download rows | `release.yml` builds six targets; the musl static build is mentioned only parenthetically |
| `tests/functest.ps1:1` | "covers 14 commands" | Covers all 16 |
| `tests/functest.ps1:118` | Asserts `iptc --clear` worked | The assertion is vacuous. See F-19 |
| `src/commands.rs:1822` | Error says `apply` supports EXIF fields only | XMP and IPTC are supported via `xmp:` and `iptc:` prefixes; the message sends users to a dead end |
| `Cargo.toml` | — | No `rust-version` field, so the minimum supported Rust version is unenforced |
| repository root | — | Five releases, no `CHANGELOG` |

The `.github` directory contains only the two workflows: no issue templates, no `dependabot.yml`, no security policy.

---

## 6. Findings

Severity reflects real user impact. *Reproduced* means the behaviour was triggered against the release binary during this audit.

### F-01 `strip` leaves XMP and IPTC intact — the privacy promise is broken

**Location** `src/commands.rs:2089-2109` → `src/exif.rs:82-95`

**Impact** Critical. `strip` is documented as "clear all metadata to protect privacy". `exif::strip_all` delegates to the EXIF engine's `clear_metadata`, which only clears the EXIF segment. The XMP packet and the IPTC-IIM block are never touched, so creator names, cities, locations, titles and keywords all survive. The command prints `[OK]` and reports the file as modified. A user strips a photo before publishing and publishes their name and home city anyway.

**Evidence** Reproduced. A JPEG was given EXIF `Artist` and GPS, XMP `dc:title`/`dc:creator`/`photoshop:City`, and IPTC `Creator`/`City`. After `pic-killer strip -y`, `show` still reported the complete XMP and IPTC blocks, and a raw byte scan of the file still found `Zhang San`, `Hangzhou`, `dc:creator`, `xmpmeta` and `8BIM`. `strip --gps` likewise removes EXIF GPS while leaving `photoshop:City` and a custom `photoshop:Location` in place.

The end-to-end suite passes because `tests/functest.ps1:87-94` populates only EXIF before stripping.

**Recommendation** Call `xmp::remove_packet` and `iptc::remove_jpeg_iptc` from `process_strip` after `exif::strip_all` — both already exist and are unit-tested. For `--gps`, additionally drop XMP `exif:GPSLatitude`/`GPSLongitude` and `photoshop:City`/`State`/`Country`/`Location` and IPTC 2:90/2:95/2:101. Add an end-to-end assertion that populates all three systems before stripping. Consider a `--keep` whitelist so "remove everything except copyright and creator" becomes expressible.

**Status** Fixed. `process_strip` now clears the EXIF segment, the XMP packet and the IPTC block in a single atomic write, reporting which were present; `--gps` additionally removes XMP `exif:GPS*` properties. Re-verified with the original probe: after `strip`, a byte scan of a photo that carried all three systems finds none of the original names or place names. Regression assertions `F-01 …` cover both the tag view and the raw bytes.

### F-02 A negative `--where` comparison on an unknown tag name matches every file

**Location** `src/whereexpr.rs:204-220`

**Impact** High. `eval_tag` collects the values of every tag whose name contains the query name, then defines `!=` as `!eq` and `!~` as `!contains`. When the name matches no tag at all, the value list is empty, so both negations are true for every file. There is no distinction between "the tag exists and differs" and "there is no such tag". A typo — `camera!=Canon`, when the tag is actually `Make` — selects the entire library.

**Impact is amplified by what `--where` gates**: `strip`, `gps --clear`, `xmp --clear` and `iptc --clear` all accept it.

**Evidence** Reproduced against four files (two Canon, one Nikon, one bare). `make!=Canon` correctly selected 2/4. `camera!=Canon` selected **4/4**, as did `nonexistenttag!=whatever` and `nonexistenttag!~whatever`. A dry run of `strip --where "camera!=Canon"` listed all four files as targets.

**Recommendation** Distinguish absence from inequality. When no tag matches the name, `!=` and `!~` should return false, not true — or the whole condition should be an error, since a name matching nothing is almost always a typo. At minimum, warn on stderr when a comparison's tag name resolves to zero tags across the whole selection.

**Status** Fixed. `apply_where` now collects the tag names used by every negative comparison and checks each one against the whole selection in parallel before filtering, aborting with the suspect name if it matches nothing anywhere. `make!=Canon` still matches other makes and photos with no make at all.

### F-03 Mixing `&&` and `||` misparses silently instead of erroring

**Location** `src/whereexpr.rs:105-120`

**Impact** High. `parse_expr` checks for `||` first; if present it splits on `||` only and parses each half as a single condition. A half such as `no-gps && make=Canon` still contains `=`, so it parses as a comparison against a tag literally named `no-gps && make`, which matches nothing. The expression is accepted and returns a wrong file set. Worse, if the `&&` half uses `!=`, F-02 applies to it and the half evaluates true for every file.

The README documents mixing as unsupported, which is true but describes the wrong failure mode: users expect a rejection and get a silent wrong answer.

**Evidence** Reproduced. `has-gps && make!=Canon || make=Nikon` selected **4/4** files. The same expression with `=` instead of `!=` selected 1/4. By contrast `no-gps && bogusword` — no comparison operator present — is correctly rejected with a parse error, which is why the failure is invisible: the syntax that fails loudly and the syntax that fails silently look identical to the user.

**Recommendation** Reject any sub-expression that still contains the other operator after splitting — a two-line check that converts silent corruption into a clear error. The proper fix is a recursive-descent parser with parentheses, but the guard should land first and independently.

**Status** Fixed. `parse_expr` rejects any sub-expression that still contains the other operator after splitting, naming the offending half.

### F-04 The documented CSV round trip cannot work

**Location** `src/commands.rs:635, 654-661` (export) versus `src/commands.rs:1610-1615` (import)

**Impact** High. The README presents "export with `show --csv`, edit in a spreadsheet, write back with `apply`" as a headline workflow, and the feature table calls `apply` the write-back end of bulk spreadsheet editing. But `show --csv` writes five columns, `file,group,name,hex,value`, while `apply` reads the first three as `file,field,value`. The `field` column therefore receives the IFD group and `value` receives the tag name. The header is not skipped either, because the skip test compares column two against the literal `field` while the actual value is `group`.

**Evidence** Reproduced. `show --csv` output fed directly to `apply` produced `[失败] file (不支持的文件类型：file: File does not exist!)` for the header row and `[失败] … (未知或暂不支持的字段 'IFD0')` for the data row. Zero changes, exit code 2.

**Recommendation** Either add a three-column export mode to `show` — `--csv --for-apply`, emitting `file,field,value` with names `apply` accepts — or teach `apply` to detect and consume the five-column layout. Until one ships, the README should describe `apply`'s CSV as hand-authored. This is a design gap, not a typo: the two formats were never reconciled.

**Status** Fixed. `show --csv --for-apply` emits `file,field,value` using names `apply` accepts, merging the GPS component tags into a single `gps` row and skipping tags that cannot be written back. A full export-edit-import cycle is asserted end to end.

### F-05 Sidecar paths collide for RAW+JPEG pairs, and sidecar writes are not atomic

**Location** `src/xmp.rs:676-678`, `src/commands.rs:1107-1140`

**Impact** High. `sidecar_path` is `image.with_extension("xmp")`, so `IMG_0001.CR2` and `IMG_0001.JPG` — the standard dual-format output of every camera that shoots RAW+JPEG — both resolve to `IMG_0001.xmp`. `process_xmp_sidecar` is dispatched through `run_batch`, which is parallel by default, and it writes with a bare `std::fs::write`: no temporary file, no rename, bypassing `commit_raw` entirely. Two threads open, truncate and write the same path concurrently.

This also falsifies the comment at `src/commands.rs:143` asserting that parallel writing is safe because each task writes its own file.

**Evidence** Reproduced. A directory containing `IMG_0001.CR2` and `IMG_0001.JPG` processed with `xmp --sidecar --title Sunrise --rating 5` produced **one** `IMG_0001.xmp`; both files reported `[OK] 已写入 sidecar IMG_0001.xmp`. No conflict was reported.

**Recommendation** Route sidecar writes through the same atomic-replace path as everything else. Then either detect target collisions before dispatch and fail, or adopt the darktable convention of appending `.xmp` to the full filename (`IMG_0001.CR2.xmp`), which cannot collide. A `--sidecar-style adobe|darktable` option covers both ecosystems. Separately, sidecar mode currently ignores `--backup`, and `--sidecar --clear` hard-deletes the file, leaving `restore` nothing to recover.

**Status** Fixed. Colliding sidecar targets are detected before any file is touched and refused with both paths named; sidecar writes now go through the same `commit_raw` atomic path as every other write, and `--sidecar --clear` honours `--backup`.

### F-06 `verify` cannot detect the metadata corruption it advertises

**Location** `src/exif.rs:52`, `src/commands.rs:2270-2281`

**Impact** High for a command whose stated purpose is a health check and whose exit code is designed to gate CI. `verify`'s "problem" branch for unreadable metadata fires only when `load_metadata` returns an error. But `load_metadata` is `Metadata::new_from_path(path).unwrap_or_else(|_| Metadata::new())` — any parse failure is swallowed and replaced with empty metadata. Only an unrecognised container type produces an error. A JPEG with a truncated or corrupted EXIF block therefore looks like a photo with no metadata, yields no issues, and is counted as healthy.

**Evidence** Reproduced. A JPEG whose EXIF region was overwritten with `0xFF` bytes was reported by `verify` as `共 1 张：正常 1，问题 0，警告 0`, exit code 0.

**Recommendation** Add a fallible loader that surfaces the parse error, and have `verify` use it while the write commands keep the forgiving one. If that is not wanted, remove corruption detection from the command's description and from `cli.rs:54`.

**Status** Fixed. A new strict loader propagates parse failures while normalising the library's "no EXIF data" case — the only one with a stable message — back to empty metadata. Verified across all three cases: no EXIF is healthy, valid EXIF is healthy, and a present-but-unparseable block is reported as a problem with the underlying reason.

### F-07 The documented `gps.clear` CSV field does not exist

**Location** `src/commands.rs:1781-1784`, `:1807`

**Impact** High as a documentation defect, because it fails an operation users perform for privacy reasons and the error message does not suggest the working spelling. Field-name normalisation strips `-`, `_` and spaces but not `.`, while the match arm is `"gpsclear" | "cleargps"`. The documented `gps.clear` falls through to the catch-all and aborts the whole file's import.

**Evidence** Reproduced. A CSV row with field `gps.clear` produced `[失败] … (未知或暂不支持的字段 'gps.clear')`; the same row with `gpsclear` succeeded.

**Recommendation** Add `'.'` to the stripped character set — one character, and it makes the documented spelling work. Fix `README.md:371` either way.

**Status** Fixed. Field-name normalisation now strips `.` alongside `-`, `_` and space, so the documented spelling resolves. The unknown-field message of F-14 was rewritten at the same time to point at the `xmp:` / `iptc:` prefixes.

### F-08 Three user-triggerable panics

**Location** `src/commands.rs:1580` (`parse_tz`), `src/commands.rs:939` (`rename --pattern`), `src/whereexpr.rs:134`

**Impact** Medium. Each crashes with a Rust panic message and a backtrace hint instead of a clean error. No data is lost — all three panic before any write — but a crash in a tool whose help text advertises non-ASCII usage (`artist~张`) is a poor failure mode, and it undercuts the "no panic surface" impression given by a zero-`unwrap` production path. These panics come from library calls and byte-offset slicing, not from `unwrap`, so counting `unwrap`s does not measure them.

**Evidence** All three reproduced.

| Input | Result |
|-------|--------|
| `geotag --tz 😀` | panic at `commands.rs:1580` |
| `geotag --tz 中中` / `中文中` | clean error — the panic needs a specific byte length |
| `rename --pattern "%Q"` or `"%"` | panic inside chrono's formatter |
| `--where "😀=x"`, `"no:😀"`, `"😀"` | panic at `whereexpr.rs:134` |
| `--where "artist~😀"`, `"has:😀"` | no panic |

**Recommendation** Replace fixed byte-offset slicing with character-aware access or `get(..)` in `parse_tz` and `whereexpr.rs`. Validate `--pattern` with `StrftimeItems` before use and reject unknown specifiers with a message naming the offending token.

**Status** Fixed. Both byte-offset slices are now guarded (`get`, and an ASCII-digit check), and `--pattern` is validated with `StrftimeItems` before the batch runs, also rejecting path separators. All four inputs above now produce clean errors.

### Remaining findings

Medium severity:

| ID | Finding | Location |
|----|---------|----------|
| F-09 | `show --json`/`--csv` print Chinese prose to stdout when nothing matches, instead of `[]` or a bare header — reproduced, breaks `jq` and `Import-Csv` | `src/commands.rs:425-428` |
| F-10 | `apply` performs up to three separate atomic replaces per file, so a failure in a later system leaves earlier ones already committed | `src/commands.rs:1692` |
| F-11 | Multi-value separator differs between paths: `--keywords` splits on commas, CSV `xmp:keywords` splits on `;`/`\|` — reproduced, a comma-separated CSV value becomes one keyword | `src/commands.rs:1768-1773` |
| F-12 | An unrecognised name after `xmp:` in CSV is written as `dc:<name>` rather than rejected — reproduced, `xmp:ttile` creates `dc:ttile` | `src/commands.rs:1764` |
| F-13 | Nonexistent input paths are dropped silently and the run exits 0 — reproduced | `src/scan.rs:41` |
| F-14 | `apply`'s unknown-field error claims EXIF-only support, hiding the working `xmp:`/`iptc:` prefixes | `src/commands.rs:1822` |
| F-15 | XMP qualified names are interpolated into raw XML with no escaping or NCName validation | `src/xmp.rs:95` |
| F-16 | A GPS rational with denominator 0 makes `show --json` emit bare `NaN`/`inf`, invalidating the document | `src/commands.rs:556` |
| F-17 | `--where` re-reads and re-parses the whole file once per condition, and filters serially before the parallel batch | `src/whereexpr.rs:179`, `src/commands.rs:54` |
| F-18 | `--sequential` orders by byte-lexicographic path, silently overriding the user's argument order | `src/scan.rs:44` |
| F-19 | The `iptc --clear` end-to-end assertion is vacuous: `^\s+Title` without `(?m)` can never match a multi-line string whose first line is `=== path ===` — reproduced; the assertion passes even if `--clear` does nothing | `tests/functest.ps1:118` |
| F-20 | Sidecar mode ignores `--backup`, and `--sidecar --clear` hard-deletes with no recovery path | `src/commands.rs:1121` |

**Every medium finding is now fixed except half of F-17.** `apply` composes all three metadata systems into one buffer and commits once, so a failure in a later system can no longer leave an earlier one already on disk (F-10). An unrecognised short name after `xmp:` is rejected with the valid names rather than written as `dc:<name>` (F-12). A named path that does not exist aborts the run before anything is touched, instead of being dropped silently with exit 0 (F-13). XMP qualified names are validated as NCNames before being interpolated into the packet (F-15). A GPS rational with denominator 0 no longer escapes as `NaN` into the JSON document — `read_gps` rejects non-finite values and `verify` reports the file as corrupt via a new `has_gps_tags` probe (F-16). File ordering is now natural rather than byte-lexicographic, so `--sequential` numbers `IMG_2` before `IMG_10` (F-18). Empty result sets emit `[]` or a bare header with the notice on stderr (F-09); multi-value separators are consistent across export and import (F-11); the misleading `apply` error points at the `xmp:`/`iptc:` prefixes (F-14); the vacuous assertion gained its `(?m)` flag (F-19); sidecar mode honours `--backup` and writes atomically (F-20).

**F-17 is now fully fixed.** `--where` reads each candidate file once into a `FileFacts` value that every condition in the expression shares, so a two-condition expression no longer loads the file twice; the filter also runs in parallel, and the unknown-tag-name guard piggybacks on the same pass instead of costing an extra scan. Measured on 400 JPEGs with a warm cache and conditions chosen to match zero files (so the timing contains only the filter): 1 condition 77 ms, 2 conditions 97 ms, 3 conditions 107 ms, 4 conditions 94 ms — flat in the number of conditions, where it previously scaled with it.

Low severity, in brief: the confirmation prompt is written to stdout and vanishes when stdout is redirected; stdin EOF is treated as "no" and exits 0, so non-TTY runs silently no-op; `-v/--verbose` is dead code because its only branch is unconditionally true; `show` reads each file three times; every write loads the whole file into memory with no size cap; the temporary filename is fully predictable and `File::create` follows symlinks; `get_file_type` failures report "unsupported file type" even when the file simply does not exist; exit code 2 means two different things across commands and is undocumented; `report`'s camera-distribution column pads by character count rather than display width, so CJK camera names misalign; `show --csv` does not neutralise spreadsheet formula prefixes while the README tells users to open the file in Excel; `rotate --reset` cannot repair an out-of-range orientation, which is exactly the defect `verify` reports; `atomic_replace` never fsyncs the directory and leaves `.pkick.tmp` behind on interruption; `restore` is not atomic in `--keep-backup` mode; XMP and IPTC writes are hard-capped at one 64 KB JPEG segment; PNG EXIF is written as a legacy zTXt chunk rather than the standard `eXIf` chunk.

Of that list, the following are now fixed and carry regression assertions where they are observable: the prompt moved to stderr and a non-TTY run without `-y` fails with an error instead of no-opping; `-v` prints a real settings summary; the temporary filename now carries the process id and a counter and is created with `create_new` so it cannot follow a planted symlink, and it is removed if the write fails; a missing file reports "文件不存在" rather than "unsupported file type"; `report` pads by display width; `show --csv` neutralises formula prefixes while `--for-apply` stays verbatim; `rotate --reset` repairs an out-of-range orientation; `restore` is atomic in both modes; and the directory is fsynced after rename on Unix. Exit codes are now documented in the user manual (§12). Still open and documented rather than fixed: `show` reading each file three times, the absence of a size cap on whole-file reads, the one-segment ceiling on XMP and IPTC, and PNG EXIF using zTXt.

Three candidate findings were **refuted** during adversarial review and are not listed above: a claim that `load_metadata` wipes metadata on any write, a claim that `apply` lacks directory confinement, and a claim that sequential read-only commands constitute a defect rather than a performance opportunity.

---

## 7. Security review

The security posture is better than the correctness posture. An adversarial pass over every attack surface found no memory-safety exposure.

- **Hand-written parsers are sound.** The 8BIM, APP13, IIM and PNG-chunk parsers in `iptc.rs` and `xmp.rs` are correctly bounds-checked throughout. Length fields taken from the file are validated before use; there are no unchecked slices and no integer overflows. A malicious image cannot cause out-of-bounds reads. Two silent truncations were noted, but they are safe.
- **No XML entity exposure.** `quick-xml` does not expand entities by default, so GPX and XMP parsing carry no billion-laughs or external-entity risk, and deep nesting is bounded. One note: a `DOCTYPE` in a source XMP packet is copied through verbatim into the written packet, which is worth stripping.
- **No path traversal in practice.** `apply` reads target paths from the CSV without directory confinement, which an adversarial reviewer examined and refuted as a finding: the CSV is a user-authored input file, equivalent in trust to the command line itself.

The real security-adjacent issues are the privacy ones, and they are correctness bugs rather than vulnerabilities: F-01 (`strip` leaves personal data in the file) and F-02/F-03 (a filter intended to protect files can silently fail to). For a tool whose stated purpose includes stripping metadata before publication, F-01 is the most serious item in this report.

Two lower-severity items: the temporary filename is fully predictable and `File::create` follows symlinks, so a hostile local user with write access to the target directory could pre-plant a symlink — low impact given that such a user can already modify the photos. And `show --csv` does not prefix-neutralise cells beginning with `=`, `+`, `-` or `@`, so image metadata containing a formula becomes a live formula when the README's suggested Excel workflow is followed.

---

## 8. Test coverage

The suite is genuinely good at what it covers and structurally blind in a specific way.

Strengths: 51 unit tests concentrate on the pure logic where bugs hide — the orientation composition matrix is exhaustively verified as a closed group, time-delta calendar arithmetic is tested across month ends, GPX interpolation is tested at its boundaries, and IPTC and XMP round-trip at the byte level with preservation of unrelated blocks. The end-to-end suite verifies losslessness the only way that actually proves it: SHA-256 of the decoded pixels before and after each write.

The blind spots are structural, and every one of F-01 through F-07 sits in one:

- **The end-to-end suite is not in CI.** It requires PowerShell and `System.Drawing`, so it runs on Windows only, by hand. CI runs `cargo test`, `fmt` and `clippy` on three platforms — which never execute `commands.rs`'s orchestration logic at all.
- **Fixtures are too simple.** Tests build a photo with one metadata system and then assert about that system. `strip` is tested on an EXIF-only file, which is exactly why F-01 survives a green suite.
- **Round trips between commands are untested.** Each command is tested alone; no test feeds `show --csv` to `apply`, which is why F-04 survives.
- **`--where` is unit-tested for parsing but not for selection semantics.** `whereexpr.rs` has six tests, all on `parse`. Nothing tests what `matches` returns for an absent tag name, which is why F-02 survives.
- **One assertion is vacuous** (F-19), and the perf suite's speedup numbers are confounded: the sequential run executes first and the `-j 4` run last, so file-cache warming inflates later runs. `-j 4` measuring faster than the 24-thread default is not currently interpretable.

Coverage by module is inversely proportional to module size: `namedate.rs` has more test code than production code; `commands.rs`, at 41% of the codebase, has 1.8%.

**After remediation.** The end-to-end suite is 103 assertions and runs in CI on `windows-latest`, so `commands.rs` has automated regression protection for the first time. Every fixed finding gained an assertion named after its ID, and the fixture weakness that let F-01 survive was addressed directly: the new `strip` test populates EXIF, XMP and IPTC and then checks the raw bytes, not just the tag view. The command-to-command round trip is now tested (F-04), and `--where` selection semantics are tested rather than only its parser (F-02, F-03).

Unit tests went from 51 to 73. `commands.rs` went from five to twenty-two, covering the pure helpers the audit named: field-name normalisation, the `apply` field vocabulary, XMP short-name resolution, timezone parsing, rename pattern validation and target naming, sidecar collision detection, and the CSV/JSON escapers. Two of those tests failed on first run and both were real: `parse_tz` accepted `++08` because Rust's integer parser tolerates a leading sign, and `filter_props` depended on every caller pre-lowercasing the keyword — an implicit contract whose failure mode is silently showing fewer rows. Both were fixed.

---

## 9. Build, CI and release

The build and release machinery is the most mature part of the project. CI enforces formatting, treats clippy warnings as errors, and runs tests on Linux, Windows and macOS with cancel-in-progress concurrency. The release workflow cross-compiles six targets including a musl static build and ARM64, publishes SHA-256 checksums, and fails on unmatched files.

Gaps, in priority order:

| Gap | Consequence |
|-----|-------------|
| ~~The end-to-end suite is not in CI~~ — **fixed**, an `e2e` job on `windows-latest` builds release and runs `functest.ps1` | The largest module had no automated regression protection |
| No `rust-version` in `Cargo.toml` | The minimum supported Rust version is unenforced, and the README's claimed 1.88+ is wrong — edition 2024 needs 1.85+ |
| No `CHANGELOG` | Five releases with only auto-generated notes |
| No `dependabot.yml` | 86 transitive dependencies with no automated update path |
| No security policy or issue templates | `.github/` contains only the two workflows |
| CI does not run `cargo test --release` | The release profile uses thin LTO and `strip`; only debug builds are tested |

Adding a `windows-latest` job that runs `tests/functest.ps1` is the single highest-value change available. The script is already assertion-based and already returns a non-zero exit code, so it can serve as a gate unmodified.

---

## 10. Prioritized recommendations

| # | Action | Effort | Why now |
|---|--------|--------|---------|
| 1 | Make `strip` remove XMP and IPTC (F-01) | Small | The tool's privacy promise is currently false. `xmp::remove_packet` and `iptc::remove_jpeg_iptc` already exist |
| 2 | Fix `--where` negation and reject mixed `&&`/`||` (F-02, F-03) | Small | Both silently widen destructive commands. The mixed-operator guard is a two-line check |
| 3 | Add `tests/functest.ps1` to CI as a `windows-latest` job | Small | Converts a manual gate into an automated one and covers the untested 41% |
| 4 | Reconcile `show --csv` with `apply` (F-04) | Medium | A headline documented workflow does not work at all |
| 5 | Fix sidecar collision and route it through atomic write (F-05) | Small | Silent data loss on the standard RAW+JPEG layout |
| 6 | Strengthen fixtures: populate all three systems, test command round trips, test `--where` selection | Medium | Every top finding survived a green suite for the same reason |
| 7 | Fix the three panics (F-08) and the vacuous assertion (F-19) | Small | Crashes on advertised non-ASCII input |
| 8 | Make `verify` see corruption, or stop claiming it (F-06) | Small | A health check that cannot see the problem it names |
| 9 | Emit `[]` / bare header for empty `--json`/`--csv` (F-09); fix `gps.clear` (F-07) | Small | Both break scripted use for one-character reasons |
| 10 | Add `rust-version`, a `CHANGELOG`, and `dependabot.yml` | Small | Standard hygiene for a v1.0.0 with published binaries |
| 11 | Parallelise read-only commands; cache file reads across `--where` conditions (F-17) | Medium | Speed is an explicit positioning claim that currently holds only for writes |

**Status: every recommendation in this table is now done.** An earlier draft of this section claimed that items 1, 2, 3, 5, 7, 8 and 9 closed every critical and high finding; that was wrong, because F-04 is a high finding addressed by item 4, which was therefore done as well. Item 6 is now complete: `commands.rs` gained seventeen unit tests covering its pure helpers, and two of them found real defects on the first run. Item 10 landed only partially — `rust-version` was added to `Cargo.toml`, but there is still no `CHANGELOG` and no `dependabot.yml`. Item 11 (sharing one file read across `--where` conditions) landed as well, and `CHANGELOG.md` / `dependabot.yml` closed the rest of item 10. Most of the low-severity list at the end of §6 has since been addressed too; what remains there is documented rather than fixed.

---

## 11. Appendix: how each claim was verified

| Claim class | Method |
|-------------|--------|
| Gate status | `cargo test --locked`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features`, `cargo build --release`, `tests/functest.ps1`, `tests/perftest.ps1`, all executed on Windows 11 with Rust 1.91.1 |
| Command surface, flags, defaults | `pic-killer <cmd> --help` captured for all 16 commands and compared against `src/cli.rs` |
| Output formats | Real output captured from the release binary for table, `--json` and `--csv` modes |
| F-01 `strip` | Photo populated with EXIF + XMP + IPTC, stripped, then re-read with `show` and scanned at the byte level for the original strings |
| F-02, F-03 `--where` | Four fixture files with known metadata; each expression run under `show` and the selection count compared against the correct answer; `strip -n` used to confirm the target set |
| F-04 CSV round trip | `show --csv` output piped directly into `apply --from` |
| F-05 sidecar | Directory containing `IMG_0001.CR2` and `IMG_0001.JPG` processed with `xmp --sidecar`; resulting sidecar count checked |
| F-06 `verify` | JPEG with its EXIF region overwritten, then run through `verify` |
| F-07 `gps.clear` | CSV rows with both the documented and the working spelling |
| F-08 panics | Each input run directly and the process output checked for a panic message and source location |
| F-09, F-11, F-12, F-13, F-19 | Reproduced individually against the release binary |
| Findings not marked *reproduced* | Source reading plus independent adversarial review; three candidate findings were refuted and dropped |
| Line counts and metrics | Direct measurement, splitting each source file at its `#[cfg(test)]` boundary |
| Each fix | The probe that originally produced the failure was re-run against the rebuilt release binary and had to produce the corrected behaviour, then encoded as an end-to-end assertion. `cargo test`, `fmt`, `clippy -D warnings`, `functest.ps1` (83/83) and `perftest.ps1` (4/4) were all re-run green afterwards |
