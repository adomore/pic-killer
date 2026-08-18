# PIC-Killer — Getting Started

> A 20-minute introduction for people who have never used PIC-Killer. Read this once, then keep [the User Manual](USER-MANUAL.md) open as a reference.

**English** | [中文](GETTING-STARTED.zh.md)

---

## 1. What PIC-Killer does

PIC-Killer edits the **metadata** of photos — capture time, author, copyright, camera, lens, GPS, orientation, title, keywords, rating — in bulk, from the command line.

It never re-encodes the image. It removes the old metadata block, inserts a new one, and leaves the compressed pixel data untouched byte for byte. Decode the photo before and after an edit and you get identical pixels.

A photo can carry three independent metadata systems at once. PIC-Killer reads and writes all three, and guarantees that touching one never damages the others:

| System | Where it lives | What it usually holds |
|--------|----------------|-----------------------|
| EXIF | JPEG APP1, PNG/TIFF/WebP/HEIC containers | Capture time, camera, lens, exposure, GPS, orientation |
| XMP | JPEG APP1, PNG iTXt, or a sidecar `.xmp` file | Title, description, rating, keywords, colour label, city |
| IPTC-IIM | JPEG APP13 (Photoshop 8BIM), JPEG only | Newsroom and stock-library fields: caption, byline, credit |

Pick a command by what you want to do:

| I want to… | Command |
|-----------|---------|
| See what metadata a photo already has | `show` |
| Fix a wrong capture time | `time` |
| Set author, copyright, camera, lens | `set` |
| Add or remove GPS coordinates | `gps` |
| Remove metadata before sharing | `strip` |
| Fix a sideways photo without re-encoding | `rotate` |
| Copy metadata from one photo to many | `copy` |
| Rename files by capture time | `rename` |
| Edit title, rating, keywords | `xmp` |
| Edit newsroom / stock-library fields | `iptc` |
| Undo an edit | `restore` |
| Add GPS from a recorded track | `geotag` |
| Import a batch of edits from a spreadsheet | `apply` |
| Summarise a whole library | `report` |
| Check a library for metadata problems | `verify` |
| Install shell tab-completion | `completions` |

> PIC-Killer does not convert formats, resize, crop, or re-compress. It only rewrites metadata. If a format has no place to store metadata (BMP, GIF), PIC-Killer tells you so and skips the file instead of damaging it.

---

## 2. Install

The easiest route is a prebuilt binary. Download the archive for your platform from the [Releases page](https://github.com/adomore/pic-killer/releases/latest) and unpack it — inside is a single executable with no runtime dependencies.

Every release ships a `SHA256SUMS.txt` so you can check the download:

```bash
sha256sum -c SHA256SUMS.txt
```

To build from source instead, you need a Rust toolchain new enough for edition 2024:

```bash
git clone https://github.com/adomore/pic-killer.git
cd pic-killer
cargo build --release
```

Confirm the binary works before you point it at real photos:

```bash
pic-killer --version
pic-killer --help
```

> On older Windows consoles, Chinese text may appear as garbage. Run `chcp 65001` once to switch the console to UTF-8.

---

## 3. Look before you touch

Every session should start with `show`. It is read-only — it cannot damage anything.

```powershell
pic-killer show .\photo.jpg
```

Real output looks like this:

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

Read it top to bottom: if the photo has GPS, the first line is the decoded decimal position; then the EXIF tags; then, if present, an `--- XMP ---` block and an `--- IPTC ---` block. When a file has no metadata at all you get `(无匹配的元数据)`.

To narrow a long list, or to survey a whole folder instead of one file:

```powershell
pic-killer show .\photo.jpg --filter gps
pic-killer report .\photos -r
```

`report` answers the question "what am I dealing with?" for a whole library — how many photos have a capture time, how many have GPS, the date range, and which cameras took them.

---

## 4. The safety net

PIC-Killer has three independent layers of protection. Learn them before your first write.

**Layer 1 — preview.** `-n` (or `--dry-run`) shows exactly what would change and writes nothing:

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

**Layer 2 — backup and undo.** `--backup` copies each file to `<name>.bak` before touching it, and `restore` puts it back byte for byte:

```powershell
pic-killer set .\photo.jpg --artist "Zhang San" --backup
pic-killer restore .\photo.jpg
```

**Layer 3 — atomic writes.** Every write goes to a temporary file in the same folder, is flushed to disk, and only then atomically replaces the original. Interrupting a batch job cannot leave a half-written photo. The original file's modification time is preserved unless you ask otherwise.

> Two things about `--backup` that surprise people. First, an existing `.bak` is **never** overwritten — so if you edit twice with `--backup`, `restore` takes you back to the *pristine original*, not to the state after the first edit. Second, `restore` deletes the `.bak` afterwards unless you pass `--keep-backup`.

Write commands ask for confirmation unless you pass `-y`. In a script or a pipeline there is no one to answer, so **always pass `-y` in automation** — otherwise the command reads end-of-input, treats it as "no", prints `已取消。`, and exits successfully having done nothing.

---

## 5. Five real tasks

### 5.1 The capture time is wrong

Three different problems, three different flags. Use `--shift` when the camera clock was off by a constant amount, `--set` when you know the true time, and `--from-name` when the filename already contains the date.

```powershell
pic-killer time .\photos -r --shift "+2h"
pic-killer time .\photo.jpg --set "2024-01-01 12:00:00"
pic-killer time .\photos --from-name
```

Offset units combine freely: `y` years, `mo` months, `w` weeks, `d` days, `h` hours, `m` minutes, `s` seconds. Note that `m` is **minutes** and `mo` is **months** — this is the single most common mistake.

```powershell
pic-killer time .\photos --shift "-1d12h30m"
```

### 5.2 Stamp author and copyright across a shoot

```powershell
pic-killer set .\shoot -r --artist "Zhang San" --copyright "(C) 2024 Zhang San"
```

The same command sets camera and lens fields, which is useful for scans and film work where the camera wrote nothing:

```powershell
pic-killer set .\scans -r --make "Nikon" --model "FM2" --lens-model "50mm f/1.4"
```

### 5.3 Remove GPS before sharing

`strip --gps` removes only the location and keeps everything else. Plain `strip` removes all metadata.

```powershell
pic-killer strip .\to-publish -r --gps
pic-killer strip .\to-publish -r
```

Verify it actually went away before you upload:

```powershell
pic-killer show .\to-publish -r --filter gps
```

### 5.4 Add GPS from a recorded track

If you recorded a GPX track on a phone or watch, PIC-Killer can interpolate each photo's position from its capture time.

```powershell
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00
```

`--tz` matters more than anything else here. Capture times are local wall-clock times with no timezone attached; GPX timestamps are UTC. Tell PIC-Killer what timezone the camera clock was in, or every photo lands at the wrong point on the track. Photos with no capture time, or that fall in a gap longer than `--max-gap` (600 seconds by default), are skipped rather than guessed.

### 5.5 Rename files by capture time

```powershell
pic-killer rename .\photos -r -n
pic-killer rename .\photos -r
```

The default template produces `20230115_143022.jpg`. Files with no capture time are skipped, and if two photos would get the same name the second becomes `20230115_143022_1.jpg`.

---

## 6. Choosing which files to process

Every command except `apply` and `completions` takes one or more paths. A path can be a file, a folder, or a wildcard.

```powershell
pic-killer show .\photo.jpg
pic-killer show .\photos
pic-killer show .\photos -r
pic-killer show ".\photos\IMG_*.jpg"
```

| Form | What it means |
|------|---------------|
| A file | Always processed, even if its extension is not in `--ext` |
| A folder | One level deep by default; add `-r` to recurse |
| A wildcard | Expanded by PIC-Killer itself, one directory level, `*` and `?` |
| Several paths | Combined, de-duplicated, and sorted |

Only files whose extension is in `--ext` are picked up from folders. The default list is `jpg,jpeg,png,tif,tiff,webp`, so widen it when you need to:

```powershell
pic-killer show .\photos -r --ext jpg,jpeg,heic,avif
```

> Quote your wildcards on Windows. PowerShell and `cmd` do not expand `*` for external programs, so PIC-Killer expands it internally — but only if the shell hands the pattern over unexpanded.

---

## 7. Only the files that need it

`--where` filters by metadata, so you can say "only the photos that are missing GPS" instead of sorting them by hand. It works on every command that takes paths.

```powershell
pic-killer show .\photos -r --where no-gps
pic-killer time .\photos -r --where no-date --from-name
pic-killer set .\photos -r --where make=Canon --artist "Zhang San"
```

| Condition | Matches |
|-----------|---------|
| `has-gps` / `no-gps` | Photos with / without GPS coordinates |
| `has-date` / `no-date` | Photos with / without a capture time |
| `has-xmp` / `no-xmp` | Photos with / without an XMP packet |
| `has:name` / `no:name` | A named tag is present / absent, across EXIF, XMP and IPTC |
| `name=value` / `name!=value` | Tag value equals / does not equal |
| `name~value` / `name!~value` | Tag value contains / does not contain |

Combine conditions with `&&` (all must match) or `||` (any may match):

```powershell
pic-killer show .\photos -r --where "no-gps && make=Canon"
pic-killer show .\photos -r --where "make=Canon || make=Nikon"
```

> **Do not mix `&&` and `||` in one expression.** The parser splits on only one of them, so an expression containing both is rejected with an error. Run two commands instead.

> **Check your spelling before using `!=` or `!~`.** These are negations, so a name matching no tag at all would match *every* file. PIC-Killer refuses such a filter — `camera!=Canon` aborts, because no tag is called *camera*. A real name that some files lack is fine: `make!=Canon` matches other makes and photos with no make. Even so, on a destructive batch it costs nothing to run the same `--where` under `show` first and confirm the count.

---

## 8. Reading the output

Write commands print a header, one line per file, and a summary.

```
[OK]   C:\photos\a.jpg
[跳过] C:\photos\b.jpg  (无可解析的原始拍摄时间，偏移模式跳过)
[失败] C:\photos\c.jpg  (不支持的文件类型)

已修改 1，跳过 1，失败 1，共 3 个文件。
```

`[OK]` means changed, `[跳过]` means deliberately skipped with the reason in brackets, `[失败]` means the file could not be processed. A skip is normal and safe — it usually means the file did not meet the command's precondition. The exit code tells a script what happened:

| Exit code | Meaning |
|-----------|---------|
| `0` | Everything succeeded, or nothing needed doing |
| `1` | The command itself failed — bad arguments, unreadable GPX, unparseable `--where` |
| `2` | The command ran, but at least one file failed (for `verify`, at least one problem was found) |

Results go to standard output; the `--where` filter summary and the progress bar go to standard error. That means `show --json` and `show --csv` can be redirected to a file without contamination.

---

## 9. Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Chinese shows as garbage in the console | Console is not UTF-8 | Run `chcp 65001` |
| `未找到符合条件的图片文件。` | No file matched the path, extension filter, or `--where` | Check the path spelling; widen `--ext`; test the filter with `show` |
| `错误：以下路径不存在` | You named a path that is not on disk | Check the spelling. The run stops before touching anything, so nothing was half-processed |
| The command printed `已取消。` and did nothing | No `-y`, and no terminal to answer the prompt | Add `-y` |
| `BMP 无元数据容器，建议先转成 PNG 再处理` | BMP and GIF cannot store metadata | Convert to PNG or JPEG first |
| A RAW file (CR2, NEF, ARW) cannot be written | The EXIF engine does not parse RAW containers | Use `pic-killer xmp <file> --sidecar …`, which writes a separate `.xmp` and never touches the RAW |

---

## 10. Where to go next

[The User Manual](USER-MANUAL.md) documents every command, every option, the full `--where` grammar, the metadata field tables, the CSV import format, and the exact safety guarantees.

Two things worth setting up once:

```bash
pic-killer completions powershell | Out-String | Invoke-Expression
pic-killer completions --man > pic-killer.1
```

And one habit worth keeping: on any batch you have not run before, do it in three steps — `show --where …` to check the selection, then the same command with `-n`, then the real run with `--backup`.
