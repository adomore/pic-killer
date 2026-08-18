# PIC-Killer — User Manual

> Complete reference for every command, option, field name and guarantee. If you have never used PIC-Killer, read [Getting Started](GETTING-STARTED.md) first.

**English** | [中文](USER-MANUAL.zh.md)

---

## 1. About this manual

This manual documents PIC-Killer 1.0.0. Everything here was checked against the source and against the running binary; where behaviour is surprising it is described as it actually is, not as it ought to be.

Cross-references use section numbers (§5, §8.3) rather than links, because section numbers are identical in the English and Chinese editions.

Conventions used throughout:

- Paths in examples use PowerShell style (`.\photos`). On Linux and macOS use `./photos`.
- `<角括号>` in a synopsis marks a value you supply; `[方括号]` marks something optional.
- "Write commands" means the ten commands that modify image files: `time`, `set`, `gps`, `strip`, `rotate`, `copy`, `xmp`, `iptc`, `geotag`, `apply`.

---

## 2. Core concepts

**Lossless editing.** A JPEG is a sequence of segments; EXIF lives in an APP1 segment and the picture itself is a separate block of compressed scan data. PIC-Killer removes the old metadata segment and inserts a new one. The scan data is never read, decoded, or re-encoded, so there is no generation loss. Decode the file before and after and the pixels are bit-identical.

**Three metadata systems.** A single JPEG can carry EXIF, XMP and IPTC-IIM simultaneously. They are stored in different segments and describe overlapping but distinct things. PIC-Killer edits each through its own command (`set`/`time`/`gps` for EXIF, `xmp` for XMP, `iptc` for IPTC) and never damages the other two while writing one.

**Atomic writes.** No command edits a file in place. Each write produces a complete new file in a temporary location and then atomically replaces the original. See §13.

**Skip, don't guess.** When a file does not meet a command's precondition — no capture time to shift, no date in the filename, no track point near enough — PIC-Killer skips it and says why. It never invents a value.

---

## 3. Installation

Prebuilt binaries are published for six targets on the [Releases page](https://github.com/adomore/pic-killer/releases/latest). Each archive contains one self-contained executable plus the README and licence.

| Platform | Archive |
|----------|---------|
| Windows x64 | `pic-killer-<version>-x86_64-pc-windows-msvc.zip` |
| macOS (Apple Silicon) | `pic-killer-<version>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `pic-killer-<version>-x86_64-apple-darwin.tar.gz` |
| Linux x64 (glibc) | `pic-killer-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux x64 (static) | `pic-killer-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `pic-killer-<version>-aarch64-unknown-linux-gnu.tar.gz` |

Verify the download against the published checksums, then build from source only if you need to:

```bash
sha256sum -c SHA256SUMS.txt
git clone https://github.com/adomore/pic-killer.git
cd pic-killer
cargo build --release
cargo install --path .
```

The crate declares `edition = "2024"`, which requires Rust 1.85 or newer.

---

## 4. Command-line structure

```
pic-killer <command> [options] <paths...>
```

Every command takes its own options plus, depending on its kind, the shared file-selection options of §5 and the shared write options of §7.

| Kind | Commands | Takes §5 | Takes §7 |
|------|----------|----------|----------|
| Write | `time` `set` `gps` `strip` `rotate` `copy` `xmp` `iptc` `geotag` | yes | yes |
| Write, no paths | `apply` | no | yes |
| Read-only | `show` `report` `verify` | yes | no |
| File operation | `rename` `restore` | yes | partly — `-n`, `-y`, `-v` only |
| Utility | `completions` | no | no |

---

## 5. Selecting files

These options are accepted by every command that takes paths.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `<paths...>` | file, directory or wildcard | — | Required; one or more |
| `-r`, `--recursive` | — | off | Descend into subdirectories |
| `--ext` | comma-separated list | `jpg,jpeg,png,tif,tiff,webp` | Extensions to pick up from directories |
| `--where` | expression | — | Filter by metadata; see §6 |

Selection rules:

- A path naming a **file** is always processed, even if its extension is not in `--ext`. Naming a file explicitly is taken as intent.
- A path naming a **directory** is expanded one level deep; `-r` makes it fully recursive. Only files whose extension is in `--ext` are collected.
- A path containing `*` or `?` is expanded by PIC-Killer itself, because `cmd` and PowerShell do not expand wildcards for external programs. Expansion covers one directory level and ignores `--ext`, since the pattern is already the filter.
- Results from all paths are combined, de-duplicated and sorted in **natural order**: runs of digits compare numerically, so `IMG_2.jpg` precedes `IMG_10.jpg`. This ordering is what makes `time --sequential` both reproducible and correct for numbered photos.
- A path you name that does not exist is an error, not a silent skip: the run stops before touching anything and lists the offending paths. A wildcard matching nothing is not an error, since a pattern legitimately matches zero files.

```powershell
pic-killer show .\photo.jpg
pic-killer show .\photos -r --ext jpg,jpeg,heic
pic-killer show ".\photos\IMG_*.jpg"
pic-killer show .\a.jpg .\b.jpg .\more-photos
```

---

## 6. The `--where` filter

`--where` restricts a command to files whose metadata satisfies a condition. Tag lookup spans EXIF, XMP and IPTC. Names are matched case-insensitively and by **substring**, so `make` matches `Make` and `LensMake`.

| Syntax | Matches |
|--------|---------|
| `has-gps` / `no-gps` | GPS coordinates present / absent |
| `has-date` / `no-date` | Capture time present / absent |
| `has-xmp` / `no-xmp` | XMP packet present / absent |
| `has:name` / `no:name` | A tag whose name contains `name` is present / absent |
| `name=value` | Some matching tag's value equals `value` |
| `name!=value` | No matching tag's value equals `value` |
| `name~value` | Some matching tag's value contains `value` |
| `name!~value` | No matching tag's value contains `value` |

Conditions combine with `&&` (all must hold) or `||` (any may hold):

```powershell
pic-killer show .\photos -r --where "no-gps && make=Canon"
pic-killer show .\photos -r --where "make=Canon || make=Nikon"
pic-killer geotag .\photos -r --gpx .\t.gpx --tz +08:00 --where no-gps
```

> **`&&` and `||` cannot be mixed.** The parser splits on one operator only, so an expression containing both is rejected with an error naming the offending half. Split the work into two commands instead. Earlier versions accepted such an expression and silently returned the wrong file set.

> **A negative comparison against an unknown tag name is refused.** `!=`, `!~` and `no:` are negations, so a name that matches no tag at all would be true for every file — `--where "camera!=Canon"` would select everything, because no tag is called *camera*. Before filtering, PIC-Killer checks each such name against the whole selection and aborts if it matches nothing anywhere, naming the suspect name. A name that exists but is absent from a particular file still behaves normally: `make!=Canon` matches files with a different make *and* files with no make at all.

---

## 7. Options shared by write commands

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--backup` | — | off | Copy each file to `<name>.bak` before writing |
| `-n`, `--dry-run` | — | off | Report what would change; write nothing |
| `-y`, `--yes` | — | off | Skip the confirmation prompt |
| `-v`, `--verbose` | — | off | Print a settings summary before the batch |
| `-j`, `--jobs` | integer | `0` | Worker threads: `0` = one per CPU core, `1` = sequential |

Notes on each:

- `--backup` never overwrites an existing `.bak`. After two backed-up edits, `restore` returns the file to its **pristine original**, not to the state after the first edit. This is deliberate — the first backup is the only one guaranteed to be untouched by PIC-Killer.
- `-n` short-circuits inside the write path, after all parsing and metadata reading, so a dry run exercises the same code that a real run does and reports the same skips and failures.
- Without `-y`, write commands prompt for confirmation on standard input, writing the prompt to standard error so it stays visible when standard output is redirected. **When standard input is not a terminal — a script, a pipeline, CI — the command fails with an error rather than prompting.** Pass `-y` in automation, or `-n` to preview. Earlier versions read end-of-input, treated it as "no", and exited 0 having changed nothing, which made a broken automation look successful.
- `-j` builds a dedicated thread pool for values other than `0` and `1`. Because each file is written independently to its own temporary file, parallel writing is safe.
- `rename` and `restore` accept `-n`, `-y` and `-v` but not `--backup` or `-j`.

---

## 8. Command reference

### 8.1 `time` — change capture time

Sets, shifts, sequences or derives the capture time.

```
pic-killer time [options] <--set <time>|--shift <delta>|--sequential <start>|--from-name> <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--set` | date-time | — | Set an absolute time, e.g. `"2024-01-01 12:00:00"` |
| `--shift` | delta | — | Offset the existing time, e.g. `+2h`, `-3d`, `+1y2mo` |
| `--sequential` | date-time | — | Give the first file this time, then step by `--interval` |
| `--interval` | delta | `+1s` | Step between files in sequential mode |
| `--from-name` | — | off | Parse the date out of the filename |
| `--tags` | comma-separated | `original,digitized,modify` | Which time fields to write |
| `--tz` | offset | — | Also write `OffsetTime*` tags, e.g. `+08:00` |
| `--also-file-time` | — | off | Also set the filesystem modification time |

**Behavior**

- Exactly one of `--set`, `--shift`, `--sequential`, `--from-name` is required; they are mutually exclusive.
- `--tags` maps to EXIF as: `original` → `DateTimeOriginal`, `digitized` → `CreateDate`, `modify` → `ModifyDate`.
- `--shift` skips a file that has no parsable existing capture time — there is nothing to offset from.
- `--from-name` skips a file whose name contains no recognisable date. A name with a date but no time is given midnight.
- `--sequential` walks files in the natural sorted order of §5, so numbered photos increment in the order a person would expect.
- Offset units are `y` years, `mo` months, `w` weeks, `d` days, `h` hours, `m` minutes, `s` seconds; see §9.6. Months and years are calendar arithmetic.

**Examples**

```powershell
pic-killer time .\photos --set "2024-01-01 12:00:00"
pic-killer time .\photos -r --shift "+2h"
pic-killer time .\photos --shift "-1d12h30m"
pic-killer time .\photos --sequential "2021-01-01 08:00:00" --interval "+1m"
pic-killer time .\photos --from-name --where no-date
pic-killer time .\photo.jpg --set "2023-06-15 18:00:00" --tz +08:00 --also-file-time
```

> `m` is minutes and `mo` is months. `--shift "+6m"` moves the photo forward six minutes, not six months.

### 8.2 `show` — inspect metadata

Read-only. Prints or exports everything PIC-Killer can read.

```
pic-killer show [options] <paths...>
```

**Options** — plus §5.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--json` | — | off | Emit JSON; conflicts with `--csv` |
| `--csv` | — | off | Emit CSV, one row per tag |
| `--for-apply` | — | off | With `--csv`, emit the three columns `apply` reads instead of the full dump |
| `--filter` | keyword | — | Only tags whose name contains the keyword, case-insensitive |

**Behavior**

- Default output is a table per file: a decoded decimal GPS line if present, then EXIF tags, then an `--- XMP ---` block, then an `--- IPTC ---` block.
- A file with nothing readable prints `(无匹配的元数据)`.
- When nothing matches, `--json` prints `[]` and `--csv` prints the header alone; the human-readable notice goes to standard error so the output stays parseable.
- `--csv` alone is a *dump* format — it carries the IFD group and hex code, and splits GPS into its component tags, so it cannot be fed back to `apply`. Use `--csv --for-apply` for the round trip; see §10.
- Because the dump is meant to be opened in a spreadsheet, a value beginning with `=`, `+`, `-`, `@` or a tab is prefixed with an apostrophe so the spreadsheet does not evaluate it as a formula. `--for-apply` deliberately does **not** do this: it is an interchange format, and a southern-hemisphere GPS value legitimately starts with `-`.
- A sidecar `.xmp` is read and shown automatically, including for files whose own container cannot be parsed.
- `--filter` narrows the tag list in all three metadata systems. The decoded GPS line is *not* filtered — it is printed whenever the file has coordinates.
- Output formats are specified in §11.

**Examples**

```powershell
pic-killer show .\photo.jpg
pic-killer show .\photo.jpg --filter gps
pic-killer show .\photos -r --json > meta.json
pic-killer show .\photos -r --csv  > meta.csv
pic-killer show .\photos -r --where no-gps
```

### 8.3 `set` — set common tags

```
pic-killer set [options] <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--artist` | text | — | `Artist` |
| `--copyright` | text | — | `Copyright` |
| `--description` | text | — | `ImageDescription` |
| `--software` | text | — | `Software` |
| `--make` | text | — | `Make` |
| `--model` | text | — | `Model` |
| `--lens-model` | text | — | `LensModel` |
| `--user-comment` | text | — | `UserComment` |
| `--owner` | text | — | `OwnerName` |
| `--orientation` | keyword or 1–8 | — | Absolute orientation; see §9.5 |
| `--set-string` | `name=value` | — | Any supported tag by name; repeatable |
| `--remove` | name | — | Delete a tag by name; repeatable |

**Behavior**

- `--set-string` first tries the string tags of §9.1, then the numeric and rational tags of §9.2. Rational values accept `1/200` as well as decimals.
- Tag names are normalised: case, hyphens, underscores and spaces are ignored, so `lens-model`, `LensModel` and `lens model` are the same name.
- `--remove` only works for names that map to a known tag; an unknown name is an error rather than a silent no-op.
- `--orientation` sets the code absolutely. To rotate relative to the current value use `rotate` (§8.6).

**Examples**

```powershell
pic-killer set .\photos --artist "Zhang San" --copyright "(C) 2024 Zhang San"
pic-killer set .\photos --make "Canon" --model "EOS R5" --lens-model "RF 24-70"
pic-killer set .\photo.jpg --set-string "iso=100" --set-string "fnumber=2.8"
pic-killer set .\photo.jpg --set-string "exposuretime=1/200"
pic-killer set .\photo.jpg --remove artist --remove copyright
```

### 8.4 `gps` — set or clear location

```
pic-killer gps [options] <--lat <deg>|--clear> <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--lat` | decimal degrees | — | Latitude; north positive, south negative |
| `--lon` | decimal degrees | — | Longitude; east positive, west negative |
| `--alt` | metres | — | Altitude, optional |
| `--clear` | — | off | Remove all GPS tags |

**Behavior**

- At least one of `--lat` or `--clear` is required. `--lat` and `--lon` require each other.
- Decimal degrees are converted to the EXIF degrees/minutes/seconds rational form, with the hemisphere written to the `Ref` tags.
- `--clear` removes every GPS tag including `GPSVersionID`.
- Coordinates are written as given; out-of-range values are not rejected here, but `verify` (§8.15) reports them.

**Examples**

```powershell
pic-killer gps .\photo.jpg --lat 39.9042 --lon 116.4074
pic-killer gps .\photo.jpg --lat 30.2741 --lon 120.1551 --alt 12
pic-killer gps .\photos -r --clear
```

### 8.5 `strip` — remove metadata

```
pic-killer strip [options] <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--gps` | — | off | Remove only GPS; default removes everything |

**Behavior**

- Without `--gps`, **all three metadata systems are cleared in a single atomic write**: the EXIF segment, the XMP packet and the IPTC block. The per-file line reports which ones were present, e.g. `已清除 EXIF + XMP + IPTC`.
- With `--gps`, only *coordinates* are removed — EXIF GPS tags and XMP `exif:GPS*` properties. Textual place names such as `photoshop:City` or IPTC `City` are not coordinates and survive; use a full `strip` to remove those.
- A file that already has no metadata still reports `[OK]` rather than a skip; with `--gps`, a file with no coordinates anywhere is skipped.

**Examples**

```powershell
pic-killer strip .\to-publish -r
pic-killer strip .\to-publish -r --gps
pic-killer strip .\photos -r --where has-gps --backup
```

### 8.6 `rotate` — compose an orientation change

Adjusts the EXIF orientation flag relative to its current value. Pixels are untouched.

```
pic-killer rotate [options] <--cw|--ccw|--r180|--flip-h|--flip-v|--reset> <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--cw` | — | — | Rotate 90° clockwise |
| `--ccw` | — | — | Rotate 90° counter-clockwise |
| `--r180` | — | — | Rotate 180° |
| `--flip-h` | — | — | Mirror horizontally |
| `--flip-v` | — | — | Mirror vertically |
| `--reset` | — | — | Set orientation back to normal, and repair an out-of-range value |

**Behavior**

- Exactly one operation is required.
- The operation composes with the existing orientation. Applying `--cw` to a photo already marked 90° clockwise yields 180°, not 90°.
- Composition is a closed group over the eight EXIF orientation codes: `--cw` four times is the identity, `--cw` then `--ccw` is the identity, and any flip applied twice is the identity.
- A file with no orientation tag is treated as normal (code 1).

**Examples**

```powershell
pic-killer rotate .\photo.jpg --cw
pic-killer rotate .\photo.jpg --ccw
pic-killer rotate .\photos -r --r180
pic-killer rotate .\photos -r --reset
```

> `set --orientation` writes a code absolutely; `rotate` composes with what is already there. Use `set --orientation normal` and `rotate --reset` interchangeably, but never assume `rotate --cw` is idempotent.

### 8.7 `copy` — copy metadata from a reference photo

```
pic-killer copy [options] --from <reference> <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--from` | file | — | Required; the photo to read metadata from |
| `--time` | — | off | Copy capture-time fields only |
| `--gps` | — | off | Copy GPS only |
| `--all` | — | on | Copy everything copyable |

**Behavior**

- With no selector, `--all` is the default.
- `--time` and `--gps` may be combined to copy both and nothing else.
- Tags bound to the specific image — pixel dimensions, orientation and similar — are deliberately not copied, because they would describe the wrong picture.
- The reference file is read once and applied to every target.

**Examples**

```powershell
pic-killer copy ".\burst\*.jpg" --from .\reference.jpg
pic-killer copy .\photos -r --from .\ref.jpg --time
pic-killer copy .\photos -r --from .\ref.jpg --gps
pic-killer copy .\photos -r --from .\ref.jpg --time --gps
```

### 8.8 `rename` — rename by capture time

```
pic-killer rename [options] <paths...>
```

**Options** — plus §5; accepts `-n`, `-y`, `-v` from §7 but not `--backup` or `-j`.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--pattern` | strftime template | `%Y%m%d_%H%M%S` | Filename without extension |

**Behavior**

- Files with no capture time are skipped; the file is never renamed to a guessed name.
- The extension is preserved as-is; the pattern covers the stem only.
- If two files would produce the same name, the second gets `_1`, the third `_2`, and so on.
- This is the inverse of `time --from-name` (§8.1).

**Examples**

```powershell
pic-killer rename .\photos -r -n
pic-killer rename .\photos -r
pic-killer rename .\photos --pattern "%Y-%m-%d_%H.%M.%S"
pic-killer rename .\photos -r --where has-date
```

### 8.9 `xmp` — read and write XMP

XMP is where cameras, Lightroom and phones keep titles, ratings, keywords and rights. Modern IPTC Core is XMP-based.

```
pic-killer xmp [options] <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--title` | text | — | `dc:title` |
| `--description` | text | — | `dc:description` |
| `--creator` | text | — | `dc:creator`; repeatable |
| `--rights` | text | — | `dc:rights` |
| `--rating` | 0–5 | — | `xmp:Rating` |
| `--label` | text | — | `xmp:Label` |
| `--keywords` | comma-separated | — | `dc:subject` |
| `--city` | text | — | `photoshop:City` |
| `--country` | text | — | `photoshop:Country` |
| `--set` | `prefix:name=value` | — | Any property; repeatable |
| `--remove` | `prefix:name` | — | Delete a property; repeatable |
| `--clear` | — | off | Remove the entire XMP packet |
| `--sidecar` | — | off | Write `<stem>.xmp` and never touch the image |

**Behavior**

- Supported containers are JPEG (APP1) and PNG (iTXt). TIFF, WebP and HEIC are not supported for XMP.
- Writing preserves every property already in the packet that you did not name; only the properties you specify are added or replaced.
- `--rating` is validated to 0–5. `--set` validates the namespace prefix against §9.3 and rejects unknown ones.
- `--keywords` splits on commas. Note that the CSV import path of §10 splits on `;` or `|` instead.
- `--sidecar` writes a standalone `.xmp` next to the image. Because the image is never parsed, this works for RAW files (CR2, NEF, ARW) that the EXIF engine cannot read. `--sidecar --clear` deletes the sidecar, honouring `--backup`.
- The sidecar name comes from the stem, so `IMG_0001.CR2` and `IMG_0001.JPG` would both map to `IMG_0001.xmp`. Such collisions are detected before any file is touched and refused with an error; run the two extensions separately with `--ext`. Sidecar writes use the same atomic replace as every other write.

**Examples**

```powershell
pic-killer xmp .\photo.jpg --title "West Lake" --rating 5 --keywords "landscape,sunrise"
pic-killer xmp .\photo.jpg --creator "Zhang San" --creator "Li Si" --rights "(C) 2024"
pic-killer xmp .\photo.jpg --set "photoshop:Headline=Front page" --remove dc:description
pic-killer xmp .\photos -r --clear
pic-killer xmp .\shot.CR2 --sidecar --title "Sunrise" --rating 5
```

> `show` reads sidecars automatically, so after `--sidecar` you can verify with a plain `pic-killer show shot.CR2`.

### 8.10 `iptc` — read and write IPTC-IIM

Legacy IPTC-IIM in the JPEG APP13 / Photoshop 8BIM block, still standard in newsrooms and stock libraries.

```
pic-killer iptc [options] <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--title` | text | — | 2:05 Object Name |
| `--description` | text | — | 2:120 Caption |
| `--keywords` | comma-separated | — | 2:25 Keywords |
| `--creator` | text | — | 2:80 By-line; repeatable |
| `--headline` | text | — | 2:105 Headline |
| `--city` | text | — | 2:90 City |
| `--state` | text | — | 2:95 Province/State |
| `--country` | text | — | 2:101 Country |
| `--copyright` | text | — | 2:116 Copyright Notice |
| `--credit` | text | — | 2:110 Credit |
| `--source` | text | — | 2:115 Source |
| `--instructions` | text | — | 2:40 Special Instructions |
| `--set` | `name=value` or `2:105=value` | — | Any dataset; repeatable |
| `--remove` | name or `record:dataset` | — | Delete a dataset; repeatable |
| `--clear` | — | off | Remove the entire IPTC block |

**Behavior**

- JPEG only. Other formats are skipped.
- Values are written as UTF-8, and the character-set marker is added so readers interpret them correctly.
- Other 8BIM resource blocks — thumbnails, colour profiles, paths — are preserved untouched.
- Dataset names accept the aliases listed in §9.4, or an explicit `record:dataset` pair.

**Examples**

```powershell
pic-killer iptc .\photo.jpg --title "Opening" --city "Beijing" --keywords "sport,opening"
pic-killer iptc .\photo.jpg --creator "Reporter A" --credit "Agency" --copyright "(C) Agency"
pic-killer iptc .\photo.jpg --set "2:105=Front page" --remove keywords
pic-killer iptc .\photos -r --clear
```

### 8.11 `restore` — undo from a backup

```
pic-killer restore [options] <paths...>
```

**Options** — plus §5; accepts `-n`, `-y`, `-v` from §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--keep-backup` | — | off | Keep the `.bak` after restoring |

**Behavior**

- Restores `<name>.bak` over `<name>` byte for byte; the backup is a complete copy of the original file.
- Files with no `.bak` are excluded before processing rather than marked as skipped; the header reports how many backups were found out of how many files.
- The `.bak` is deleted after a successful restore unless `--keep-backup` is given.
- Because `--backup` never overwrites an existing `.bak`, restore always returns the file to its state before the *first* backed-up edit.

**Examples**

```powershell
pic-killer set .\photo.jpg --artist "Zhang San" --backup
pic-killer restore .\photo.jpg
pic-killer restore .\photos -r --dry-run
pic-killer restore .\photos -r --keep-backup
```

### 8.12 `geotag` — GPS from a GPX track

```
pic-killer geotag [options] --gpx <file> <paths...>
```

**Options** — plus §5 and §7.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--gpx` | file | — | Required; the GPX track |
| `--tz` | offset | system local | Timezone of the camera clock, e.g. `+08:00` |
| `--offset` | delta | — | Extra correction for camera clock error, e.g. `-5m` |
| `--max-gap` | seconds | `600` | Refuse to tag if the nearest track point is further away in time |

**Behavior**

- Each photo's capture time is converted to UTC and located on the track by linear interpolation between the two surrounding points; altitude is interpolated too when the track has it.
- `--tz` is the critical parameter: capture times are local wall-clock with no zone, GPX times are UTC. Without `--tz` the system's local timezone is assumed.
- A photo with no capture time is skipped.
- A photo whose time falls outside the track, or in a gap wider than `--max-gap`, is skipped rather than approximated.
- `--offset` is applied to the photo time before lookup, to compensate for a camera clock that ran fast or slow.

**Examples**

```powershell
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00
pic-killer geotag .\photos -r --gpx .\track.gpx --tz +08:00 --where no-gps
pic-killer geotag .\photos --gpx .\track.gpx --offset "+30s"
pic-killer geotag .\photos --gpx .\track.gpx --tz +08:00 --max-gap 120
```

> Combine with `--where no-gps` so photos that already carry a position from a GPS-equipped camera are left alone.

### 8.13 `apply` — import edits from CSV

Reads a three-column CSV and writes the values back. The write end of an export/edit/import round trip with `show --csv`.

```
pic-killer apply [options] --from <csv>
```

**Options** — §7 only. This command takes no paths, so §5 does not apply and neither does `--where`.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `--from` | file | — | Required; the CSV to read |

**Behavior**

- The CSV format, the field names and the value syntax are specified in §10.
- All rows for the same file are merged, so each file is opened and written once per metadata system, not once per field.
- Rows may name EXIF, XMP (`xmp:` prefix) and IPTC (`iptc:` prefix) fields in the same file; each system gets one write.
- A row naming a file that does not exist is reported as a failure, and the command exits 2.

**Examples**

```powershell
pic-killer show .\photos -r --csv > meta.csv
pic-killer apply --from .\edited.csv -n
pic-killer apply --from .\edited.csv --backup -y
```

### 8.14 `report` — summarise a library

Read-only. Answers "what am I dealing with?" for a folder of photos.

```
pic-killer report [options] <paths...>
```

**Options** — §5 only.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| — | — | — | No command-specific options |

**Behavior**

- Reports the total count, how many photos have and lack a capture time, how many have and lack GPS, the earliest and latest capture time, and the distribution of camera make and model.
- Photos with no camera information are grouped under `(无相机信息)`.
- Processing is sequential, not parallel.

**Examples**

```powershell
pic-killer report .\photos -r
pic-killer report .\photos -r --where no-gps
pic-killer report .\photos -r --ext jpg,jpeg,heic
```

### 8.15 `verify` — check for metadata problems

Read-only health check. Intended for scripts and release gates.

```
pic-killer verify [options] <paths...>
```

**Options** — §5 only.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| — | — | — | No command-specific options |

**Behavior**

Checks, at two severities:

- **Problem** — metadata corrupt (the EXIF block exists but cannot be parsed, reported with the underlying reason); GPS latitude outside ±90 or longitude outside ±180; capture time that cannot be parsed; orientation outside 1–8.
- **Warning** — capture time in the future; `DateTimeOriginal` and `CreateDate` disagreeing.
- A photo that simply has no EXIF is healthy, not a problem — only a block that is present and unparseable counts as corruption.
- BMP and GIF are not counted as problems, since they simply cannot hold metadata.
- Exits 2 when at least one problem is found; warnings alone do not change the exit code.

**Examples**

```powershell
pic-killer verify .\photos -r
pic-killer verify .\photos -r --where has-gps
pic-killer verify .\incoming -r --ext jpg,jpeg,png,heic
```

### 8.16 `completions` — shell completion and man page

```
pic-killer completions [--man] [shell]
```

**Options** — neither §5 nor §7 applies.

| Option | Value | Default | Description |
|--------|-------|---------|-------------|
| `[shell]` | `bash` `zsh` `fish` `powershell` `elvish` | — | Target shell |
| `--man` | — | off | Emit a roff man page instead |

**Behavior**

- Writes the generated script to standard output; redirect it where your shell expects it.
- Either a shell or `--man` must be given; with neither, the command errors.

**Examples**

```powershell
pic-killer completions powershell | Out-String | Invoke-Expression
pic-killer completions bash > /etc/bash_completion.d/pic-killer
pic-killer completions zsh > ~/.zfunc/_pic-killer
pic-killer completions --man > pic-killer.1
```

---

## 9. Metadata field reference

### 9.1 EXIF string tags

Accepted by `set --set-string`, `set --remove` and CSV `apply`. Names ignore case, hyphens, underscores and spaces.

| Name and aliases | EXIF tag |
|------------------|----------|
| `artist` | Artist |
| `copyright` | Copyright |
| `description`, `imagedescription` | ImageDescription |
| `software` | Software |
| `make` | Make |
| `model` | Model |
| `lensmake` | LensMake |
| `lensmodel` | LensModel |
| `owner`, `ownername` | OwnerName |
| `serial`, `serialnumber` | SerialNumber |
| `imageid`, `imageuniqueid` | ImageUniqueID |

### 9.2 EXIF numeric and rational tags

Rational values accept a fraction (`1/200`) or a decimal (`0.005`).

| Name and aliases | EXIF tag | Value |
|------------------|----------|-------|
| `iso`, `isospeed`, `isospeedratings` | ISO | Integer 0–65535 |
| `fnumber`, `aperture` | FNumber | Unsigned rational |
| `exposuretime`, `shutter`, `shutterspeed` | ExposureTime | Unsigned rational |
| `focallength` | FocalLength | Unsigned rational |
| `focallengthin35mmformat`, `focallength35` | FocalLengthIn35mmFormat | Integer |
| `exposurecompensation`, `exposurecomp`, `ev` | ExposureCompensation | Signed rational |
| `meteringmode` | MeteringMode | Integer |
| `whitebalance` | WhiteBalance | Integer |
| `flash` | Flash | Integer |
| `exposureprogram` | ExposureProgram | Integer |
| `colorspace` | ColorSpace | Integer |
| `contrast` | Contrast | Integer |
| `saturation` | Saturation | Integer |
| `sharpness` | Sharpness | Integer |

### 9.3 XMP namespaces and properties

Namespace prefixes accepted by `xmp --set` and `xmp --remove`:

| Prefix | URI |
|--------|-----|
| `rdf` | `http://www.w3.org/1999/02/22-rdf-syntax-ns#` |
| `dc` | `http://purl.org/dc/elements/1.1/` |
| `xmp` | `http://ns.adobe.com/xap/1.0/` |
| `photoshop` | `http://ns.adobe.com/photoshop/1.0/` |
| `lr` | `http://ns.adobe.com/lightroom/1.0/` |
| `xmpRights` | `http://ns.adobe.com/xap/1.0/rights/` |
| `Iptc4xmpCore` | `http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/` |

Properties written by the dedicated options, and their XMP value shape:

| Option | Property | Shape |
|--------|----------|-------|
| `--title` | `dc:title` | Language alternative |
| `--description` | `dc:description` | Language alternative |
| `--rights` | `dc:rights` | Language alternative |
| `--creator` | `dc:creator` | Ordered sequence |
| `--keywords` | `dc:subject` | Unordered bag |
| `--rating` | `xmp:Rating` | Simple |
| `--label` | `xmp:Label` | Simple |
| `--city` | `photoshop:City` | Simple |
| `--country` | `photoshop:Country` | Simple |

### 9.4 IPTC-IIM datasets

Names accepted by `iptc --set`, `iptc --remove` and CSV `iptc:` fields. An explicit `record:dataset` such as `2:105` is always accepted.

| Name and aliases | Dataset | Displayed as |
|------------------|---------|--------------|
| `title`, `objectname` | 2:05 | Title |
| `category` | 2:15 | Category |
| `keywords`, `keyword` | 2:25 | Keywords |
| `instructions` | 2:40 | Instructions |
| `datecreated` | 2:55 | DateCreated |
| `creator`, `byline`, `author` | 2:80 | Creator |
| `city` | 2:90 | City |
| `sublocation` | 2:92 | Sublocation |
| `state`, `province` | 2:95 | Province/State |
| `country` | 2:101 | Country |
| `headline` | 2:105 | Headline |
| `credit` | 2:110 | Credit |
| `source` | 2:115 | Source |
| `copyright` | 2:116 | Copyright |
| `caption`, `description` | 2:120 | Caption |
| `captionwriter` | 2:122 | CaptionWriter |

### 9.5 Orientation codes

Accepted by `set --orientation` and CSV `orientation`.

| Code | Keywords | Meaning |
|------|----------|---------|
| 1 | `normal`, `top-left`, `tl` | Normal |
| 2 | `mirror-h`, `flip-h` | Mirrored horizontally |
| 3 | `180`, `rotate-180`, `bottom-right`, `br` | Rotated 180° |
| 4 | `mirror-v`, `flip-v` | Mirrored vertically |
| 5 | `mirror-h-cw` | Mirrored horizontally, then rotated 90° clockwise |
| 6 | `cw`, `cw90`, `90`, `rotate-cw` | Rotated 90° clockwise |
| 7 | `mirror-h-ccw` | Mirrored horizontally, then rotated 90° counter-clockwise |
| 8 | `ccw`, `ccw90`, `270`, `rotate-ccw` | Rotated 90° counter-clockwise |

### 9.6 Time offset units

Used by `time --shift`, `time --interval` and `geotag --offset`. Units combine freely and are case-insensitive; a leading sign applies to the whole expression.

| Unit | Meaning |
|------|---------|
| `y` | Years, calendar arithmetic |
| `mo` | Months, calendar arithmetic |
| `w` | Weeks |
| `d` | Days |
| `h` | Hours |
| `m` | Minutes |
| `s` | Seconds |

---

## 10. CSV import format

`apply` reads a three-column CSV: file, field, value. `show --csv --for-apply` (§8.2) emits exactly this layout, so export, edit and re-import round-trip cleanly:

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

Parsing rules:

- A header row is detected and skipped automatically.
- A UTF-8 byte-order mark is tolerated, so files saved by Excel work.
- Values containing commas must be double-quoted; a literal double quote inside a quoted value is written `""`.
- All rows for one file are merged and applied together.

Field names fall into three groups.

| Prefix | Goes to | Field names |
|--------|---------|-------------|
| none | EXIF | See below |
| `xmp:` | XMP | `title` `description` `creator` `rights` `rating` `label` `keywords` `city` `country`, or any `prefix:name` |
| `iptc:` | IPTC-IIM | Any name or `record:dataset` from §9.4 |

EXIF field names accepted without a prefix:

| Field | Value |
|-------|-------|
| `datetimeoriginal`, `date`, `original` | Date-time |
| `createdate`, `digitized`, `datetimedigitized` | Date-time |
| `modifydate`, `modify`, `datetime` | Date-time |
| `alldates`, `all` | Date-time, written to all three |
| `gps` | `lat,lon[,alt]` |
| `gpsclear`, `cleargps`, `gps.clear` | Any value; removes GPS |
| `orientation` | Code or keyword from §9.5 |
| `usercomment` | Text |
| Any name from §9.1 | Text |
| Any name from §9.2 | Number or rational |

> Two traps to know. First, multi-value fields split on **`;` or `|`**, not on commas — unlike the `--keywords` option of §8.9, which splits on commas. A hand-written CSV value of `"a,b,c"` becomes one keyword, not three; write `"a;b;c"`. `--for-apply` emits the `;` form, so exported files round-trip correctly. This applies to `xmp:keywords`, `xmp:creator`, `iptc:keywords` and `iptc:creator`. Second, a short name after `xmp:` must be one this manual lists; an unrecognised one is rejected with the valid names shown, so a typo such as `xmp:ttile` fails loudly instead of quietly creating a `dc:ttile` property. To write any other property, give the full qualified name, e.g. `xmp:photoshop:Headline`.

---

## 11. Output formats

`show` has three output modes. All three write to standard output, so they can be redirected safely; the `--where` summary and the progress bar go to standard error.

The default table prints one block per file:

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

`--json` emits an array of file objects. `latitude` and `longitude` appear only when the file has GPS; `xmp` and `iptc` appear only when those systems are present:

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

`--csv` emits one row per tag with a fixed header. XMP and IPTC rows leave `hex` empty, and values containing commas are quoted:

```csv
file,group,name,hex,value
C:\photos\beach.jpg,IFD0,Make,0x010F,Canon
C:\photos\beach.jpg,GPS,GPSLatitude,0x0002,"30, 16, 2676/100 (26.7600)"
C:\photos\beach.jpg,XMP,dc:title,,West Lake
C:\photos\beach.jpg,IPTC,City,,Hangzhou
```

The `group` column is the EXIF IFD the tag belongs to — `IFD0`, `EXIF`, `GPS` — or `XMP` / `IPTC` for the other two systems.

---

## 12. Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success, including "nothing matched" and "user declined the prompt" |
| `1` | The command failed before processing: bad arguments, unparseable `--where`, unreadable GPX or CSV |
| `2` | Processing ran but at least one file failed; for `verify`, at least one problem was found |

Because a declined prompt also exits 0, scripts must pass `-y`; see §7.

---

## 13. Safety model

Every write follows the same sequence, implemented once and shared by all write commands:

1. If `--dry-run`, stop here and report what would have happened.
2. If `--backup` and no `<name>.bak` exists yet, copy the original to `<name>.bak`.
3. Record the file's current modification and access times.
4. Write the complete new file to `.<name>.pkick.tmp` in the same directory.
5. Flush it to disk, then atomically rename it over the original.
6. Restore the recorded timestamps, unless `--also-file-time` asked for something else.

The consequences worth knowing:

- Interrupting a batch — power loss, Ctrl-C, a full disk — cannot leave a partially written photo. Either the old file or the complete new file is present.
- The temporary file is in the same directory as the target, so the rename stays within one filesystem and is genuinely atomic.
- Because the file is rewritten rather than patched, the filesystem timestamp would change; it is deliberately restored so that tools sorting by file date are not disturbed.
- Parallel processing is safe because each file has its own temporary name and no two workers touch the same file.

---

## 14. Format support matrix

| Format | EXIF | XMP | IPTC | Notes |
|--------|------|-----|------|-------|
| JPEG | yes | yes | yes | Fully supported; all three systems coexist |
| PNG | yes | yes | no | XMP stored in an iTXt chunk |
| TIFF | yes | no | no | EXIF only |
| WebP | yes | no | no | Lossless and extended WebP only |
| HEIC | yes | no | no | Supported by the engine, less battle-tested than JPEG |
| AVIF | yes | no | no | Supported by the engine, less battle-tested than JPEG |
| JXL | yes | no | no | Supported by the engine, less battle-tested than JPEG |
| RAW (CR2, NEF, ARW…) | no | sidecar | no | Use `xmp --sidecar`; the RAW file is never opened |
| BMP, GIF | no | no | no | No metadata container; PIC-Killer says so and skips |

---

## 15. Messages and errors

Per-file result markers:

| Marker | Meaning |
|--------|---------|
| `[OK]` | The file was changed, or would be changed under `--dry-run` |
| `[跳过]` | Deliberately skipped; the reason follows in brackets |
| `[失败]` | The file could not be processed; the error follows in brackets |

Messages you are likely to meet:

| Message | Meaning and response |
|---------|----------------------|
| `未找到符合条件的图片文件。` | Nothing matched the paths, `--ext`, or `--where`. Check spelling and widen the filter |
| `已取消。` | The confirmation prompt was answered no — or could not be answered at all. Pass `-y` |
| `无可解析的原始拍摄时间，偏移模式跳过` | `--shift` needs an existing time to offset from. Use `--set` or `--from-name` |
| `BMP 无元数据容器，建议先转成 PNG 再处理` | BMP and GIF cannot store metadata at all |
| `不支持的文件类型：<path>` | The engine cannot parse this container, or the file does not exist |
| `错误：无法解析 --where 条件 ...` | The condition is not valid syntax; see §6 |
| `错误：--where 条件为空` | The expression contained no usable condition |
| `未知方向 ...` | Not one of the keywords or codes in §9.5 |
| `--rating 需在 0-5 之间` | `xmp --rating` accepts 0 to 5 only |
| `未知的命名空间前缀 ...` | `xmp --set` requires a prefix from §9.3 |
| `未知或暂不支持的字段 ...` | A CSV field name that `apply` does not recognise; see §10 |

---

## 16. Known limitations

- **Uncommon tags may not survive a rewrite.** The EXIF engine understands several dozen common tags and preserves them exactly. Rare vendor tags, particularly parts of MakerNote, can be dropped when the metadata block is rebuilt. Pixel data is never affected.
- **XMP is JPEG and PNG only.** TIFF, WebP and HEIC cannot carry XMP through PIC-Killer; use `--sidecar` if you need XMP alongside them.
- **IPTC-IIM is JPEG only.**
- **RAW files cannot be written directly.** Use `xmp --sidecar`, which is what Lightroom and darktable read anyway.
- **Thumbnail replacement is not supported**, because it requires rewriting IFD1 data offsets.
- **HEIC, AVIF and JXL are less proven than JPEG.** Use `--backup` or `--dry-run` on a sample before a large batch.
- **`--where` cannot mix `&&` and `||`.** Both mixing and a negative comparison against an unknown tag name are now refused with an error rather than silently misapplied, but the grammar still has no parentheses or precedence. See §6.
- **Read-only commands are sequential.** `show`, `report` and `verify` do not use the thread pool, so they are slower than the write commands on large libraries. The `--where` filter itself is parallel.
- **XMP and IPTC writes are capped at one JPEG segment.** A packet larger than roughly 64 KB cannot be written; multi-segment ExtendedXMP is not implemented.

---

## 17. Shell completion and man page

Generate a completion script once and load it from your shell profile:

```powershell
pic-killer completions powershell | Out-String | Invoke-Expression
```

```bash
pic-killer completions bash > /etc/bash_completion.d/pic-killer
pic-killer completions zsh > ~/.zfunc/_pic-killer
pic-killer completions fish > ~/.config/fish/completions/pic-killer.fish
pic-killer completions --man > pic-killer.1
```

Supported shells are `bash`, `zsh`, `fish`, `powershell` and `elvish`. The man page is roff, ready for `man -l pic-killer.1` or installation into a `man1` directory.
