#!/usr/bin/env bash
# Negative tests of scripts/certify itself: each case must stop the driver with its error, never
# with a CERTIFIED line. The driver cases fail before any Lean runs; the audit cases, which need
# Lean on PATH, check that tools/certify/audit.sh rejects a declared axiom, native_decide and sorry.
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
    else
        echo "FAIL audit: the scratch package does not build:"; tail -20 "$tmp/audit-build.log"
        failures=$((failures + 1))
    fi
else
    echo "skip audit: no lake on PATH"
fi

if [[ $failures -ne 0 ]]; then
    echo "certify driver tests: $failures failed" >&2
    exit 1
fi
echo "certify driver tests: all passed"
