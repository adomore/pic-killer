# PIC-Killer Documentation

> Index of the PIC-Killer documentation set. Every document exists in English and Chinese, structurally mirrored section for section.

**English** | [中文](README.zh.md)

---

## 1. Documents

| Document | English | 中文 | What it is |
|----------|---------|------|------------|
| Project README | [README.en.md](../README.en.md) | [README.md](../README.md) | The project's front page and quick tour |
| Getting Started | [GETTING-STARTED.md](GETTING-STARTED.md) | [GETTING-STARTED.zh.md](GETTING-STARTED.zh.md) | A 20-minute introduction: install, the safety net, five real tasks |
| User Manual | [USER-MANUAL.md](USER-MANUAL.md) | [USER-MANUAL.zh.md](USER-MANUAL.zh.md) | Complete reference: all 16 commands, every option, field tables, formats |
| Audit Report | [AUDIT.md](AUDIT.md) | [AUDIT.zh.md](AUDIT.zh.md) | Engineering audit: architecture, findings, security, test coverage |
| Feature Research | [FEATURE-RESEARCH.md](FEATURE-RESEARCH.md) | [FEATURE-RESEARCH.zh.md](FEATURE-RESEARCH.zh.md) | Competitive positioning, capability gaps, candidate features |
| Changelog | [CHANGELOG.md](../CHANGELOG.md) | [CHANGELOG.zh.md](../CHANGELOG.zh.md) | What changed in each release, and why |
| Release gate tests | [tests/README.md](../tests/README.md) | [tests/README.zh.md](../tests/README.zh.md) | What the two PowerShell suites cover and how to run them |

One naming exception: the repository README keeps Chinese at `README.md`, because that is the file GitHub shows as the front page, and puts English at `README.en.md`. Every other pair follows `X.md` for English and `X.zh.md` for Chinese.

---

## 2. Which one do I need

| If you… | Read |
|---------|------|
| Have never run PIC-Killer | Getting Started, start to finish |
| Need the exact syntax of one option | User Manual §8, the command reference |
| Are writing a script around PIC-Killer | User Manual §7 (`-y` matters), §12 (exit codes), §11 (output formats) |
| Are about to run a destructive batch | User Manual §6 (the two `--where` warnings) and §13 (safety model) |
| Want to know what can go wrong | User Manual §16, then the Audit Report |
| Are deciding what to build next | Feature Research |
| Are reviewing or contributing to the code | Audit Report §4 and §6 |

---

## 3. Conventions

These rules hold across the whole documentation set:

- **Mirrored structure.** The English and Chinese editions of a document have identical heading trees, identical section numbers, the same tables with the same rows, and the same code blocks. A change to one must be made to the other.
- **Section numbers, not anchors.** Cross-references are written `§8.3` rather than as links, because heading anchors differ between languages but section numbers do not.
- **Commands are not translated.** Only prose is. Command lines, field names, output samples and CSV content are identical in both editions.
- **Behaviour as observed.** Everything documented was checked against the source and against the running binary. Where the tool behaves surprisingly, the documentation says so rather than describing the intended behaviour.
- **PowerShell-style paths** in examples (`.\photos`). On Linux and macOS use `./photos`.
- **The mirror is machine-checked.** `bash docs/check-parity.sh --all` compares every pair's heading tree, section numbers, table row counts, code blocks and blockquotes, and exits non-zero on drift. Run it after editing any document; the `docs` job in CI runs it on every push and pull request, so drift fails the build rather than waiting to be noticed.
