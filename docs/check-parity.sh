#!/usr/bin/env bash
# EN/ZH lockstep parity checker for the PIC-Killer documentation set.
#
#   usage: bash docs/check-parity.sh <en.md> <zh.md>
#          bash docs/check-parity.sh --all
#
# Exits non-zero if a pair has drifted. See docs/README.md section 3.

# --all mode: check every documented pair in this directory.
if [ "$1" = "--all" ]; then
  here="$(cd "$(dirname "$0")" && pwd)"
  root="$(cd "$here/.." && pwd)"
  rc=0
  for base in README GETTING-STARTED USER-MANUAL AUDIT FEATURE-RESEARCH; do
    bash "$0" "$here/$base.md" "$here/$base.zh.md" || rc=1
    echo
  done
  # 仓库根目录与 tests/ 下的双语对也要守同一条铁律。
  for base in CHANGELOG tests/README; do
    bash "$0" "$root/$base.md" "$root/$base.zh.md" || rc=1
    echo
  done
  # 根 README 是唯一的命名例外：项目门面刻意把中文留在 README.md（GitHub 首页），
  # 英文放 README.en.md。它一度既没有英文版也不在这份清单里，于是没有任何东西
  # 会发现门面本身破了铁律。
  bash "$0" "$root/README.en.md" "$root/README.md" || rc=1
  echo
  if [ $rc = 0 ]; then echo "ALL PAIRS IN LOCKSTEP"; else echo "DRIFT DETECTED"; fi
  exit $rc
fi

en="$1"; zh="$2"
fail=0
name="$(basename "$en") <-> $(basename "$zh")"

report() { # label en_val zh_val
  if [ "$2" != "$3" ]; then
    printf '  [DRIFT] %-26s EN=%-28s ZH=%s\n' "$1" "$2" "$3"
    fail=1
  else
    printf '  [ok]    %-26s %s\n' "$1" "$2"
  fi
}

# --- structural counts -------------------------------------------------
h2_en=$(grep -c '^## '  "$en"); h2_zh=$(grep -c '^## '  "$zh")
h3_en=$(grep -c '^### ' "$en"); h3_zh=$(grep -c '^### ' "$zh")
h4_en=$(grep -c '^#### ' "$en"); h4_zh=$(grep -c '^#### ' "$zh")
fence_en=$(grep -c '^```' "$en"); fence_zh=$(grep -c '^```' "$zh")
bq_en=$(grep -c '^> '  "$en"); bq_zh=$(grep -c '^> '  "$zh")
hr_en=$(grep -c '^---$' "$en"); hr_zh=$(grep -c '^---$' "$zh")
trow_en=$(grep -c '^|'  "$en"); trow_zh=$(grep -c '^|'  "$zh")
li_en=$(grep -c '^[-*] \|^[0-9]\+\. ' "$en"); li_zh=$(grep -c '^[-*] \|^[0-9]\+\. ' "$zh")

echo "== $name"
report "## headings"        "$h2_en"    "$h2_zh"
report "### headings"       "$h3_en"    "$h3_zh"
report "#### headings"      "$h4_en"    "$h4_zh"
report "code fences"        "$fence_en" "$fence_zh"
report "blockquote lines"   "$bq_en"    "$bq_zh"
report "horizontal rules"   "$hr_en"    "$hr_zh"
report "table lines"        "$trow_en"  "$trow_zh"
report "list items"         "$li_en"    "$li_zh"

# --- section numbering must match --------------------------------------
# 只抽取带编号的小节（`## 3. Foo`）。没有编号的标题——比如 CHANGELOG 的版本号标题
# ——不参与文本比较，它们的数量已由上面的「## headings」计数覆盖。
n_en=$(grep '^## ' "$en" | sed -n 's/^## \([0-9][0-9.]*\)\..*/\1/p' | tr '\n' ',')
n_zh=$(grep '^## ' "$zh" | sed -n 's/^## \([0-9][0-9.]*\)\..*/\1/p' | tr '\n' ',')
report "## section numbers" "$n_en" "$n_zh"

s_en=$(grep '^### ' "$en" | sed 's/^### \([0-9.]*\).*/\1/' | tr '\n' ',')
s_zh=$(grep '^### ' "$zh" | sed 's/^### \([0-9.]*\).*/\1/' | tr '\n' ',')
report "### section numbers" "$s_en" "$s_zh"

# --- per-code-block line counts ----------------------------------------
blk() { awk '/^```/{f=!f; if(f){n=0; next} else {printf "%d,", n; next}} f{n++}' "$1"; }
report "code block sizes" "$(blk "$en")" "$(blk "$zh")"

# --- executable content inside code blocks must be identical -----------
# 注释是散文，可以翻译；命令不能。因此整行注释直接丢掉，行尾注释（` # …`）也一并
# 截掉——它同样不是可执行内容。截断只认「空白 + #」，命令本身没有裸 # 的写法。
code() {
  awk '/^```/{f=!f; next} f' "$1" |
    grep -v '^[[:space:]]*#' |
    sed 's/[[:space:]]\{1,\}#.*$//' |
    sed 's/[[:space:]]*$//'
}
if diff -q <(code "$en") <(code "$zh") >/dev/null 2>&1; then
  printf '  [ok]    %-26s identical\n' "code block content"
else
  printf '  [DRIFT] %-26s differs:\n' "code block content"
  diff <(code "$en") <(code "$zh") | head -20 | sed 's/^/            /'
  fail=1
fi

# --- per-table row counts ----------------------------------------------
trows() { awk '/^\|/{n++; next} {if(n){printf "%d,", n; n=0}} END{if(n)printf "%d,", n}' "$1"; }
report "per-table row counts" "$(trows "$en")" "$(trows "$zh")"

# --- language switcher header ------------------------------------------
grep -q '^\*\*English\*\* | \[中文\](' "$en" || { echo "  [DRIFT] EN missing language switcher"; fail=1; }
grep -q '| \*\*中文\*\*$' "$zh" || { echo "  [DRIFT] ZH missing language switcher"; fail=1; }

# --- forbidden placeholders --------------------------------------------
# Only unfinished-work markers, not legitimate prose mentions of the words.
PLACEHOLDER='TODO:\|FIXME:\|TBD\|待补充\|待完善\|（略）\|<!-- *TODO'
for f in "$en" "$zh"; do
  if grep -n "$PLACEHOLDER" "$f" >/dev/null 2>&1; then
    echo "  [DRIFT] placeholder text left in $(basename "$f"):"
    grep -n "$PLACEHOLDER" "$f" | head -5 | sed 's/^/            /'
    fail=1
  fi
done

if [ "$fail" = 0 ]; then echo "  == PASS"; else echo "  == FAIL"; fi
exit $fail
