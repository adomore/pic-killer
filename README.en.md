# PIC-Killer

[![CI](https://github.com/adomore/pic-killer/actions/workflows/ci.yml/badge.svg)](https://github.com/adomore/pic-killer/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/adomore/pic-killer?logo=github)](https://github.com/adomore/pic-killer/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/adomore/pic-killer/total?logo=github)](https://github.com/adomore/pic-killer/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**English** | [中文](README.md)

**A Swiss army knife for photo metadata** — a command-line tool written in Rust.

Batch-edit photo EXIF metadata losslessly: capture time, author and copyright, camera and lens, GPS position, orientation, plus inspection and removal.

Only the metadata segments are rewritten. **The image is never re-encoded**, so the compressed pixel data is bit-for-bit unchanged — lossless in the literal sense.

Supported formats: JPEG / PNG / TIFF / WebP / HEIC / AVIF / JXL.

---

## Commands at a glance

| Subcommand | What it does |
|--------|------|
| [`time`](#time--change-capture-time) | Change capture time: fixed value / relative shift / sequence / extracted from the filename |
| [`show`](#show--inspect-metadata) | Inspect or export metadata (table / JSON / CSV) |
| [`set`](#set--set-common-tags) | Set common tags: artist, copyright, description, camera, lens, orientation and more |
| [`gps`](#gps--location) | Set or clear the GPS position |
| [`strip`](#strip--remove-metadata) | Remove metadata for privacy: everything / GPS only |
| [`rotate`](#rotate--lossless-rotation) | Lossless rotation marker: compose a rotation or mirror onto the existing orientation |
| [`copy`](#copy--copy-metadata) | Copy metadata from one reference photo onto a batch |
| [`rename`](#rename--rename-by-capture-time) | Rename files in bulk by capture time (the inverse of `--from-name`) |
| [`xmp`](#xmp--read-and-write-xmp) | Read and write XMP: title/description/creator/rating/keywords/city (JPEG and PNG) |
| [`iptc`](#iptc--read-and-write-iptc-iim) | Read and write legacy IPTC-IIM: title/caption/keywords/creator/city/copyright (JPEG) |
| [`restore`](#restore--restore-from-a-backup) | Restore a file from its `.bak` backup, undoing an earlier edit |
| [`geotag`](#geotag--gpx-geotagging) | Geotag a batch from a GPX track by capture time (interpolated GPS) |
| [`apply`](#apply--import-from-csv) | Import metadata from CSV and write it back (the write-back end of bulk spreadsheet edits) |
| [`report`](#report--metadata-statistics) | Summarise metadata coverage over a batch (capture time/GPS/camera mix/time span) |
| [`verify`](#verify--metadata-health-check) | Health check: find metadata problems (corruption/out-of-range coordinates/odd times/inconsistent fields) |
| `completions` | Generate a shell completion script (bash/zsh/fish/powershell) or a man page |

## Documentation

The full documentation lives in [`docs/`](docs/), and every document exists in both English and Chinese:

| Document | What it is |
|------|------|
| [Getting Started](docs/GETTING-STARTED.md) | A 20-minute introduction: install, the safety net, five real tasks |
| [User Manual](docs/USER-MANUAL.md) | Complete reference: all 16 commands, every option, field tables, format support |
| [Audit Report](docs/AUDIT.md) | Engineering audit: architecture, findings, security, test coverage |
| [Feature Research](docs/FEATURE-RESEARCH.md) | Competitive positioning, capability gaps, candidate features |

## Download and install

### Prebuilt binaries (recommended)

Go to the **[Releases page](https://github.com/adomore/pic-killer/releases/latest)**, download the archive for your platform, and unpack it to get a single executable:

| Platform | File |
|------|----------|
| Windows x64 | `pic-killer-<version>-x86_64-pc-windows-msvc.zip` |
| macOS (Apple Silicon) | `pic-killer-<version>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `pic-killer-<version>-x86_64-apple-darwin.tar.gz` |
| Linux x64 | `pic-killer-<version>-x86_64-unknown-linux-gnu.tar.gz` (static build: `…-musl`) |
| Linux ARM64 | `pic-killer-<version>-aarch64-unknown-linux-gnu.tar.gz` |

Every release ships a `SHA256SUMS.txt` so you can verify what you downloaded:

```bash
sha256sum -c SHA256SUMS.txt
```

### Building from source

Requires Rust 1.88+ (the code uses let-chains):

```bash
git clone https://github.com/adomore/pic-killer.git
cd pic-killer
cargo build --release          # output: target/release/pic-killer
cargo install --path .         # or install straight onto PATH
```

## General notes

Every subcommand accepts one or more **files, directories or wildcards** as its target:

- `-r, --recursive` descend into subdirectories (a directory is otherwise processed one level deep)
- `--ext <list>` which extensions to process (default `jpg,jpeg,png,tif,tiff,webp`)
- `--where <condition>` select files by metadata (see below)
- **Built-in wildcards**: `pic-killer set "*.jpg" ...` — cmd and PowerShell on Windows do not expand `*` for external programs, so this tool expands `*` and `?` itself

### The `--where` filter

Any command can use `--where` to process only the files that match a condition — "add GPS only to the photos that lack it", for example:

```powershell
pic-killer show .\photos -r --where no-gps          # list only the ones without GPS
pic-killer time .\photos -r --where no-date --from-name   # fill capture time only where it is missing
pic-killer set  .\photos -r --where make=Canon --artist 张三   # touch only what a Canon shot
```

Supported conditions (case-insensitive):

| Condition | Meaning |
|------|------|
| `has-gps` / `no-gps` | GPS position present / absent |
| `has-date` / `no-date` | Capture time present / absent |
| `has-xmp` / `no-xmp` | XMP present / absent |
| `has:name` / `no:name` | A tag with that name is present / absent (across EXIF/XMP/IPTC) |
| `name=value` `name!=value` | Tag value equals / does not equal |
| `name~value` `name!~value` | Tag value contains / does not contain |

Several conditions can be combined with `&&` (all must hold) or `||` (any may hold), for example
`--where "no-gps && make=Canon"` or `--where "make=Canon || make=Nikon"`.
The two cannot be mixed; mixing them is an error. Note also that `!=` / `!~` / `no:` are negations, so naming
a tag that does not exist (`camera!=Canon`, say) would match every file — that too is rejected. See
[User Manual §6](docs/USER-MANUAL.md).

The writing subcommands (`time`/`set`/`gps`/`strip`/`rotate`/`copy`/`xmp`/`iptc`/`geotag`) additionally accept:

- `-n, --dry-run` preview only, write nothing
- `--backup` copy each file to `<name>.bak` before processing
- `-y, --yes` skip the confirmation prompt
- `-v, --verbose` more detailed output
- `-j, --jobs <N>` worker threads (default: one per CPU core, `1` = sequential)

> **Parallelism and the progress bar**: these commands are multi-threaded by default, which is noticeably
> faster on a large library. A progress bar is drawn in a terminal (on stderr, so it never disturbs the
> stdout of `show --json/--csv`). Per-file writes are independent, and results still print in the original order.

---

## `time` · change capture time

```powershell
# set an absolute time
pic-killer time .\photos --set "2024-01-01 12:00:00"

# relative shift (fixing a camera clock or time zone) — any combination of +1y2mo3d4h5m6s
pic-killer time .\photos --shift "+2h" -r
pic-killer time .\photos --shift "-3d"

# sequence mode (forces an order), +1 minute per photo from the first
pic-killer time .\photos --sequential "2021-01-01 08:00:00" --interval "+1m"

# extract the date from the filename (IMG_20230115_143022.jpg, 2022-07-04 09.15.00.jpg, …)
pic-killer time .\photos --from-name
```

Shift units (case-insensitive, combinable): `y` years, `mo` months, `w` weeks, `d` days, `h` hours, `m` minutes, `s` seconds.
Note that `m` means **minutes** and `mo` means **months**; months and years follow the calendar.

Other options:
- `--tags <list>` which time fields to write: `original,digitized,modify` (default: all)
- `--tz <offset>` also write the time-zone offset tags (OffsetTime*), e.g. `--tz +08:00`
- `--also-file-time` set the filesystem modification time to the capture time as well

## `show` · inspect metadata

```powershell
# human-readable table
pic-killer show .\photo.jpg

# only tags whose name contains gps
pic-killer show .\photo.jpg --filter gps

# export as JSON or CSV (for scripts or spreadsheets)
pic-killer show .\photos -r --json > meta.json
pic-killer show .\photos -r --csv  > meta.csv
```

If the file carries GPS, an extra line prints the decimal latitude, longitude and altitude.

## `set` · set common tags

```powershell
pic-killer set .\photos --artist "张三" --copyright "© 2024 张三"
pic-killer set .\photos --make "Canon" --model "EOS R5" --lens-model "RF 24-70"
pic-killer set .\photo.jpg --description "海边日落" --user-comment "备注"

# orientation (the lossless rotation marker): normal / cw90 / ccw90 / 180 / mirror-h / mirror-v, or 1-8
pic-killer set .\photo.jpg --orientation cw90

# set any string or numeric tag generically (repeatable)
pic-killer set .\photo.jpg --set-string "OwnerName=Zhang" --set-string "Software=PicKiller"
pic-killer set .\photo.jpg --set-string "iso=100" --set-string "fnumber=2.8" --set-string "exposuretime=1/200"

# remove specific tags (repeatable)
pic-killer set .\photo.jpg --remove artist --remove copyright
```

Dedicated options for the common tags: `--artist` `--copyright` `--description` `--software`
`--make` `--model` `--lens-model` `--user-comment` `--owner` `--orientation`.

## `gps` · location

```powershell
# set coordinates (decimal degrees: north and east positive, south and west negative), altitude optional
pic-killer gps .\photo.jpg --lat 39.9042 --lon 116.4074 --alt 50

# clear GPS
pic-killer gps .\photos -r --clear
```

## `strip` · remove metadata

```powershell
# remove all metadata (for privacy)
pic-killer strip .\photos -r

# remove only the GPS position, keep everything else
pic-killer strip .\photos -r --gps
```

## `rotate` · lossless rotation

Composes a rotation or mirror onto the photo's **existing** orientation (correctly combined, not simply overwritten).
This is lossless: only the EXIF orientation marker changes, never the pixels.

```powershell
pic-killer rotate .\photo.jpg --cw        # 90° clockwise
pic-killer rotate .\photo.jpg --ccw       # 90° counter-clockwise
pic-killer rotate .\photo.jpg --r180      # 180°
pic-killer rotate .\photo.jpg --flip-h    # mirror horizontally
pic-killer rotate .\photo.jpg --flip-v    # mirror vertically
pic-killer rotate .\photos -r --reset     # reset to normal
```

> `set --orientation` sets the orientation code **absolutely**; `rotate` composes **relative** to the current one,
> so a `--cw` on a photo already rotated 90° clockwise leaves it at 180°.

## `copy` · copy metadata

Copy metadata from one reference photo onto a batch — giving a whole burst the same time or place, for instance.

```powershell
# by default everything copyable is copied (fields bound to the specific image, such as size and orientation, are skipped)
pic-killer copy .\burst\*.jpg --from .\reference.jpg

# copy only the capture time, or only the GPS
pic-killer copy .\photos -r --from .\ref.jpg --time
pic-killer copy .\photos -r --from .\ref.jpg --gps
```

## `rename` · rename by capture time

Rename in bulk by capture time (the inverse of `time --from-name`). Collisions get a numeric suffix; files without a capture time are skipped.

```powershell
# default template %Y%m%d_%H%M%S → 20230115_143022.jpg
pic-killer rename .\photos -r

# custom template (strftime syntax, no extension)
pic-killer rename .\photos --pattern "%Y-%m-%d_%H.%M.%S"

# preview first
pic-killer rename .\photos -r --dry-run
```

## `xmp` · read and write XMP

Read and write XMP metadata — cameras, Lightroom and phones commonly keep title, rating, keywords and copyright here
(modern **IPTC Core** is XMP-based too). **JPEG and PNG are supported** (PNG stores it in an iTXt chunk).

XMP is a metadata block independent of EXIF, and the two do not interfere. Writing XMP **preserves every existing property
it does not recognise**, changing only the ones you name.

```powershell
# set title/description/creator/rating/keywords
pic-killer xmp .\photo.jpg --title "西湖日出" --description "清晨的西湖" `
  --creator "张三" --creator "李四" --rating 5 --keywords "风景,西湖,日出"

# city/country and a colour label
pic-killer xmp .\photo.jpg --city 杭州 --country 中国 --label 红色

# set any property generically (prefix:name=value), or remove one
pic-killer xmp .\photo.jpg --set "photoshop:Headline=头条" --remove dc:description

# clear the whole XMP packet
pic-killer xmp .\photos -r --clear
```

Dedicated options: `--title` `--description` `--creator` (repeatable) `--rights` `--rating` (0-5)
`--label` `--keywords` (comma-separated) `--city` `--country`. Use `show` to read them back (it lists the XMP block separately).

**Sidecar `.xmp` (RAW support)**: add `--sidecar` and the data goes to a companion `<stem>.xmp` file, **leaving the original untouched**.
Because the original is never parsed, this works even for RAW formats `little_exif` cannot read (CR2/NEF/ARW and friends) — it is exactly the sidecar
Lightroom and darktable read. `show` picks up and displays a sidecar automatically (including when the original itself cannot be read).

```powershell
pic-killer xmp shot.CR2 --sidecar --title 日出 --rating 5   # writes shot.xmp, leaves the CR2 alone
pic-killer xmp shot.CR2 --sidecar --clear                    # deletes shot.xmp
```

## `iptc` · read and write IPTC-IIM

Read and write legacy **IPTC-IIM** metadata (stored in a JPEG's APP13 / Photoshop 8BIM block, common in news and stock workflows).
**JPEG only.** Values are written as UTF-8 and the other 8BIM resource blocks (thumbnail, colour profile, …) are preserved.

```powershell
# set title/caption/keywords/creator/city/copyright
pic-killer iptc .\photo.jpg --title "开幕式" --description "现场" `
  --keywords "体育,开幕" --creator "记者甲" --city 北京 --copyright "© 新华社"

# generic set (by field name or record:dataset), remove, clear
pic-killer iptc .\photo.jpg --set "2:105=头条" --remove keywords
pic-killer iptc .\photos -r --clear
```

Dedicated options: `--title` `--description` `--keywords` `--creator` (repeatable) `--headline`
`--city` `--state` `--country` `--copyright` `--credit` `--source` `--instructions`.

> EXIF, XMP and IPTC are three independent systems that can coexist in one JPEG, and this tool guarantees it will not damage any of them.

## `restore` · restore from a backup

The counterpart to `--backup`: put the `<name>.bak` produced earlier back in place, undoing an edit in one step.

```powershell
# edit, with a backup
pic-killer set .\photo.jpg --artist 张三 --backup

# changed your mind — restore (the .bak is removed afterwards by default)
pic-killer restore .\photo.jpg

# keep the .bak so you can restore repeatedly
pic-killer restore .\photos -r --keep-backup

# preview which files have a backup to restore
pic-killer restore .\photos -r --dry-run
```

Files without a `.bak` are skipped; the restore is byte-for-byte (a `.bak` is a complete copy of the original).

## `geotag` · GPX geotagging

Fill in GPS from a recorded track (`.gpx`) by capture time — the classic workflow when a phone or watch logged the route and the camera has no GPS.
Latitude, longitude (and altitude) are **linearly interpolated** in the track at each photo's capture moment.

```powershell
# the camera runs on Beijing time, the track is UTC: name the camera's zone with --tz
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00

# only fill photos that have no GPS yet (with --where)
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00 --where no-gps

# camera clock 30 seconds slow? correct it with --offset
pic-killer geotag .\photos --gpx .\track.gpx --offset +30s
```

Options:
- `--gpx <file>` the GPX track (required)
- `--tz <offset>` the camera clock's time zone, e.g. `+08:00` (capture times are otherwise read in the **system local zone**)
- `--offset <offset>` an extra time offset correcting camera-clock error, e.g. `-5m`
- `--max-gap <seconds>` skip a photo whose nearest track point is further away in time than this (default 600)

> Capture times are local and GPX times are usually UTC, so the two have to be aligned — which is what makes `--tz` matter.
> Photos with no capture time, or falling in a gap in the track (beyond `--max-gap`), are skipped safely.

## `apply` · import from CSV

Write metadata back in bulk from a three-column `file,field,value` CSV — pair it with `show --csv --for-apply`
and you get "export → bulk-edit in a spreadsheet → write back". Several fields of one file are merged into a single write (parallel, atomic).

```powershell
pic-killer show .\photos -r --csv --for-apply > meta.csv
pic-killer apply --from meta.csv
```

> Remember the `--for-apply`. Without it, `show --csv` is a **dump** format (five columns, including group and hex code,
> with GPS split into component tags) and cannot be fed back into `apply`.

A sample CSV (quote values containing commas; a header row is optional and skipped automatically; Excel's UTF-8 BOM is tolerated):

```csv
file,field,value
photo1.jpg,artist,张三
photo1.jpg,datetimeoriginal,2023-03-03 12:00:00
photo1.jpg,gps,"31.23,121.47,15"
photo1.jpg,iso,200
photo1.jpg,xmp:title,西湖日出
photo1.jpg,xmp:rating,5
photo1.jpg,iptc:city,杭州
```

Prefix a field name with `xmp:` / `iptc:` to write into that system (EXIF/XMP/IPTC can be mixed in one CSV; per file, each system is written once).

Supported fields (EXIF): `datetimeoriginal`/`createdate`/`modifydate`/`alldates`,
string tags such as `artist` `copyright` `description` `software` `make` `model` `lensmodel` `owner`,
numeric and rational tags such as `iso` `fnumber` `exposuretime` (`1/200` accepted) `focallength` `exposurecompensation`,
plus `orientation`, `usercomment`, `gps` (value `lat,lon[,alt]`) and `gps.clear`.

## `report` · metadata statistics

Scan a batch read-only and summarise metadata coverage — a quick answer to "how many lack GPS or a capture time, and which cameras shot these".

```powershell
pic-killer report .\photos -r
pic-killer report .\photos -r --where no-gps   # --where works here too
```

The output covers: total count, with/without capture time, with/without GPS, time span, and the distribution of camera models.

## `verify` · metadata health check

Check a batch read-only for metadata **problems**, split into two levels — "problems" (errors) and "warnings":

```powershell
pic-killer verify .\photos -r
```

What it checks: corrupt or unreadable metadata, out-of-range GPS coordinates, malformed capture-time strings, capture times in the future,
`DateTimeOriginal` disagreeing with `CreateDate`, and out-of-range orientation values (outside 1-8). A non-zero exit code is returned when
problems are found, which makes it usable as a script or CI gate.

## shell completion / man page

```powershell
# PowerShell: add to $PROFILE
pic-killer completions powershell | Out-String | Invoke-Expression
```
```bash
# bash: write into the completion directory
pic-killer completions bash > /etc/bash_completion.d/pic-killer
pic-killer completions --man > pic-killer.1        # the man page
```
`bash` / `zsh` / `fish` / `powershell` / `elvish` are supported.

---

## Why it is lossless

A JPEG is a series of "segments": EXIF lives in an APP1 segment, while the actual image pixels are a separate stream of compressed scan data.
This tool only deletes the old metadata segments and inserts new ones, **never touching the scan data**, so there is no recompression
and no quality loss whatsoever. Decode the photo before and after with any image library and the pixels are identical.

## Safety design

- **Atomic writes**: a temporary file is written and fsynced, then atomically renamed over the original, so an interrupted batch cannot corrupt a file.
- **File timestamps preserved by default**: writing creates a new file, but the original mtime/atime are restored unless `--also-file-time` says otherwise.
- **Previewable and backup-able**: `--dry-run` to look before you leap, `--backup` to keep the original.

## Known limitations

- **Metadata round trip**: the underlying EXIF library (`little_exif`) recognises dozens of common tags (time, Make, Model,
  Artist, GPS, aperture and shutter, …) and preserves them all; a very small number of obscure tags it does not know
  (parts of some vendors' MakerNote) may be lost when the file is rewritten. The image pixels are always lossless.
- **HEIC / AVIF / JXL**: supported by the underlying library, but less mature than JPEG — try `--backup` or `--dry-run` first.
- **WebP**: only lossless and extended WebP are supported.
- **XMP**: JPEG and PNG are supported; XMP in TIFF/WebP/HEIC is not implemented yet.
- **IPTC-IIM**: JPEG (APP13) is supported; values are written as UTF-8.
- **Thumbnail replacement** is not supported yet (it involves IFD1 data offsets and is comparatively risky).
- If Chinese text appears garbled in an older Windows console, run `chcp 65001` to switch it to UTF-8.

## Licence

MIT
