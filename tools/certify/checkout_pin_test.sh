#!/usr/bin/env bash
# Tests of tools/certify/checkout.sh's CERTIFY_PIN_BASE check on a fixture repository: the pin plus
# one clean commit under Certified/NAME/ is accepted; an inputs commit touching another path, a
# two-commit stack, a wrong parent, a merge, a dirty tree, a CERTIFY_PIN_BASE that is not the pin
# and the hatch with CERTIFY_PIN_BASE or in CI are refused.
#   bash tools/certify/checkout_pin_test.sh
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fails=0
ok() { echo "ok: $*"; }
bad() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }
g() { git -C "$repo" -c user.name=t -c user.email=t@localhost "$@"; }

# A solar root whose solidity-lean.rev is the fixture's pin.
mkdir -p "$tmp/root/tools/certify"
cp "$here/checkout.sh" "$here/lakefile_inputs.py" "$tmp/root/tools/certify/"
repo="$tmp/sl"
mkdir -p "$repo/Certified/foo" "$repo/Certified/bar"
git init -q "$repo"
# The driver's library stanza for NAME and CONTRACT, as generic.py appends it.
stanza() { python3 -I -c 'import sys; sys.path.insert(0, sys.argv[1]); import lakefile_inputs as l; sys.stdout.write(l.lakefile_lib(sys.argv[2], sys.argv[3]))' "$here" "$@"; }
# The base declares one run's library already (dd_0000abcd), as the pin declares flag_4ce5cf9a.
{ echo base; stanza dd_0000abcd C; } > "$repo/lakefile.lean"; echo a > "$repo/Certified/bar/A.lean"
g add -A; g commit -q -m base
pin="$(g rev-parse HEAD)"
echo "$pin" > "$tmp/root/tools/certify/solidity-lean.rev"
co() { env -u CI -u GITHUB_ACTIONS -u CERTIFY_ALLOW_UNPINNED SOLIDITY_LEAN_DIR="$repo" "$@" > "$tmp/out" 2>&1; }
checkout() { co CERTIFY_PIN_BASE="$pin" bash "$tmp/root/tools/certify/checkout.sh" Certified/foo; }
refused() { # NAME PATTERN
    if checkout; then bad "$1 is accepted"
    elif grep -q -- "$2" "$tmp/out"; then ok "$1 is refused: $(grep -m1 error "$tmp/out" | cut -c1-150)"
    else cat "$tmp/out" >&2; bad "$1 is refused without naming it ($2)"; fi
}

if checkout; then ok "the pin itself is accepted"; else cat "$tmp/out" >&2; bad "the pin is refused"; fi
echo x > "$repo/Certified/foo/foo.sol"; g add -A; g commit -q -m inputs
inputs="$(g rev-parse HEAD)"
if checkout && grep -q "solidity-lean $pin + inputs $inputs (tree $(g rev-parse "HEAD^{tree}"), Certified/foo only)" "$tmp/out"; then
    ok "one clean inputs commit under Certified/foo is accepted and recorded"; else cat "$tmp/out" >&2; bad "the happy path is refused or not recorded"; fi
[[ "$(tail -1 "$tmp/out")" == "$repo" ]] || [[ "$(checkout; tail -1 "$tmp/out")" == "$(realpath "$repo")" ]] || bad "the checkout path is not printed"

echo dirty >> "$repo/Certified/foo/foo.sol"
refused "a dirty tree (modified)" "is not clean: .*Certified/foo/foo.sol"
g checkout -q -- Certified/foo/foo.sol
echo new > "$repo/Certified/foo/untracked.lean"
refused "a dirty tree (untracked)" "is not clean: .*untracked.lean"
rm "$repo/Certified/foo/untracked.lean"

g reset -q --hard "$pin"; mkdir -p "$repo/Certified/foo"; echo y > "$repo/lakefile.lean"; echo x > "$repo/Certified/foo/foo.sol"; g add -A; g commit -q -m stray
refused "an inputs commit touching lakefile.lean" "touches paths outside Certified/foo/: lakefile.lean"
g reset -q --hard "$pin"; mkdir -p "$repo/Certified/foo"; echo b > "$repo/Certified/bar/A.lean"; g add -A; g commit -q -m other
refused "an inputs commit touching another contract" "touches paths outside Certified/foo/: Certified/bar/A.lean"
g reset -q --hard "$pin"; mkdir -p "$repo/Certified/foo"; echo x > "$repo/Certified/foobar.lean"; g add -A; g commit -q -m prefix
refused "a path that only shares the prefix" "touches paths outside Certified/foo/: Certified/foobar.lean"

g reset -q --hard "$pin"; mkdir -p "$repo/Certified/foo"; echo 1 > "$repo/Certified/foo/a"; g add -A; g commit -q -m one; echo 2 > "$repo/Certified/foo/b"; g add -A; g commit -q -m two
refused "a two-commit stack" "has parent .*, not the pin $pin"

g checkout -q --orphan other; g rm -q -rf . ; mkdir -p "$repo/Certified/foo"; echo o > "$repo/Certified/foo/o"; g add -A; g commit -q -m orphan-base
other="$(g rev-parse HEAD)"; echo p > "$repo/Certified/foo/p"; g add -A; g commit -q -m on-other
refused "a wrong parent" "has parent $other, not the pin"

g checkout -q -B side "$pin"; mkdir -p "$repo/Certified/foo"; echo s > "$repo/Certified/foo/s"; g add -A; g commit -q -m side
g checkout -q -B main2 "$pin"; mkdir -p "$repo/Certified/foo"; echo m > "$repo/Certified/foo/m"; g add -A; g commit -q -m main2
g merge -q --no-edit side > /dev/null
refused "a merge" "has 2 parents"

g checkout -q -B good "$pin"; mkdir -p "$repo/Certified/foo"; echo x > "$repo/Certified/foo/foo.sol"; g add -A; g commit -q -m inputs
if co CERTIFY_PIN_BASE=0000000000000000000000000000000000000000 bash "$tmp/root/tools/certify/checkout.sh" Certified/foo; then bad "a CERTIFY_PIN_BASE that is not the pin is accepted"
elif grep -q "is not the pin $pin" "$tmp/out"; then ok "a CERTIFY_PIN_BASE that is not the pin is refused"; else cat "$tmp/out" >&2; bad "wrong CERTIFY_PIN_BASE not named"; fi
if co CERTIFY_PIN_BASE="$pin" CERTIFY_ALLOW_UNPINNED=1 bash "$tmp/root/tools/certify/checkout.sh" Certified/foo; then bad "the hatch with CERTIFY_PIN_BASE is accepted"
else ok "the hatch is refused with CERTIFY_PIN_BASE"; fi
if co CI=true CERTIFY_ALLOW_UNPINNED=1 bash "$tmp/root/tools/certify/checkout.sh" Certified/foo; then bad "the hatch is accepted in CI"
elif grep -q "CERTIFY_ALLOW_UNPINNED is refused in CI" "$tmp/out"; then ok "the hatch is still refused in CI"; else cat "$tmp/out" >&2; bad "the CI refusal is not named"; fi
if co bash "$tmp/root/tools/certify/checkout.sh" Certified/foo; then bad "an unpinned HEAD without CERTIFY_PIN_BASE or the hatch is accepted"
else ok "an unpinned HEAD without CERTIFY_PIN_BASE is refused"; fi
if co CERTIFY_PIN_BASE="$pin" bash "$tmp/root/tools/certify/checkout.sh"; then bad "CERTIFY_PIN_BASE without an inputs directory is accepted"
else ok "CERTIFY_PIN_BASE without an inputs directory is refused"; fi

# lakefile.lean in the inputs commit: only the base's bytes plus the canonical stanza for the NAME of
# Certified/NAME, which the base does not declare (tools/certify/lakefile_inputs.py).
n=ab_0000abcd
lake_case() { # LABEL INPUTS_NAME PATTERN|accept, with the lakefile already written
    mkdir -p "$repo/Certified/$2"; echo x > "$repo/Certified/$2/$2.sol"; g add -A; g commit -q -m "$1"
    if co CERTIFY_PIN_BASE="$pin" bash "$tmp/root/tools/certify/checkout.sh" "Certified/$2"; then
        [[ "$3" == accept ]] && ok "$1 is accepted" || bad "$1 is accepted"
    elif [[ "$3" == accept ]]; then cat "$tmp/out" >&2; bad "$1 is refused"
    elif grep -q -- "$3" "$tmp/out"; then ok "$1 is refused: $(grep -m1 error "$tmp/out" | cut -c1-170)"
    else cat "$tmp/out" >&2; bad "$1 is refused without naming it ($3)"; fi
}
base_lf() { g reset -q --hard "$pin"; cat "$repo/lakefile.lean"; }
{ base_lf; stanza $n Flag; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_exact_append" $n accept
{ base_lf; stanza $n Flag | sed 's/roots := #\[/roots := #[Extra, /'; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_changed_stanza" $n "lakefile.lean (what it appends is not the library stanza"
{ base_lf | sed 's/^base$/base edited/'; stanza $n Flag; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_edit_elsewhere_plus_stanza" $n "lakefile.lean (it edits the base's lakefile.lean"
{ base_lf; stanza zz_0000abcd Flag; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_stanza_for_other_name" $n "lakefile.lean (what it appends is not the library stanza for $n"
{ base_lf; stanza dd_0000abcd C; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_stanza_for_declared_name" dd_0000abcd "already declares lean_lib dd_0000abcd"
{ base_lf; stanza $n Flag; stanza $n Flag; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_duplicate_stanza" $n "lakefile.lean (what it appends is not the library stanza"
{ base_lf; printf '\n-- %s: the stages of Certified/%s/%s.sol'"'"'s Fl"ag and their kernel proofs, which\n' $n $n $n; stanza $n Flag | tail -n +3; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_bad_contract_name" $n "lakefile.lean (what it appends is not the library stanza"
{ base_lf; stanza $n Flag; } > "$tmp/lf"; cp "$tmp/lf" "$repo/lakefile.lean"
lake_case "lakefile_not_driver_name_dir" Foo_x "lakefile.lean ('Certified/Foo_x' is not Certified/NAME"
if [[ $fails -ne 0 ]]; then echo "$fails failure(s)" >&2; exit 1; fi
echo "all checkout pin tests pass"
