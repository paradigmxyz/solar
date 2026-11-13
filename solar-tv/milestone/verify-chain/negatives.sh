#!/usr/bin/env bash
# Negative tests of an example's chain above the backend stages, which
# solar-tv/milestone/negative.sh covers:
#
#   solar-tv/milestone/verify-chain/negatives.sh EXAMPLE
#
# 1. spec: each mutant verify-chain/EXAMPLE/mutants.py writes (the source with one edit
#    to its code or to its specs) must fail solar-verify with a counterexample: exit status 1, a
#    "counterexample" note, and no obligation left undecided. The unedited source must verify.
# 2. kernel VCs: solar-verify --emit-lean exports the unedited source's obligations and
#    crates/verify/lean/discharge.sh must prove every required one in the Lean kernel, with no
#    axiom beyond propext, Classical.choice and Quot.sound. Each mutant's export must then fail
#    the discharge by a failed proof of an obligation, never by a timeout.
# 3. mutate: solar-verify mutate runs on the unedited source; its summary is printed for the
#    status table, not failed on.
# 4. binding: the manifest of solar-tv/milestone/verify_EXAMPLE must bind its source, stages and
#    bytecode by SHA-256, and a manifest swapped for another milestone's, a source with one byte
#    changed, a stage with one byte changed and a bytecode pin with one byte changed must each
#    fail that check.
#
# SOLAR_VERIFY (default $CARGO_TARGET_DIR/debug/solar-verify) and Z3 on PATH.
set -euo pipefail

[[ $# -eq 1 ]] || { echo "usage: negatives.sh EXAMPLE" >&2; exit 1; }
example=$1
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
cd "$root"
verify="${SOLAR_VERIFY:-${CARGO_TARGET_DIR:-$root/target}/debug/solar-verify}"
name="verify_$(tr 'A-Z' 'a-z' <<<"$example")"
source="crates/verify/examples/$example.sol"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
failures=0
fail() { echo "FAIL: $*"; failures=$((failures + 1)); }

echo "== 1. spec mutants of $source"
"$verify" "$source" > "$tmp/control.out" 2>&1 || { cat "$tmp/control.out"; fail "the unedited $example does not verify"; }
python3 "$here/$example/mutants.py" "$source" "$tmp/mutants"
for mutant in "$tmp"/mutants/*.sol; do
    case="$(basename "$mutant" .sol)"
    status=0
    "$verify" "$mutant" > "$tmp/$case.out" 2>&1 || status=$?
    if [[ $status -ne 1 ]]; then
        fail "$case: exit status $status, not 1"
    elif grep -q -E -i 'unknown|timed? ?out|gave up|inconclusive' "$tmp/$case.out"; then
        fail "$case: an obligation was not decided"
    elif ! grep -q 'counterexample' "$tmp/$case.out"; then
        fail "$case: no counterexample"
    else
        echo "ok $case: $(grep -m1 -E '^error' "$tmp/$case.out")"
    fi
done

echo "== 2. kernel VCs of $source"
discharge="crates/verify/lean/discharge.sh"
if "$verify" --emit-lean "$tmp/vc-control" "$source" > /dev/null 2>&1 &&
    "$discharge" "$tmp/vc-control" 600 > "$tmp/vc-control.out" 2>&1; then
    echo "ok control: $(tail -n 1 "$tmp/vc-control.out")"
else
    tail -n 5 "$tmp/vc-control.out"
    fail "the unedited $example's obligations are not all proved in the kernel"
fi
for mutant in "$tmp"/mutants/*.sol; do
    case="$(basename "$mutant" .sol)"
    "$verify" --emit-lean "$tmp/vc-$case" "$mutant" > /dev/null 2>&1 || true
    status=0
    "$discharge" "$tmp/vc-$case" 600 > "$tmp/vc-$case.out" 2>&1 || status=$?
    results="$tmp/vc-$case/discharge/results.tsv"
    if [[ $status -eq 0 ]]; then
        fail "$case: every obligation is proved in the kernel"
    elif grep -q 'timeout' "$results" 2> /dev/null; then
        fail "$case: an obligation timed out instead of failing"
    elif ! grep -q $'	failed: ' "$results" 2> /dev/null; then
        tail -n 3 "$tmp/vc-$case.out"
        fail "$case: the discharge failed for another reason"
    else
        echo "ok $case: $(grep -c $'	failed: ' "$results") obligation(s) not proved, e.g. $(grep -m1 $'	failed: ' "$results" | cut -f1)"
    fi
done

echo "== 3. solar-verify mutate"
"$verify" mutate --jobs 1 "$source" 2>&1 | grep -E '^(killed|survived|equivalent|undecided|stillborn|mutation results)' || true

echo "== 4. binding of solar-tv/milestone/$name"
gen="$(ls -d solar-tv/milestone/$name/gen/*/)"
bind() {
    python3 - "$1" "$2" "$3" <<'EOF'
import hashlib, json, pathlib, sys

directory, source = pathlib.Path(sys.argv[1]), sys.argv[2]
bytecode = pathlib.Path(sys.argv[3]).read_text().strip()
manifest = json.loads((directory / "manifest.json").read_text())
sha = lambda path: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
assert len(manifest["sources"]) == 1, "sources"
assert manifest["sources"][0]["sha256"] == sha(source), "source hash"
assert manifest["bytecode"] == bytecode, "bytecode pin"
assert manifest["bytecode_sha256"] == hashlib.sha256(bytes.fromhex(bytecode)).hexdigest(), "bytecode hash"
for stage in manifest["stages"]:
    assert stage["sha256"] == sha(directory / stage["file"]), stage["file"]
EOF
}
src="solar-tv/milestone/$name/$name.sol"
pin="solar-tv/milestone/$name/bytecode.hex"
bind "$gen" "$src" "$pin" 2> "$tmp/bind.err" || { cat "$tmp/bind.err"; fail "the unedited manifest does not bind"; }
expect_unbound() {
    if bind "$2" "$3" "$4" 2> "$tmp/bind.err"; then
        fail "binding accepts $1"
    else
        echo "ok $1: $(tail -n 1 "$tmp/bind.err")"
    fi
}
# Another milestone's manifest in place of this one's.
other="$(ls -d solar-tv/milestone/counter/gen/*/)"
cp -r "$gen" "$tmp/swapped"
cp "$other/manifest.json" "$tmp/swapped/manifest.json"
expect_unbound "swapped manifest" "$tmp/swapped" "$src" "$pin"
# One byte of the source: the operand of its first compound addition (Counter's `+= 1` as `+= 2`,
# Token's `+= amount` as `+= bmount`).
python3 -c 'import sys; b = bytearray(open(sys.argv[1], "rb").read()); i = b.index(b"+= "); b[i + 3] += 1; open(sys.argv[2], "wb").write(b)' "$src" "$tmp/source.sol"
expect_unbound "edited source" "$gen" "$tmp/source.sol" "$pin"
# One byte of the first lowered MIR.
cp -r "$gen" "$tmp/stage"
sed -i '0,/\.const 1\]/s//.const 2]/' "$tmp/stage/Stage0.lean"
if cmp -s "$gen/Stage0.lean" "$tmp/stage/Stage0.lean"; then fail "the stage edit changed nothing"; fi
expect_unbound "edited stage" "$tmp/stage" "$src" "$pin"
# One byte of the bytecode pin.
python3 -c 'import sys; h = open(sys.argv[1]).read().strip(); i = h.index("01"); open(sys.argv[2], "w").write(h[:i] + "02" + h[i + 2:] + "\n")' "$pin" "$tmp/pin.hex"
expect_unbound "edited bytecode" "$gen" "$src" "$tmp/pin.hex"

if [[ $failures -ne 0 ]]; then
    echo "$failures negative tests failed"
    exit 1
fi
echo "every negative test fails as it must"
