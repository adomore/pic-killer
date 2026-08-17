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
  rc=0
  for base in README GETTING-STARTED USER-MANUAL AUDIT FEATURE-RESEARCH; do
    bash "$0" "$here/$base.md" "$here/$base.zh.md" || rc=1
    echo
  done
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
n_en=$(grep '^## ' "$en" | sed 's/^## \([0-9]*\)\..*/\1/' | tr '\n' ',')
n_zh=$(grep '^## ' "$zh" | sed 's/^## \([0-9]*\)\..*/\1/' | tr '\n' ',')
report "## section numbers" "$n_en" "$n_zh"

s_en=$(grep '^### ' "$en" | sed 's/^### \([0-9.]*\).*/\1/' | tr '\n' ',')
s_zh=$(grep '^### ' "$zh" | sed 's/^### \([0-9.]*\).*/\1/' | tr '\n' ',')
report "### section numbers" "$s_en" "$s_zh"

# --- per-code-block line counts ----------------------------------------
blk() { awk '/^```/{f=!f; if(f){n=0; next} else {printf "%d,", n; next}} f{n++}' "$1"; }
report "code block sizes" "$(blk "$en")" "$(blk "$zh")"

# --- executable content inside code blocks must be identical -----------
# (strip full-line comments starting with # , which are allowed to differ)
code() { awk '/^```/{f=!f; next} f' "$1" | grep -v '^[[:space:]]*#' | sed 's/[[:space:]]*$//'; }
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
