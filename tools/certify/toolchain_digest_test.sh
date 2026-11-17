#!/usr/bin/env bash
# Tests of tools/certify/toolchain_digest.sh on a small fake toolchain, and the real toolchain's
# digest under two locales. A changed file must be named as changed, an extra file as added, a
# removed one as missing, and an unreadable file must fail as "cannot digest toolchain".
#   bash tools/certify/toolchain_digest_test.sh [REAL_TOOLCHAIN_DIR]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tool="$here/toolchain_digest.sh"
tmp="$(mktemp -d)"
trap 'chmod -R u+rwX "$tmp"; rm -rf "$tmp"' EXIT
fails=0
ok() { echo "ok: $*"; }
bad() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }

tc="$tmp/tc"
mkdir -p "$tc/bin" "$tc/lib/lean" "$tc/lib/lean/Été"
printf 'lean\n' > "$tc/bin/lean"
printf 'checker\n' > "$tc/bin/leanchecker"
printf 'a\n' > "$tc/lib/lean/A.olean"
printf 'b\n' > "$tc/lib/lean/b.olean"
printf 'c\n' > "$tc/lib/lean/Été/C.olean"
printf 'z\n' > "$tc/lib/lean/_z.olean"
(cd "$tc" && LC_ALL=C find . -type f | LC_ALL=C sort | xargs -d '\n' sha256sum) > "$tmp/toolchain.files"
sha256sum < "$tmp/toolchain.files" | cut -c1-64 > "$tmp/toolchain.sha256"
rec="$tmp/toolchain.sha256"

run() { "$tool" "$tc" "$rec" "$tmp/saved" > "$tmp/out" 2>&1; }
if run; then ok "the recorded toolchain matches"; else cat "$tmp/out" >&2; bad "the recorded toolchain is refused"; fi
cmp -s "$tmp/saved" "$tmp/toolchain.files" || bad "the saved list is not the computed one"
for loc in en_US.UTF-8 C.UTF-8 C; do
    LANG=$loc LC_ALL=$loc run || bad "the recorded toolchain is refused under LANG=LC_ALL=$loc"
done
ok "the digest matches under LANG/LC_ALL en_US.UTF-8, C.UTF-8 and C"

printf 'tampered\n' > "$tc/lib/lean/b.olean"
if run; then bad "a changed file is accepted"; elif grep -q '^changed: ./lib/lean/b.olean$' "$tmp/out" && grep -q '0 added, 0 missing, 1 changed' "$tmp/out"; then
    ok "a changed file is named: $(grep -m1 '^changed' "$tmp/out")"; else cat "$tmp/out" >&2; bad "the changed file is not named"; fi
{ [[ -s "$tmp/saved" ]] && ! cmp -s "$tmp/saved" "$tmp/toolchain.files"; } || bad "the computed list is not saved on a mismatch"
printf 'b\n' > "$tc/lib/lean/b.olean"

printf 'x\n' > "$tc/lib/lean/Extra.olean"
if run; then bad "an extra file is accepted"; elif grep -q '^added: ./lib/lean/Extra.olean$' "$tmp/out"; then
    ok "an extra file shows as added"; else cat "$tmp/out" >&2; bad "the extra file is not named as added"; fi
rm "$tc/lib/lean/Extra.olean"

mv "$tc/lib/lean/A.olean" "$tmp/A.olean"
if run; then bad "a missing file is accepted"; elif grep -q '^missing: ./lib/lean/A.olean$' "$tmp/out"; then
    ok "a removed file shows as missing"; else cat "$tmp/out" >&2; bad "the missing file is not named"; fi
mv "$tmp/A.olean" "$tc/lib/lean/A.olean"

if [[ "$(id -u)" -eq 0 ]]; then
    echo "skip: the unreadable-file test needs a non-root user"
else
    chmod 000 "$tc/lib/lean/_z.olean"
    if run; then bad "an unreadable file is accepted"; elif grep -q '^cannot digest toolchain: .*_z.olean' "$tmp/out" && ! grep -q 'is not the one' "$tmp/out"; then
        ok "an unreadable file fails as cannot digest: $(head -1 "$tmp/out" | cut -c1-140)"; else cat "$tmp/out" >&2; bad "the unreadable file is not a digest error"; fi
    chmod 644 "$tc/lib/lean/_z.olean"
    chmod 000 "$tc/lib/lean/Été"
    if run; then bad "an unreadable directory is accepted"; elif grep -q '^cannot digest toolchain: ' "$tmp/out"; then
        ok "an unreadable directory fails as cannot digest"; else cat "$tmp/out" >&2; bad "the unreadable directory is not a digest error"; fi
    chmod 755 "$tc/lib/lean/Été"
fi

printf 'bogus\n' > "$tmp/toolchain.sha256"
if run; then bad "a list that does not hash to the record is accepted"; else ok "a recorded list that does not hash to toolchain.sha256 is refused"; fi

real="${1:-}"
if [[ -n "$real" ]]; then
    for loc in en_US.UTF-8 C; do
        if LANG=$loc LC_ALL='' "$tool" "$real" "$here/toolchain.sha256" "$tmp/real.$loc" > "$tmp/out" 2>&1; then
            echo "real toolchain under LANG=$loc: $(sha256sum < "$tmp/real.$loc" | cut -c1-64)"
        else cat "$tmp/out" >&2; bad "the real toolchain is refused under LANG=$loc"; fi
    done
    if cmp -s "$tmp/real.en_US.UTF-8" "$tmp/real.C"; then ok "the real toolchain's list is identical under LANG=en_US.UTF-8 and C"
    else bad "the real toolchain's list differs between locales"; fi
fi
if [[ $fails -ne 0 ]]; then echo "$fails failure(s)" >&2; exit 1; fi
echo "all toolchain digest tests pass"
