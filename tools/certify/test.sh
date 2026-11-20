#!/usr/bin/env bash
# Negative tests of scripts/certify itself: each case must stop the driver with its error, never
# with a CERTIFIED line. The driver cases fail before any Lean runs: a checkout or solar that is not
# the pin, malformed options and loops, and a solar (real, or an ELF stand-in) that exits non-zero
# after printing plausible output or prints the bytecode missing, empty or malformed. The unit
# cases test the driver's helpers on what it cannot meet before Lean: producers whose exit status
# a pipe would hide (tools/certify/inputs.sh), missing or malformed records, truncated spec exports
# and a solar-verify cut short (tools/certify/spec.sh, export_check.py), and the kernel replay's
# accounting (tools/certify/replay.py). The audit cases, which need Lean on PATH, check that
# tools/certify/audit.sh rejects a declared axiom, native_decide and sorry, and run the replay
# accounting against the real checker and leanchecker.
#
#   tools/certify/test.sh
#
# SOLAR names the solar binary (else target/debug/solar). The cases run against scratch packages
# standing in for solidity-lean, so they need no network.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
solar="$(realpath "${SOLAR:-$root/target/debug/solar}")"
[[ -x "$solar" ]] || { echo "error: no solar binary at $solar" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
pin="$(tr -d '[:space:]' < "$root/tools/certify/solidity-lean.rev")"
failures=0

# A scratch package at a commit that is not the pin.
lean="$tmp/lean"
mkdir -p "$lean/Examples"
git -C "$lean" init -q
printf 'name = "scratch"\n' > "$lean/lakefile.toml"
git -C "$lean" add -A
git -C "$lean" -c user.name=certify -c user.email=certify@localhost commit -q -m scratch

cat > "$tmp/counter.sol" <<'EOF'
contract Counter {
    uint256 x;
    function inc(uint256 d) external { x += d; }
    function get() external view returns (uint256) { return x; }
}
EOF
cat > "$tmp/two.sol" <<'EOF'
contract A { function f() external pure returns (uint256) { return 1; } }
contract B { function g() external pure returns (uint256) { return 2; } }
EOF

# expect CASE PATTERN ENV... -- ARGS...: the driver, with ENV, must exit non-zero with PATTERN
# (an extended regex) in its error output and no CERTIFIED line.
expect() {
    local case=$1 pattern=$2 envs=() status=0
    shift 2
    while [[ $1 != -- ]]; do envs+=("$1"); shift; done
    shift
    env -u CERTIFY_ALLOW_UNPINNED -u CERTIFY_ALLOW_FOREIGN_SOLAR -u CI -u GITHUB_ACTIONS SOLAR="$solar" "${envs[@]}" \
        "$root/scripts/certify" "$@" > "$tmp/out" 2>&1 || status=$?
    if [[ $status -ne 0 ]] && grep -Eq "$pattern" "$tmp/out" && ! grep -q '^CERTIFIED' "$tmp/out"; then
        echo "ok   $case: $(grep -Eo "$pattern" "$tmp/out" | head -1)"
    else
        echo "FAIL $case (exit $status, expected /$pattern/):"
        sed 's/^/  /' "$tmp/out" | tail -20
        failures=$((failures + 1))
    fi
}

expect unpinned-checkout "is at [0-9a-f]{40}, not the pinned $pin" \
    SOLIDITY_LEAN_DIR="$lean" -- "$tmp/counter.sol"
expect ci-unpinned "CERTIFY_ALLOW_UNPINNED is refused in CI" \
    CI=true SOLIDITY_LEAN_DIR="$lean" CERTIFY_ALLOW_UNPINNED=1 -- "$tmp/counter.sol"
expect ci-foreign-solar "CERTIFY_ALLOW_FOREIGN_SOLAR is refused in CI" \
    GITHUB_ACTIONS=true CERTIFY_ALLOW_FOREIGN_SOLAR=1 -- "$tmp/counter.sol"
expect bad-name "is not lowercase letters" \
    SOLIDITY_LEAN_DIR="$lean" -- "$tmp/counter.sol" --name Bad-Name
expect no-source "no such file" -- "$tmp/missing.sol"
# The remaining cases accept the scratch checkout to reach the compiler steps.
allow=(SOLIDITY_LEAN_DIR="$lean" CERTIFY_ALLOW_UNPINNED=1 CERTIFY_EXAMPLES=Examples)
version="$("$solar" --version | sed -n 's/^Commit SHA: //p')"
if git -C "$root" cat-file -e "$version^{commit}" 2>/dev/null &&
    git -C "$root" diff --quiet "$version" HEAD -- crates Cargo.toml Cargo.lock &&
    git -C "$root" diff --quiet HEAD -- crates Cargo.toml Cargo.lock; then
    echo "skip foreign-solar: $solar is built from this checkout"
else
    expect foreign-solar "whose compiler sources are not this checkout's" "${allow[@]}" -- "$tmp/counter.sol"
    allow+=(CERTIFY_ALLOW_FOREIGN_SOLAR=1)
fi
expect two-contracts "no single contract with runtime code" "${allow[@]}" -- "$tmp/two.sol"
expect no-such-contract "has no contract Nope" "${allow[@]}" -- "$tmp/counter.sol" --contract Nope
mkdir -p "$lean/Examples/counter"
echo 6080 > "$lean/Examples/counter/bytecode.hex"
expect bytecode-pin "the bytecode is not the pinned Examples/counter/bytecode.hex" \
    "${allow[@]}" -- "$tmp/counter.sol"
rm "$lean/Examples/counter/bytecode.hex"
echo 'contract C { function f( }' > "$tmp/broken.sol"
expect not-compiling "solar does not compile" "${allow[@]}" -- "$tmp/broken.sol"

# Malformed inputs, before any Lean runs: each fails closed with its own error.
opts="$lean/Examples/counter/options"
printf -- '-Ogas\n\n--evm-version\nosaka\n' > "$opts"
expect options-empty-line "is not one word of visible characters" "${allow[@]}" -- "$tmp/counter.sol"
printf -- '-Ogas\n--evm-version\nosaka \n' > "$opts"
expect options-trailing-space "'osaka\\\\ ' is not one word of visible characters" "${allow[@]}" -- "$tmp/counter.sol"
printf -- '-Ogas\r\n--evm-version\r\nosaka\r\n' > "$opts"
expect options-crlf "is not one word of visible characters" "${allow[@]}" -- "$tmp/counter.sol"
: > "$opts"
expect options-empty-file "no solar options" "${allow[@]}" -- "$tmp/counter.sol"
rm -f "$opts"
expect options-driver-flag "emit=asm is set by the driver" "${allow[@]}" -- "$tmp/counter.sol" -- --emit=asm
expect options-second-source "extra.sol names a file" "${allow[@]}" -- "$tmp/counter.sol" -- -Ogas extra.sol
expect options-stray-value "osaka is neither a flag nor the value of one" "${allow[@]}" -- "$tmp/counter.sol" -- osaka
expect options-empty-word "'' is not one word" "${allow[@]}" -- "$tmp/counter.sol" -- -Ogas ""
echo many > "$lean/Examples/counter/loops"
expect loops-malformed "Examples/counter/loops is not a number" "${allow[@]}" -- "$tmp/counter.sol"
rm -f "$lean/Examples/counter/loops"
: > "$lean/Examples/counter/bytecode.hex"
expect bytecode-pin-empty "the pinned Examples/counter/bytecode.hex is empty" "${allow[@]}" -- "$tmp/counter.sol"
rm "$lean/Examples/counter/bytecode.hex"

# Producers that fail after printing plausible output, and outputs with the bytecode missing,
# empty or malformed: a stand-in solar, an ELF executable (the driver refuses scripts) that runs
# a script printing what FAKE_SOLAR_MODE says and exiting as it says.
if command -v cc > /dev/null; then
    cat > "$tmp/shim.c" <<'EOF'
#include <stdlib.h>
#include <unistd.h>
int main(int argc, char **argv) {
    char *script = getenv("FAKE_SOLAR_SCRIPT");
    char **args = calloc(argc + 2, sizeof *args);
    if (!script || !args) return 99;
    args[0] = "bash";
    args[1] = script;
    for (int i = 1; i < argc; i++) args[i + 1] = argv[i];
    execv("/bin/bash", args);
    return 98;
}
EOF
    cc -O -o "$tmp/fake-solar" "$tmp/shim.c"
    cat > "$tmp/fake-solar.sh" <<'EOF'
set -euo pipefail
if [[ "${1:-}" == --version ]]; then printf 'solar (stand-in)\nCommit SHA: %s\n' "$FAKE_SOLAR_COMMIT"; exit 0; fi
src=$1
shift
stages=
for a in "$@"; do [[ "$a" != -Zemit-lean-stages=* ]] || stages=${a#*=}; done
out() { printf '{"contracts":{"%s:Counter":%s}}\n' "$src" "$1"; }
good='{"bin-runtime":"6080604052"}'
case "$FAKE_SOLAR_MODE:${stages:+export}" in
    compile-nonzero:) out "$good"; exit 1 ;;
    truncated-json:) printf '{"contracts":{"%s:Counter":{"bin-run' "$src" ;;
    no-bin-runtime:) out '{}' ;;
    null-bin-runtime:) out '{"bin-runtime":null}' ;;
    empty-bin-runtime:) out '{"bin-runtime":""}' ;;
    odd-hex:) out '{"bin-runtime":"608"}' ;;
    prefixed-hex:) out '{"bin-runtime":"0x6080"}' ;;
    export-nonzero:export) mkdir -p "$stages/Counter"; echo '{}' > "$stages/Counter/manifest.json"; out "$good"; exit 3 ;;
    export-no-bin-runtime:export) out '{}' ;;
    *) out "$good" ;;
esac
EOF
    fake=("${allow[@]}" SOLAR="$tmp/fake-solar" FAKE_SOLAR_SCRIPT="$tmp/fake-solar.sh"
        FAKE_SOLAR_COMMIT="$(git -C "$root" rev-parse HEAD)" CERTIFY_ALLOW_FOREIGN_SOLAR=1)
    expect solar-nonzero-after-output "solar does not compile .*\(exit 1\)" "${fake[@]}" FAKE_SOLAR_MODE=compile-nonzero -- "$tmp/counter.sol"
    expect solar-truncated-json "cannot read solar's output for .* as its JSON" "${fake[@]}" FAKE_SOLAR_MODE=truncated-json -- "$tmp/counter.sol"
    expect bytecode-missing "has no runtime bytecode \(bin-runtime\) for" "${fake[@]}" FAKE_SOLAR_MODE=no-bin-runtime -- "$tmp/counter.sol" --contract Counter
    expect bytecode-null "has no runtime bytecode \(bin-runtime\) for" "${fake[@]}" FAKE_SOLAR_MODE=null-bin-runtime -- "$tmp/counter.sol" --contract Counter
    expect bytecode-empty "the runtime bytecode solar emits for Counter is empty" "${fake[@]}" FAKE_SOLAR_MODE=empty-bin-runtime -- "$tmp/counter.sol" --contract Counter
    expect bytecode-odd-length "is not hex bytes: '608'" "${fake[@]}" FAKE_SOLAR_MODE=odd-hex -- "$tmp/counter.sol"
    expect bytecode-0x-prefix "is not hex bytes: '0x6080'" "${fake[@]}" FAKE_SOLAR_MODE=prefixed-hex -- "$tmp/counter.sol"
    expect exporter-nonzero-after-output "the exporter rejects Counter \(exit 3" "${fake[@]}" FAKE_SOLAR_MODE=export-nonzero -- "$tmp/counter.sol"
    expect exporter-bytecode-missing "the export of Counter has no runtime bytecode" "${fake[@]}" FAKE_SOLAR_MODE=export-no-bin-runtime -- "$tmp/counter.sol"
else
    echo "skip fake-solar: no C compiler (cc) on PATH"
fi

# The driver's helpers (tools/certify/inputs.sh) and checkers on their own, with the producers and
# records the driver cannot meet before Lean runs.
# unit CASE PATTERN CODE: CODE, run by bash with set -euo pipefail and tools/certify/inputs.sh
# sourced, must exit non-zero with PATTERN in its output and never print REACHED; unit_ok CASE
# PATTERN CODE must exit 0 with PATTERN in its output.
unit_run() {
    local status=0
    bash -c "set -euo pipefail; source '$root/tools/certify/inputs.sh'; $1" > "$tmp/out" 2>&1 || status=$?
    return "$status"
}
unit() {
    local case=$1 pattern=$2 status=0
    unit_run "$3" || status=$?
    if [[ $status -ne 0 ]] && grep -Eq -- "$pattern" "$tmp/out" && ! grep -q REACHED "$tmp/out"; then
        echo "ok   $case: $(grep -Eo -- "$pattern" "$tmp/out" | head -1)"
    else
        echo "FAIL $case (exit $status, expected /$pattern/):"; sed 's/^/  /' "$tmp/out" | tail -10
        failures=$((failures + 1))
    fi
}
unit_ok() {
    local case=$1 pattern=$2 status=0
    unit_run "$3" || status=$?
    if [[ $status -eq 0 ]] && grep -Eq -- "$pattern" "$tmp/out"; then
        echo "ok   $case: $(grep -Eo -- "$pattern" "$tmp/out" | head -1)"
    else
        echo "FAIL $case (exit $status, expected success with /$pattern/):"; sed 's/^/  /' "$tmp/out" | tail -10
        failures=$((failures + 1))
    fi
}

# Why capture exists: the old pattern reads a failing producer's lines and goes on.
unit_ok legacy-mapfile-hides-status "mapfile < <\(producer\) read 2 lines of a producer that exits 3 and went on" \
    'p() { echo plausible; echo output; return 3; }; mapfile -t a < <(p); echo "mapfile < <(producer) read ${#a[@]} lines of a producer that exits 3 and went on"'
unit capture-nonzero-after-output "the producer fails \(exit 3\)" \
    'p() { echo plausible; echo output; return 3; }; capture a "the producer" p; echo REACHED'
unit capture-set-e-inside "the producer fails \(exit 1\)" \
    'p() { false; echo plausible; }; capture a "the producer" p; echo REACHED'
unit capture-pipe-inside "the producer fails \(exit 1\)" \
    'p() { false | sort; echo plausible; }; capture a "the producer" p; echo REACHED'
unit capture-killed "the producer fails \(exit 137\)" \
    'p() { echo plausible; kill -9 "$BASHPID"; }; capture a "the producer" p; echo REACHED'
unit capture-nonempty-empty "the producer prints nothing" \
    'p() { :; }; capture_nonempty a "the producer" p; echo REACHED'
unit_ok capture-ok "captured 2 lines: a b" \
    'p() { printf "a\nb\n"; }; capture a "the producer" p; echo "captured ${#a[@]} lines: ${a[*]}"'
unit entries-unreadable "the entries fails \(exit 2\)" \
    "capture a 'the entries' entries '$tmp/no-such-file'; echo REACHED"
unit tree-diff-crash "clean_tree fails \(exit 2\)" \
    'tree_diff d clean_tree python3 -c "print(\"a plausible difference\"); raise SystemExit(2)"; echo REACHED'
unit tree-diff-silent-exit-1 "exit 1 with no difference listed" \
    'tree_diff d clean_tree python3 -c "raise SystemExit(1)"; echo REACHED'
mkdir -p "$tmp/notgit"
unit clean-tree-not-a-checkout "clean_tree.py on a directory that is not a checkout fails \(exit 2\)" \
    "tree_diff d 'clean_tree.py on a directory that is not a checkout' python3 '$root/tools/certify/clean_tree.py' '$tmp/notgit'; echo REACHED"

# The record of the published statement: missing, empty, truncated or incomplete fails.
cat > "$tmp/final.json" <<'EOF'
{"module": "counter.Final", "statement_module": "counter.Statement", "theorem": "Certified.counter.Final.final",
 "kind": "compiled", "statement": "Shapes.Compiled", "assumptions": [], "excluded": [], "strongest": false,
 "statuses": {"equivalence": "proved", "termination": "proved", "spec": "not applicable", "resources": "proved"}}
EOF
unit_ok final-record-whole "record accepted" "check_final_record '$tmp/final.json'; echo record accepted"
unit final-record-missing "no record" "check_final_record '$tmp/no-record.json'; echo REACHED"
: > "$tmp/final-empty.json"
unit final-record-empty "final-empty.json is empty" "check_final_record '$tmp/final-empty.json'; echo REACHED"
head -c 90 "$tmp/final.json" > "$tmp/final-cut.json"
unit final-record-truncated "final-cut.json is not JSON" "check_final_record '$tmp/final-cut.json'; echo REACHED"
python3 -c 'import json, sys; d = json.load(open(sys.argv[1])); del d["theorem"]; d["strongest"] = "yes"; json.dump(d, open(sys.argv[2], "w"))' \
    "$tmp/final.json" "$tmp/final-incomplete.json"
unit final-record-incomplete "lacks theorem, strongest" "check_final_record '$tmp/final-incomplete.json'; echo REACHED"
python3 -c 'import json, sys; d = json.load(open(sys.argv[1])); del d["statuses"]["resources"]; json.dump(d, open(sys.argv[2], "w"))' \
    "$tmp/final.json" "$tmp/final-nostatus.json"
unit final-record-no-status "has no status resources" "check_final_record '$tmp/final-nostatus.json'; echo REACHED"
unit run-record-missing "the run's record .* is missing or malformed" "check_run_record '$tmp/no-record.json'; echo REACHED"

# spec.sh against a stand-in solar-verify: a run that exits non-zero after writing a whole
# module, one cut short before its summary line, one whose status contradicts its summary, and a
# truncated module all fail; a whole one passes.
cat > "$tmp/fake-verify" <<'EOF'
#!/usr/bin/env bash
# --dry-run --dump-lean FILE --lean-namespace NS SOURCE
out=$3 ns=$5
whole() {
    printf -- '-- Generated by solar-verify --dump-lean. Do not edit.\n\nimport Verify.VC.Term\n\nnamespace %s\n\n' "$ns"
    printf 'open SolidCore.Verify.VC\n\n/-- The commands solar-verify sent the solver. -/\ndef script : List Cmd := [\n  .checkSat\n]\n\n'
    printf '/-- The transcript, as solar-verify wrote it. -/\ndef transcript : String := "(check-sat)\\n"\n\nend %s\n' "$ns"
}
case $FAKE_VERIFY_MODE in
    nonzero-after-output) whole > "$out"; echo "verification results:: 1 verified, 0 errors"; exit 3 ;;
    no-summary) whole > "$out"; echo "checking C.inc"; exit 1 ;;
    status-contradicts) whole > "$out"; echo "verification results:: 1 verified, 0 errors"; exit 1 ;;
    truncated) whole | head -c 150 > "$out"; echo "verification results:: 1 verified, 0 errors" ;;
    cut-at-a-line) whole | sed '$d' > "$out"; echo "verification results:: 1 verified, 0 errors" ;;
    whole) whole > "$out"; echo "verification results:: 1 verified, 0 errors" ;;
esac
EOF
chmod +x "$tmp/fake-verify"
# expect_spec CASE PATTERN MODE: spec.sh with the stand-in in MODE must fail with PATTERN.
expect_spec() {
    local status=0
    FAKE_VERIFY_MODE=$3 bash "$root/tools/certify/spec.sh" "$tmp/fake-verify" "$tmp/counter.sol" "$tmp/spec-$1" T.Counter > "$tmp/out" 2>&1 || status=$?
    if [[ $status -ne 0 ]] && grep -Eq -- "$2" "$tmp/out"; then
        echo "ok   $1: $(grep -Eo -- "$2" "$tmp/out" | head -1)"
    else
        echo "FAIL $1 (exit $status, expected /$2/):"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
    fi
}
expect_spec verify-nonzero-after-output "solar-verify fails on .*\(exit 3\)" nonzero-after-output
expect_spec verify-no-summary "solar-verify exits 1 without its summary line" no-summary
expect_spec verify-status-contradicts "solar-verify exits 1 but reports 0 errors" status-contradicts
expect_spec vc-export-truncated "does not end with a newline: it is truncated" truncated
expect_spec vc-export-cut-at-a-line "does not end with 'end T.Counter' .*: it is truncated" cut-at-a-line
if FAKE_VERIFY_MODE=whole bash "$root/tools/certify/spec.sh" "$tmp/fake-verify" "$tmp/counter.sol" "$tmp/spec-whole" T.Counter > "$tmp/out" 2>&1 &&
    [[ -f "$tmp/spec-whole/refused" && ! -s "$tmp/spec-whole/refused" ]]; then
    echo "ok   vc-export-whole: $(grep -o 'is whole.*' "$tmp/out")"
else
    echo "FAIL vc-export-whole:"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
fi

# The kernel replay's accounting (tools/certify/replay.py) on a closure of stand-in oleans: the
# toolchain's Init.Core and Mathlib's Mathlib.Logic trusted by directory, the contract's three
# modules to replay. Every module must be replayed by leanchecker or trusted by directory.
rp="$tmp/replay"
mkdir -p "$rp/tc/lib/lean/Init" "$rp/mathlib/.lake/build/lib/lean/Mathlib" "$rp/mathlib/.lake/build/lib/lean/Evil" \
    "$rp/pkg/.lake/build/lib/lean/counter"
lib="$rp/pkg/.lake/build/lib/lean/counter"
for f in tc/lib/lean/Init/Core mathlib/.lake/build/lib/lean/Mathlib/Logic mathlib/.lake/build/lib/lean/Evil/X \
    pkg/.lake/build/lib/lean/counter/Statement pkg/.lake/build/lib/lean/counter/Shape pkg/.lake/build/lib/lean/counter/Final; do
    echo "$f" > "$rp/$f.olean"
done
closure() {
    printf 'MODULE Init.Core %s trusted\n' "$rp/tc/lib/lean/Init/Core.olean"
    printf 'MODULE Mathlib.Logic %s trusted Init.Core\n' "$rp/mathlib/.lake/build/lib/lean/Mathlib/Logic.olean"
    printf 'MODULE counter.Statement %s replay Init.Core Mathlib.Logic\n' "$lib/Statement.olean"
    printf 'MODULE counter.Shape %s replay counter.Statement\n' "$lib/Shape.olean"
    printf 'MODULE counter.Final %s replay counter.Statement counter.Shape\n' "$lib/Final.olean"
    echo "check_final: Certified.counter.Final.final : Certified.counter.Final.Statement"
}
closure > "$rp/closure"
printf 'replaying counter.Statement\nreplaying counter.Shape\nreplaying counter.Final\n' > "$rp/all.out"
# replay_case CASE PATTERN CLOSURE CHECKER_OUT STATUS [PLAN_ARGS...]: plan and account must fail
# with PATTERN (replay_ok: pass with it).
replay_run() {
    local closure=$1 out=$2 st=$3
    shift 3
    CERTIFY_TRUSTED_ROOTS="$rp/mathlib=Mathlib;" python3 "$root/tools/certify/replay.py" plan "$closure" "$rp/plan" \
        --syslib "$rp/tc/lib/lean" --require counter.Final --require counter.Statement --require counter.Shape "$@" &&
        python3 "$root/tools/certify/replay.py" account "$rp/plan" "$out" "$st" "$rp/receipt.json"
}
replay_case() {
    local case=$1 pattern=$2 status=0
    shift 2
    replay_run "$@" > "$tmp/out" 2>&1 || status=$?
    if [[ $status -ne 0 ]] && grep -Eq -- "$pattern" "$tmp/out"; then
        echo "ok   $case: $(grep -Eo -- "$pattern" "$tmp/out" | head -1)"
    else
        echo "FAIL $case (exit $status, expected /$pattern/):"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
    fi
}
replay_ok() {
    local case=$1 pattern=$2 status=0
    shift 2
    replay_run "$@" > "$tmp/out" 2>&1 || status=$?
    if [[ $status -eq 0 ]] && grep -Eq -- "$pattern" "$tmp/out"; then
        echo "ok   $case: $(grep -Eo -- "$pattern" "$tmp/out" | head -1)"
    else
        echo "FAIL $case (exit $status, expected success with /$pattern/):"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
    fi
}
replay_ok replay-accounted "kernel replay: 3 of 3 .* 2 trusted by directory .*0 unaccounted; closure 5 = 3 \+ 2" \
    "$rp/closure" "$rp/all.out" 0
printf 'replaying %s\n' Init.Core Mathlib.Logic counter.Statement counter.Shape counter.Final > "$rp/every.out"
replay_ok replay-include-trusted "kernel replay: 5 of 5 .*closure 5 = 5 \+ 0" "$rp/closure" "$rp/every.out" 0 --include-trusted
printf 'replaying counter.Statement\nreplaying counter.Final\n' > "$rp/two.out"
replay_case replay-module-missing "1 of 3 modules of the closure were not replayed by leanchecker in this run: counter.Shape" \
    "$rp/closure" "$rp/two.out" 0
replay_case replay-checker-silent "3 of 3 modules of the closure were not replayed" "$rp/closure" /dev/null 0
replay_case replay-checker-nonzero-after-output "leanchecker exits 1: the kernel replay of the closure fails" "$rp/closure" "$rp/all.out" 1
closure | sed 's/^\(MODULE counter.Shape [^ ]*\) replay/\1 trusted/' > "$rp/c-trusted"
replay_case replay-trusted-outside-roots "unaccounted for: counter.Shape .* is marked trusted but is under no trusted directory" \
    "$rp/c-trusted" "$rp/all.out" 0
{ closure; printf 'MODULE Evil.X %s trusted\n' "$rp/mathlib/.lake/build/lib/lean/Evil/X.olean"; } > "$rp/c-evil"
replay_case replay-root-not-the-packages "unaccounted for: Evil.X" "$rp/c-evil" "$rp/all.out" 0
{ closure; echo "MODULE counter.Extra"; } > "$rp/c-malformed"
replay_case replay-malformed-line "malformed closure line" "$rp/c-malformed" "$rp/all.out" 0
{ closure; closure | grep 'MODULE counter.Shape'; } > "$rp/c-dup"
replay_case replay-duplicate-module "module counter.Shape is listed twice" "$rp/c-dup" "$rp/all.out" 0
replay_case replay-empty-closure "lists no module of the closure" /dev/null "$rp/all.out" 0
closure | grep -v 'MODULE counter.Final' > "$rp/c-nofinal"
replay_case replay-final-missing "the closure has no module counter.Final" "$rp/c-nofinal" "$rp/all.out" 0
closure | sed 's|Shape.olean|Gone.olean|' > "$rp/c-gone"
replay_case replay-olean-missing "the olean of counter.Shape at .* does not exist" "$rp/c-gone" "$rp/all.out" 0
# The replay must have run the toolchain's own leanchecker, by its real path.
mkdir -p "$rp/tc/bin" "$rp/pkg/.lake/build/bin"
printf '#!/bin/sh\n' > "$rp/tc/bin/leanchecker"
printf '#!/bin/sh\nexit 0\n' > "$rp/pkg/.lake/build/bin/leanchecker"
replay_ok replay-toolchain-checker "kernel replay: 3 of 3" "$rp/closure" "$rp/all.out" 0 &&
    python3 "$root/tools/certify/replay.py" account "$rp/plan" "$rp/all.out" 0 "$rp/receipt.json" \
        --checker "$rp/tc/bin/leanchecker" --toolchain "$rp/tc" > /dev/null
if ! python3 "$root/tools/certify/replay.py" account "$rp/plan" "$rp/all.out" 0 "$rp/receipt.json" \
    --checker "$rp/pkg/.lake/build/bin/leanchecker" --toolchain "$rp/tc" > "$tmp/out" 2>&1 &&
    grep -q "not the toolchain's leanchecker" "$tmp/out"; then
    echo "ok   replay-planted-checker: $(grep -o "the replay ran .*bin/leanchecker (" "$tmp/out" | head -1)... not the toolchain's leanchecker"
else
    echo "FAIL replay-planted-checker:"; sed 's/^/  /' "$tmp/out" | tail -5; failures=$((failures + 1))
fi
# An olean changed between the plan and the account.
if CERTIFY_TRUSTED_ROOTS="$rp/mathlib=Mathlib;" python3 "$root/tools/certify/replay.py" plan "$rp/closure" "$rp/plan" \
    --syslib "$rp/tc/lib/lean" > /dev/null && echo changed >> "$lib/Shape.olean" &&
    ! python3 "$root/tools/certify/replay.py" account "$rp/plan" "$rp/all.out" 0 "$rp/receipt.json" > "$tmp/out" 2>&1 &&
    grep -q "oleans of the closure changed since they were planned: counter.Shape" "$tmp/out"; then
    echo "ok   replay-olean-changed: $(grep -o 'oleans of the closure changed.*' "$tmp/out")"
else
    echo "FAIL replay-olean-changed:"; sed 's/^/  /' "$tmp/out" | tail -5; failures=$((failures + 1))
fi

# The audits themselves, when Lean is on PATH: a theorem that uses a declared axiom, native_decide
# or sorry must fail the axiom audit, their file must fail the scan, and a clean theorem passes.
if command -v lake > /dev/null; then
    pkg="$tmp/audit"
    mkdir -p "$pkg"
    # The toolchain solar's Lean pins (elan reads it; a plain Lean install ignores it).
    echo "${CERTIFY_LEAN_TOOLCHAIN:-leanprover/lean4:v4.35.0}" > "$pkg/lean-toolchain"
    printf 'name = "audit"\n\n[[lean_lib]]\nname = "Bad"\n\n[[lean_lib]]\nname = "Good"\n' > "$pkg/lakefile.toml"
    cat > "$pkg/Bad.lean" <<'EOF'
axiom certify_bad : False
theorem uses_axiom : (1 : Nat) = 2 := certify_bad.elim
theorem uses_native : 2 + 2 = 4 := by native_decide
theorem uses_sorry : (1 : Nat) = 2 := by sorry
theorem clean : ∀ n : Nat, n + 0 = n := fun _ => rfl
EOF
    printf 'theorem good (p : Prop) [Decidable p] : p ∨ ¬p := Classical.em p\n' > "$pkg/Good.lean"
    if (cd "$pkg" && lake build Bad Good > "$tmp/audit-build.log" 2>&1); then
        # expect_audit CASE PATTERN THEOREM: the audit of THEOREM must fail with PATTERN.
        expect_audit() {
            local case=$1 pattern=$2 status=0
            bash "$root/tools/certify/audit.sh" axioms "$pkg" "$tmp/$case.out" Bad -- "$3" > "$tmp/out" 2>&1 || status=$?
            if [[ $status -ne 0 ]] && grep -Eq "$pattern" "$tmp/out"; then
                echo "ok   $case: $(grep -Eo "$pattern" "$tmp/out" | head -1)"
            else
                echo "FAIL $case (exit $status, expected /$pattern/):"; sed 's/^/  /' "$tmp/out" | tail -10
                failures=$((failures + 1))
            fi
        }
        expect_audit audit-axiom "axioms beyond .*certify_bad" uses_axiom
        expect_audit audit-native-decide "axioms beyond .*(native_decide|ofReduceBool)[A-Za-z0-9_.]*" uses_native
        expect_audit audit-sorry "axioms beyond .*sorryAx" uses_sorry
        expect_audit audit-missing "unknown (constant|identifier)|does not check" no_such_theorem
        if bash "$root/tools/certify/audit.sh" axioms "$pkg" "$tmp/clean.out" Bad Good -- clean good > "$tmp/out" 2>&1; then
            echo "ok   audit-clean: clean and good pass"
        else
            echo "FAIL audit-clean:"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
        fi
        if bash "$root/tools/certify/audit.sh" scan "$pkg/Bad.lean" > "$tmp/out" 2>&1; then
            echo "FAIL scan-bad: Bad.lean passes the scan"; failures=$((failures + 1))
        elif [[ "$(grep -cE '^[^:]+:[0-9]+:' "$tmp/out")" -eq 3 ]]; then
            echo "ok   scan-bad: the axiom, native_decide and sorry lines are flagged"
        else
            echo "FAIL scan-bad: expected 3 flagged lines:"; sed 's/^/  /' "$tmp/out"; failures=$((failures + 1))
        fi
        if bash "$root/tools/certify/audit.sh" scan "$pkg/Good.lean" > "$tmp/out" 2>&1; then
            echo "ok   scan-good: Good.lean passes the scan"
        else
            echo "FAIL scan-good:"; sed 's/^/  /' "$tmp/out"; failures=$((failures + 1))
        fi
        # The replay accounting against the real checker and leanchecker, as the driver runs it:
        # every module of Good's closure (Good and each toolchain module it imports) replayed by
        # leanchecker in this run, none trusted; and leanchecker's output for another module does
        # not account for Good.
        prefix="$(cd "$pkg" && lake env lean --print-prefix)"
        syslib="$prefix/lib/lean"
        if (cd "$pkg" && lake env lean --run "$root/tools/certify/check_final.lean" --closure Good) > "$tmp/good.closure" 2>&1 &&
            python3 "$root/tools/certify/replay.py" plan "$tmp/good.closure" "$tmp/good.plan" --syslib "$syslib" --require Good \
                --include-trusted > "$tmp/out" 2>&1; then
            n_closure="$(grep -c '^MODULE ' "$tmp/good.closure")"
            for target in Good Bad; do
                st=0
                if [[ $target == Good ]]; then
                    gmods_out="$(awk '$1 == "replay" { print $2 }' "$tmp/good.plan")"
                    mapfile -t gmods <<<"$gmods_out"
                    (cd "$pkg" && lake env "$prefix/bin/leanchecker" --verbose "${gmods[@]}") > "$tmp/replay-$target.out" 2>&1 || st=$?
                else
                    (cd "$pkg" && lake env "$prefix/bin/leanchecker" --verbose "$target") > "$tmp/replay-$target.out" 2>&1 || st=$?
                fi
                if python3 "$root/tools/certify/replay.py" account "$tmp/good.plan" "$tmp/replay-$target.out" "$st" \
                    "$tmp/good.receipt" --checker "$prefix/bin/leanchecker" --toolchain "$prefix" > "$tmp/account-$target.out" 2>&1; then
                    echo ok > "$tmp/account-$target.status"
                else
                    echo failed > "$tmp/account-$target.status"
                fi
            done
            if [[ "$(cat "$tmp/account-Good.status")" == ok ]] &&
                grep -q "^kernel replay: $n_closure of $n_closure .*; 0 trusted by directory (none); 0 unaccounted; closure $n_closure = $n_closure + 0" "$tmp/account-Good.out" &&
                [[ "$(grep -c '^replayed ' "$tmp/account-Good.out")" -eq "$n_closure" ]]; then
                echo "ok   lean-replay-accounted: $(cat "$tmp/out"); $(grep '^kernel replay' "$tmp/account-Good.out")"
            else
                echo "FAIL lean-replay-accounted:"; sed 's/^/  /' "$tmp/account-Good.out" "$tmp/replay-Good.out" | tail -10
                failures=$((failures + 1))
            fi
            if [[ "$(cat "$tmp/account-Bad.status")" == failed ]] && grep -Eq "^certify: error: $n_closure of $n_closure modules of the closure were not replayed by leanchecker in this run" "$tmp/account-Bad.out"; then
                echo "ok   lean-replay-other-module: $(grep -o '[0-9]* of [0-9]* modules of the closure were not replayed' "$tmp/account-Bad.out")"
            else
                echo "FAIL lean-replay-other-module:"; sed 's/^/  /' "$tmp/account-Bad.out" | tail -10; failures=$((failures + 1))
            fi
        else
            echo "FAIL lean-replay: cannot plan Good's closure:"; sed 's/^/  /' "$tmp/good.closure" "$tmp/out" | tail -10
            failures=$((failures + 1))
        fi
    else
        echo "FAIL audit: the scratch package does not build:"; tail -20 "$tmp/audit-build.log"
        failures=$((failures + 1))
    fi
else
    echo "skip audit: no lake on PATH"
fi

# The spec export, when SOLAR_VERIFY names a solar-verify binary: contracts inside the fragment
# (Counter, and a nested dynamic array, which the fragment now covers) export whole with nothing
# refused, and conditions outside it are refused, never passing: a bool mapping's function is left
# raw ("undeclared symbol"), and a struct's datatype refuses the whole export.
if [[ -n "${SOLAR_VERIFY:-}" ]]; then
    cat > "$tmp/boolmap.sol" <<'EOF'
contract C {
    mapping(uint256 => bool) m;
    /// @custom:ensures m[0] == true
    function f() external { m[0] = true; }
}
EOF
    cat > "$tmp/structmap.sol" <<'EOF'
contract C {
    struct S { uint256 a; bool b; }
    mapping(address => S) m;
    /// @custom:ensures m[msg.sender].b == true
    function f() external { m[msg.sender].b = true; }
}
EOF
    cat > "$tmp/nested.sol" <<'EOF'
contract C {
    uint256[][] n;
    /// @custom:ensures n[0][0] == 0
    function f() external { n[0][0] = 0; }
}
EOF
    if bash "$root/tools/certify/spec.sh" "$SOLAR_VERIFY" "$tmp/counter.sol" "$tmp/spec-counter" T.Counter > "$tmp/out" 2>&1 &&
        [[ -s "$tmp/spec-counter/VC.lean" && ! -s "$tmp/spec-counter/refused" ]]; then
        echo "ok   spec-counter: exported, nothing refused"
    else
        echo "FAIL spec-counter:"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
    fi
    # Supported: the nested array's conditions export whole, with no function refused or left raw.
    if bash "$root/tools/certify/spec.sh" "$SOLAR_VERIFY" "$tmp/nested.sol" "$tmp/spec-nested" T.Nested > "$tmp/out" 2>&1 &&
        [[ -f "$tmp/spec-nested/refused" && ! -s "$tmp/spec-nested/refused" ]] &&
        ! grep -q 'Left raw' "$tmp/spec-nested/VC.lean" && grep -q '; ===== C.f' "$tmp/spec-nested/VC.lean"; then
        echo "ok   spec-nested-supported: exported, C.f translated, nothing refused ($(grep -o 'is whole.*' "$tmp/out"))"
    else
        echo "FAIL spec-nested-supported:"; sed 's/^/  /' "$tmp/out" "$tmp/spec-nested/refused" | tail -10; failures=$((failures + 1))
    fi
    # Unsupported: refused, and recorded so, by function and as a whole.
    if bash "$root/tools/certify/spec.sh" "$SOLAR_VERIFY" "$tmp/boolmap.sol" "$tmp/spec-boolmap" T.Boolmap > "$tmp/out" 2>&1 &&
        grep -qx 'C.f: undeclared symbol: .*' "$tmp/spec-boolmap/refused" && [[ "$(wc -l < "$tmp/spec-boolmap/refused")" -eq 1 ]]; then
        echo "ok   spec-refused-function: $(head -1 "$tmp/spec-boolmap/refused")"
    else
        echo "FAIL spec-refused-function:"; sed 's/^/  /' "$tmp/out" "$tmp/spec-boolmap/refused" | tail -10; failures=$((failures + 1))
    fi
    if bash "$root/tools/certify/spec.sh" "$SOLAR_VERIFY" "$tmp/structmap.sol" "$tmp/spec-structmap" T.Structmap > "$tmp/out" 2>&1 &&
        grep -q '(every function): command outside the Lean fragment: (declare-datatypes' "$tmp/spec-structmap/refused" &&
        [[ ! -e "$tmp/spec-structmap/VC.lean" ]]; then
        echo "ok   spec-refused-whole: $(head -1 "$tmp/spec-structmap/refused" | cut -c1-120)"
    else
        echo "FAIL spec-refused-whole:"; sed 's/^/  /' "$tmp/out" "$tmp/spec-structmap/refused" | tail -10; failures=$((failures + 1))
    fi
    # The real exports, whole and truncated: --dump-lean's conditions (from spec-counter above) and
    # --emit-lean-spec's specification of a contract with an ensures.
    vc="$tmp/spec-counter/VC.lean"
    head -c "$(( $(wc -c < "$vc") / 2 ))" "$vc" > "$tmp/VC-cut.lean"
    if python3 "$root/tools/certify/export_check.py" vc "$vc" T.Counter > "$tmp/out" 2>&1 &&
        ! python3 "$root/tools/certify/export_check.py" vc "$tmp/VC-cut.lean" T.Counter >> "$tmp/out" 2>&1 &&
        grep -q 'VC-cut.lean .*truncated' "$tmp/out"; then
        echo "ok   vc-export-real: whole passes, half of it fails: $(grep -o 'does not end.*' "$tmp/out" | head -1)"
    else
        echo "FAIL vc-export-real:"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
    fi
    cat > "$tmp/ensures.sol" <<'EOF'
contract Counter {
    uint256 x;
    /// @custom:ensures x == old(x) + d
    function inc(uint256 d) external { x += d; }
    function get() external view returns (uint256) { return x; }
}
EOF
    sns=T.Counter.Spec
    # expect_export CASE PATTERN FILE SOURCE CONTRACT: the spec check must fail with PATTERN.
    expect_export() {
        local status=0
        python3 "$root/tools/certify/export_check.py" spec "$3" "$sns" --source "$4" --contract "$5" > "$tmp/out" 2>&1 || status=$?
        if [[ $status -ne 0 ]] && grep -Eq -- "$2" "$tmp/out"; then
            echo "ok   $1: $(grep -Eo -- "$2" "$tmp/out" | head -1)"
        else
            echo "FAIL $1 (exit $status, expected /$2/):"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
        fi
    }
    if "$SOLAR_VERIFY" --emit-lean-spec "$tmp/Spec.lean" --lean-spec-module "$sns" "$tmp/ensures.sol" > "$tmp/out" 2>&1 &&
        python3 "$root/tools/certify/export_check.py" spec "$tmp/Spec.lean" "$sns" --source "$tmp/ensures.sol" --contract Counter > "$tmp/out" 2>&1; then
        echo "ok   spec-export-whole: $(cat "$tmp/out")"
        head -c "$(( $(wc -c < "$tmp/Spec.lean") * 2 / 3 ))" "$tmp/Spec.lean" > "$tmp/Spec-cut.lean"
        expect_export spec-export-truncated "it is truncated" "$tmp/Spec-cut.lean" "$tmp/ensures.sol" Counter
        sed '$d' "$tmp/Spec.lean" > "$tmp/Spec-noend.lean"
        expect_export spec-export-no-end "does not end with 'end $sns'" "$tmp/Spec-noend.lean" "$tmp/ensures.sol" Counter
        : > "$tmp/Spec-empty.lean"
        expect_export spec-export-empty "Spec-empty.lean is empty" "$tmp/Spec-empty.lean" "$tmp/ensures.sol" Counter
        expect_export spec-export-missing "the spec export is missing" "$tmp/Spec-none.lean" "$tmp/ensures.sol" Counter
        expect_export spec-export-stale-source "is of a source with SHA-256 [0-9a-f]{64}, not" "$tmp/Spec.lean" "$tmp/counter.sol" Counter
        expect_export spec-export-other-contract "has no Bundle Token" "$tmp/Spec.lean" "$tmp/ensures.sol" Token
    else
        echo "FAIL spec-export-whole:"; sed 's/^/  /' "$tmp/out" | tail -10; failures=$((failures + 1))
    fi
else
    echo "skip spec: SOLAR_VERIFY is not set"
fi

if [[ $failures -ne 0 ]]; then
    echo "certify driver tests: $failures failed" >&2
    exit 1
fi
echo "certify driver tests: all passed"
