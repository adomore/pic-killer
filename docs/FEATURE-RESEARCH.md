# PIC-Killer — Feature Research

> Where PIC-Killer actually stands against the alternatives, which capability gaps users will hit, and what is worth building next. Written to inform a roadmap, not to flatter the project.

**English** | [中文](FEATURE-RESEARCH.zh.md)

---

## 1. Purpose

This document answers three questions: what PIC-Killer is genuinely better at than the alternatives, what it cannot do that users will need, and which of those gaps are worth closing given the architecture it has.

Feasibility judgements here are constrained by one fact that shapes almost everything: PIC-Killer does not parse image containers itself for EXIF. It delegates to `little_exif`, and that dependency sets a hard ceiling on several otherwise-obvious features. Where a proposal escapes that ceiling, it does so by bypassing the dependency entirely — which is why sidecar files keep reappearing below as the answer to RAW.

Findings referenced as F-nn are from the [Audit Report](AUDIT.md).

---

## 2. Current capability boundary

What PIC-Killer does today, stated precisely enough to reason about:

| Dimension | Boundary |
|-----------|----------|
| Metadata systems | EXIF, XMP, IPTC-IIM — all three, through one uniform command surface |
| Containers, EXIF | JPEG, PNG, TIFF, WebP, HEIC, AVIF, JXL |
| Containers, XMP | JPEG and PNG only, plus standalone sidecar files |
| Containers, IPTC | JPEG only |
| RAW | No native read or write; sidecar `.xmp` only |
| Video | None |
| Writable EXIF tags | A hand-maintained whitelist: 11 string tags, 14 numeric and rational tags, plus time, GPS, orientation and user comment |
| Batch selection | Paths, directories, recursion, extension filter, internal wildcard expansion, metadata filter |
| Parallelism | Write commands only; read-only commands are sequential |
| Safety | Dry-run, backup, confirmation prompt, atomic replace, timestamp preservation |
| Automation | Exit codes, JSON and CSV export, shell completion, man page |
| Language | Chinese only, in both the interface and the documentation |

Three of those lines are the strategically important ones: the whitelist of roughly 25 writable tags, the absence of RAW and video, and Chinese-only output.

---

## 3. Competitive landscape

| Tool | Strength | Weakness | Where PIC-Killer wins |
|------|----------|----------|----------------------|
| ExifTool | The de-facto standard. Thousands of writable tags, hundreds of formats including RAW, video, PDF and audio. Nothing else is close on coverage | Needs a Perl runtime or a multi-megabyte bundled executable; per-file process startup makes large batches slow; single-threaded; the option surface is famously hard to learn | Single static binary with no runtime, parallel writes, dry-run and confirmation on every write command, and a task-shaped rather than tag-shaped command surface |
| exiv2 | Fast native C++, no runtime dependency, and the same three-system scope — the closest real competitor | The CLI is a thin wrapper over the library: syntax like `-M"set Exif.Image.Artist Ascii 张三"` is unfriendly, and there is no geotagging, no rename, no CSV import, no metadata filter | Task-shaped commands (`time`, `geotag`, `rename`, `apply`, `report`, `verify`), the `--where` filter, and a safety model that is on by default |
| jhead | Tiny, fast, single binary, well-honed at date shifting and date-based rename | JPEG only, EXIF only, very small writable tag set, effectively unmaintained, English only | A strict superset of scope while matching the single-binary ergonomics — except that jhead can regenerate thumbnails and PIC-Killer cannot |
| ExifCleaner | Zero learning curve for the privacy case: drag in, metadata gone. Cross-platform GUI | Strip-only; cannot set, copy, geotag or rename; large Electron bundle; not scriptable | Scriptable, CI-integrable and previewable — **but see F-01: on this competitor's single core use case PIC-Killer is currently incorrect**, because `strip` leaves XMP and IPTC behind |
| GUI managers (Lightroom, digiKam, XnView MP, Photo Mechanic) | Metadata editing sits inside the workflow where photos are already being viewed and culled; visual feedback; excellent for per-photo work | Heavyweight, often commercial; batch operations are click-driven, hard to reproduce and impossible to audit; not scriptable | The operations that are painful in a GUI precisely because they are mechanical and high-volume: shift 4,000 timestamps by +2h, geotag a whole trip from a track, rename a card dump |

**The honest positioning.** PIC-Killer is not competing with ExifTool on coverage and should not pretend to. Its defensible position is a zero-runtime single binary that performs the twenty-odd batch operations a working photographer actually needs, combining three properties none of the alternatives combine: write safety as a default rather than a flag, one uniform selection model across all three metadata systems, and Chinese-first ergonomics.

Two of those three claims currently have holes in them. Write safety is undermined by F-01 and F-05; the uniform selection model is undermined by F-02 and F-03. Closing those is worth more than any new feature on this list.

---

## 4. Capability gaps

Ordered by how likely a real user is to hit them.

| Gap | Impact | Note |
|-----|--------|------|
| `strip` does not remove XMP or IPTC | High | Not a gap but a defect — see F-01. Listed here because it is also the feature ExifCleaner exists for |
| No native RAW read or write, and the sidecar workaround collides | High | RAW is the biggest hole in the product; the workaround has a data-loss bug (F-05) |
| Writable tags are a whitelist of about 25 names | High | Every new tag a user wants is a code change. This is the largest functional distance to ExifTool |
| XMP is JPEG and PNG only, yet `--ext` defaults include TIFF and WebP | High | Self-inflicted: a first `xmp` run on a TIFF folder returns nothing but skips, which reads as the tool being broken |
| No video metadata of any kind | High | Modern phone libraries are roughly half video, and both rename-by-time and geotag apply equally to clips |
| Chinese-only interface and documentation | High | The single biggest adoption barrier outside Chinese-speaking users |
| Read-only commands are single-threaded, and `--where` re-reads per condition | Medium | Speed is an explicit positioning claim that currently holds only for writes (F-17) |
| MakerNote survives as bytes but its internal offsets are not fixed up | Medium | A corrupted MakerNote is worse than a missing one: downstream tools parse garbage instead of reporting absence |
| `rename` supports only a capture-time strftime template | Medium | No filename token, no counter, no date subdirectories, no mtime fallback |
| No selective strip | Medium | "Remove everything except copyright and creator" — the most common professional need — is inexpressible |
| Undo is per-file `.bak` with no operation journal | Medium | Requires foresight, and repeated edits collapse onto the first backup |
| No config file and no presets | Medium | Every invocation retypes the full option set, which pushes users into wrapper scripts |
| XMP and IPTC writes capped at one 64 KB JPEG segment | Medium | Lightroom packets with develop settings or deep keyword trees routinely exceed this |
| PNG EXIF written as legacy zTXt, never the standard `eXIf` chunk | Medium | An interoperability trap with modern readers |
| Embedded thumbnails never updated or replaceable | Medium | After a rotation the thumbnail still shows the old orientation |
| No structured output for `verify` | Medium | Pitched as a CI gate but exposes only an exit code and Chinese prose |

---

## 5. Candidate features

Value is user impact, cost is implementation effort, risk is the chance of breaking something that currently works.

| # | Candidate | Value | Cost | Risk |
|---|-----------|-------|------|------|
| 1 | Make `strip` actually strip, plus a `--keep` whitelist | High | Low | Low |
| 2 | Recursive-descent parser for `--where`, with absence/inequality separated | High | Low | Low |
| 3 | Write-preflight guardrails and post-write verification | High | Low | Low |
| 4 | Close the sidecar loop: collision-free naming, read-back, darktable style | High | Low | Low |
| 5 | Parallelise read-only commands and cache reads across `--where` conditions | High | Low | Low |
| 6 | Real-sample fixtures and the end-to-end suite in CI | High | Medium | Low |
| 7 | Generic tag write by hex and format, via the dependency's `Unknown*` variants | High | Medium | Medium |
| 8 | English output and documentation | High | Medium | Low |
| 9 | Extend `rename`: filename and counter tokens, date subdirectories, mtime fallback | High | Medium | Low |
| 10 | Unified machine-readable output across all commands | High | Medium | Medium |
| 11 | Extend sidecars to carry EXIF and IPTC for a full RAW workflow | High | Medium | Medium |
| 12 | Config file and named presets | Medium | Low | Low |
| 13 | Undo journal with a real `undo` command | Medium | High | Medium |
| 14 | XMP for WebP, then TIFF, then HEIC | Medium | High | Medium |
| 15 | ExtendedXMP segmentation to lift the 64 KB ceiling | Medium | High | Medium |
| 16 | Expose the engine as a Rust library | Medium | Medium | Medium |
| 17 | Exact-match content deduplication | Medium | Low | Low |
| 18 | MakerNote preservation | High | High | High |
| 19 | Video container metadata (MP4/MOV) | Medium | High | High |
| 20 | Interactive TUI mode | Low | High | Medium |

---

## 6. Feasibility deep dives

### 6.1 Make `strip` actually strip

The best ratio on the board, and it closes a correctness gap rather than a feature gap. Both building blocks already exist in-tree and are unit-tested: `xmp::remove_packet` and `iptc::remove_jpeg_iptc`. `process_strip` needs to call both after `exif::strip_all`. For `--gps`, additionally drop the XMP location properties and IPTC 2:90/2:95/2:101. A `--keep` whitelist is a natural extension of the same pass and makes the common professional case expressible.

No upstream dependency is involved — the dependency's `clear_metadata` only ever handled the EXIF segment, and the other two segments are already handled by this repository's own segment-surgery code.

### 6.2 Rebuild the `--where` parser

This is a correctness fix wearing a feature's clothes. Two independent problems: the splitter cannot express mixed operators and misparses silently (F-03), and negative comparisons cannot distinguish an absent tag name from an unequal value (F-02).

The staged approach is worth taking. First, a guard that rejects any sub-expression still containing the other operator after splitting — two lines, and it converts silent corruption into an error immediately. Then a proper recursive-descent parser with parentheses and precedence, which is well-trodden work against an existing test module. The absence-versus-inequality fix is independent of both and should land first, because it is the one with data-loss consequences.

### 6.3 Write-preflight guardrails

The highest value-per-line item, and a precondition for candidates 7, 11, 14 and 19. Today nothing verifies that a write produced a readable file. A preflight would check the container is what the extension claims, and a post-write pass would re-read the written file and confirm the intended tags are present and the pixel data is unchanged.

This matters specifically because of three known-dangerous behaviours in the EXIF dependency: it determines file type from the extension rather than content, its TIFF writer rebuilds the entire file — which is destructive for CR2 and NEF disguised as TIFF — and MakerNote is relocated without offset fixup. A guardrail layer is how you touch any of those formats without gambling.

### 6.4 The sidecar route to RAW

Native RAW parsing is out of reach at acceptable cost, and attempting it through the current dependency would be actively dangerous given the TIFF whole-file rebuild. Sidecars are the workflow Lightroom and darktable users already live in, so this delivers most of the practical RAW value for a fraction of the effort, and it works precisely because it bypasses the dependency entirely.

Three pieces, all confined to this repository. Fix the collision first — it is a data-loss defect regardless of any roadmap (F-05) — by adopting a naming scheme that cannot collide, ideally offering both the Adobe and darktable conventions. Then add sidecar read-back so `show` and `--where` can see sidecar content for files whose container cannot be parsed. Then extend the sidecar to carry EXIF-equivalent and IPTC-equivalent properties, which XMP already has vocabularies for, giving RAW users `time`, `gps` and `geotag` without ever opening the RAW file.

### 6.5 Generic tag write

The single change that most narrows the functional distance to ExifTool, because it converts a closed whitelist of about 25 names into an open surface. The upstream capability already exists and is unused: the dependency exposes `Unknown*` tag variants carrying value, hex code and group.

The work is a new CLI surface — something like `--set-tag "EXIF:0x9291:STRING=00"` — plus a parser mapping group, hex and format onto the right variant. The risk is that it hands users a loaded gun: writing a malformed value to an arbitrary tag is exactly the kind of thing candidate 3's post-write verification exists to catch, which is why that should land first. Keep the friendly named options as the ergonomic front door; the generic form is an escape hatch, not a replacement.

---

## 7. Proposed roadmap

**Now — correctness, before any new capability.** Candidates 1, 2 and the collision half of 4, plus the audit's small fixes. Every one of these closes a case where the tool currently does something different from what it says. Shipping features on top of a `strip` that does not strip compounds the problem.

**Next — make the existing claims true.** Candidates 3, 5 and 6. Guardrails, parallel reads, and the end-to-end suite in CI. This is the release where "fast" becomes true for reads as well as writes, and where the largest module stops being unprotected. None of it is user-visible, and all of it is what makes the following release safe to attempt.

**Then — widen the audience.** Candidates 8, 9 and 12. English output is the biggest single lever on adoption and costs nothing architecturally. Richer `rename` templating and presets are the two things that most often push a batch user into writing their own wrapper script.

**After that — widen the surface.** Candidates 7, 10 and 11. Generic tag write, structured output everywhere, and the full sidecar RAW workflow. These are the features that change what the tool is for, and each depends on the guardrails from the second phase.

**Later, if demand appears.** Candidates 13 through 17. All defensible, none urgent.

**Only with a real reason.** Candidates 18 and 19. MakerNote preservation is high value but its root cause is upstream, and video is a different problem domain wearing a similar hat.

---

## 8. Explicitly not recommended

**Perceptual hashing for near-duplicate detection.** The project has never decoded a pixel — there is no image codec anywhere in the dependency tree, which is exactly why the release binary is small and why the losslessness guarantee is easy to reason about. Adding a decoder to find visually similar photos would break both properties for a feature that sits outside the tool's purpose. Exact-match deduplication by content hash needs no decoder and is the version worth having.

**Chasing ExifTool's tag coverage.** A hand-maintained table can never catch up, and trying turns every release into transcription work. The generic tag write of candidate 7 is the right answer: open the surface instead of enumerating it.

**Native RAW container parsing.** The cost is high, the formats are numerous and proprietary, and the current dependency's TIFF writer would destroy the files. Sidecars deliver the workflow value without the risk.

**An interactive TUI.** The CLI already provides what a TUI would mainly be for: dry-run preview, a confirmation prompt, and a progress bar on stderr. It would be a large surface with a new dependency stack, serving a need the existing interface already meets.

**An i18n framework for the English output of candidate 8.** Two languages do not justify the machinery. A compile-time string table, or simply choosing English as the interface language with Chinese documentation, is proportionate. The decision worth making deliberately is which language the *output* speaks, because scripts and tests parse it.
