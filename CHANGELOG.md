# Changelog

All notable changes to PIC-Killer are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**English** | [中文](CHANGELOG.zh.md)

## [Unreleased]

An engineering audit of 1.0.0 found that three of the tool's main advertised paths were
broken while all six release gates passed. Everything below closes those findings. See
[docs/AUDIT.md](docs/AUDIT.md) for the report, including how each fix was verified.

### Fixed

- **`strip` now removes all three metadata systems.** It previously cleared only EXIF,
  leaving the XMP packet and IPTC block — and any name, city or location they held — in a
  file the user had just cleaned for privacy, while reporting success. EXIF, XMP and IPTC
  are now cleared in a single atomic write. `--gps` removes coordinates from EXIF and from
  XMP `exif:GPS*`; textual place names are removed by a full `strip`.
- **`--where` no longer widens silently.** A negative comparison against a tag name that
  exists nowhere in the selection (`camera!=Canon`) matched every file; it is now refused
  before any write. Mixing `&&` with `||` misparsed into the wrong file set; it is now
  rejected with the offending half named.
- **The documented CSV round trip works.** `show --csv` emits a five-column dump that
  `apply` cannot read. The new `show --csv --for-apply` emits the three columns `apply`
  accepts, merging the GPS component tags into one row.
- **Sidecar writes are safe.** RAW+JPEG pairs sharing a stem resolved to the same `.xmp`
  and overwrote each other under parallelism, both reporting success; collisions are now
  refused before dispatch. Sidecar writes go through the atomic replace path and honour
  `--backup`.
- **`verify` detects the corruption it advertises.** The loader downgraded parse failures
  to "no metadata", so a damaged EXIF block read as healthy. Absent and corrupt are now
  distinguished. It also reports GPS tags whose coordinates cannot be parsed.
- **`apply` writes each file once.** It performed up to three separate atomic replaces, so
  a failure writing IPTC left the EXIF and XMP changes already committed.
- **Paths that do not exist are an error.** They were dropped silently and the run exited
  0, so a typo could look like a successful batch that touched nothing.
- **File ordering is natural.** Byte-lexicographic order put `IMG_10` before `IMG_2`,
  which made `time --sequential` assign timestamps in the wrong order.
- **`rotate --reset` repairs an out-of-range orientation.** It previously skipped the file
  as "already normal" — the exact defect `verify` reports, with no way to fix it.
- **`restore` is atomic in both modes**, and no longer leaves a half-written file if
  interrupted while restoring from a backup.
- **Three user-triggerable panics are gone**: `--tz` with a non-ASCII value, `--pattern`
  with an invalid strftime specifier, and `--where` with a non-ASCII tag name.
- **Non-interactive runs without `-y` now fail loudly.** The confirmation prompt read
  end-of-input, treated it as "no", and exited 0 having done nothing. The prompt itself
  moved to stderr so it stays visible when stdout is redirected.
- **Empty result sets stay machine-readable.** `--json` prints `[]` and `--csv` prints the
  header alone, with the human notice on stderr.
- Unrecognised short names after `xmp:` are rejected instead of silently creating a
  `dc:<name>` property; XMP qualified names are validated before interpolation; the
  documented `gps.clear` CSV field resolves; `apply`'s unknown-field error points at the
  working `xmp:` and `iptc:` prefixes; multi-value separators are consistent between
  export and import; GPS rationals with denominator 0 no longer emit bare `NaN` into JSON;
  `report` aligns CJK camera names by display width; `show --csv` neutralises spreadsheet
  formula prefixes; temporary files are no longer predictably named and are cleaned up on
  failure; `-v` prints a settings summary instead of being dead code.

### Added

- `show --csv --for-apply` — three-column export that `apply` reads back.
- `docs/` — a getting-started guide, user manual, audit report and feature research, as
  structurally mirrored EN/ZH pairs, with `check-parity.sh` enforcing the lockstep.
- `rust-version = "1.88"` in `Cargo.toml`, enforced by a CI job that compiles against exactly that version. An earlier commit in this cycle set it to 1.85 on the reasoning that edition 2024 requires 1.85; that was wrong — the code uses let-chains, stabilised in 1.88, and 1.87 rejects it with E0658.
- Dependabot configuration for Cargo and GitHub Actions.

### Changed

- `--where` reads each file once instead of once per condition, and filters in parallel.
- CI runs the end-to-end suite (`tests/functest.ps1`) on `windows-latest`, and gained jobs for the minimum supported Rust version and for dependency advisories.
- `actions/checkout` v4 → v7 across both workflows, clearing the Node 20 deprecation warning that appeared on every job. A byte-level diff of the two `action.yml` files shows a single differing line (`using: node20` → `node24`); every input is identical.
- Release actions bumped and validated with a real pre-release run: `upload-artifact` v4 → v7, `download-artifact` v4 → v8, `action-gh-release` v2 → v3. A tag containing a hyphen is now published as a pre-release.
- Tests: 51 → 73 unit tests, 59 → 103 end-to-end assertions.

### Security

- **Command injection in the release workflow.** The packaging step interpolated the
  `workflow_dispatch` tag input directly into a shell string (`version="${{ ... }}"`).
  GitHub substitutes expressions before the shell parses the line, so an input such as
  `v1.0.0"; curl … | sh; :"` would execute, in a job holding a repository-writable
  `GITHUB_TOKEN`. The value now travels through an environment variable. Only accounts
  with write access can trigger `workflow_dispatch`, so this was a privilege-escalation
  path for a compromised or limited-write account rather than an anonymous one.
- **quick-xml 0.37.5 → 0.41.0**, the patched floor for RUSTSEC-2026-0194 (quadratic
  duplicate-attribute scanning, a denial of service on attacker-supplied XMP) and
  RUSTSEC-2026-0195. This project's own two `attributes()` call sites leave the affected
  range. **It does not make the project unaffected:** `little_exif` 0.6.23 pins
  `quick-xml ^0.37.5`, so a second copy stays in the tree, and it is reachable — that
  crate's own `src/xmp.rs` calls `attributes()` on the PNG EXIF-clearing path. Binary
  size grows 35,840 bytes (+1.0%). The duplicate disappears once upstream moves.
- CI gained a `cargo audit` job. Both advisories were published 2026-06-29 and went
  unnoticed here for two months because nothing checked. The two known-unfixable ones are
  ignored by ID, so a *new* advisory still fails the build.
- CI declares `permissions: contents: read`; the workflow previously inherited whatever
  the repository default granted.

## [1.0.0] — 2026-07-11

### Added

- `xmp --sidecar` — read and write a standalone `.xmp` next to the image, which works for
  RAW files the EXIF engine cannot parse.
- `completions` — shell completion scripts for bash, zsh, fish, PowerShell and elvish,
  plus `--man` for a roff manual page.
- `verify` — check a library for metadata problems and exit non-zero when any are found.

## [0.4.0] — 2026-07-11

### Added

- Parallel processing with progress bars for all write commands.
- `apply` — batch-import metadata from a CSV, extended to XMP and IPTC fields.
- `report` — summarise metadata coverage across a library.
- Built-in glob expansion, numeric and rational EXIF tags, timezone offset writing.
- `--where` supports `&&` and `||` combinations.

## [0.3.0] — 2026-07-11

### Added

- `restore` — undo edits from `.bak` backups.
- `--where` metadata filter on all commands.
- `geotag` — GPS tagging by interpolating a GPX track against capture times.

## [0.2.0] — 2026-07-11

### Added

- CI workflow; the codebase is `fmt` and `clippy` clean.
- A friendly skip message for container-less formats (BMP, GIF).

## [0.1.0] — 2026-07-11

Initial release: a lossless photo metadata Swiss Army knife covering `time`, `show`,
`set`, `gps`, `strip`, `rotate`, `copy`, `rename`, `xmp` and `iptc`, with cross-platform
release binaries.

[Unreleased]: https://github.com/adomore/pic-killer/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/adomore/pic-killer/compare/v0.4.0...v1.0.0
[0.4.0]: https://github.com/adomore/pic-killer/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/adomore/pic-killer/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/adomore/pic-killer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/adomore/pic-killer/releases/tag/v0.1.0
