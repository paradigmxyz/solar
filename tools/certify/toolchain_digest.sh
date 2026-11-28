#!/usr/bin/env bash
# toolchain_digest.sh DIR RECORD [SAVE]: checks that the Lean toolchain at DIR is the one RECORD
# (tools/certify/toolchain.sha256) names. The digest is the SHA-256 of the toolchain's file list:
# one "SHA256  ./PATH" line per regular file, sorted bytewise (LC_ALL=C); RECORD's sibling
# toolchain.files holds the recorded list, whose own SHA-256 must be RECORD. Any error while
# listing or hashing (a find, sort or sha256sum failure, an unreadable file) fails as
# "cannot digest toolchain", never as a mismatch. On a mismatch it prints which files were added,
# are missing or changed against the recorded list. SAVE, when given, receives the computed list,
# also on success, so a run's receipts hold what was hashed. Exits 0 only on a match.
set -euo pipefail
export LC_ALL=C
[[ $# -eq 2 || $# -eq 3 ]] || { echo "usage: toolchain_digest.sh DIR RECORD [SAVE]" >&2; exit 2; }
dir=$1 record=$2 save=${3:-}
files="${record%.sha256}.files"
list="$(mktemp)" err="$(mktemp)"
trap 'rm -f "$list" "$err"' EXIT
if ! (cd "$dir" && find . -type f | sort | xargs -r -d '\n' sha256sum) > "$list" 2> "$err" || [[ -s "$err" ]]; then
    echo "cannot digest toolchain: $(tr '\n' ' ' < "$err" | cut -c1-2000)" >&2
    exit 1
fi
if [[ -n "$save" ]]; then
    mkdir -p "$(dirname "$save")"
    cp "$list" "$save"
fi
want="$(tr -d '[:space:]' < "$record")"
[[ "$(sha256sum < "$files" | cut -c1-64)" == "$want" ]] ||
    { echo "the recorded toolchain list $files does not hash to $record's digest" >&2; exit 1; }
got="$(sha256sum < "$list" | cut -c1-64)"
[[ "$got" != "$want" ]] || exit 0
# Paths start at column 67 of a sha256sum line: 64 hex digits and two spaces.
diffs="$(awk 'NR == FNR { h[substr($0, 67)] = substr($0, 1, 64); next }
    { p = substr($0, 67); seen[p] = 1
      if (!(p in h)) print "added: " p; else if (h[p] != substr($0, 1, 64)) print "changed: " p }
    END { for (p in h) if (!(p in seen)) print "missing: " p }' "$files" "$list" | sort)"
counts="$(awk '{ c[$1]++ } END { printf "%d added, %d missing, %d changed", c["added:"], c["missing:"], c["changed:"] }' <<<"$diffs")"
{
    echo "the toolchain at $dir is not the one $record records (digest $got, $counts):"
    head -50 <<<"$diffs"
    [[ -z "$save" ]] || echo "computed list saved to $save"
} >&2
exit 1
