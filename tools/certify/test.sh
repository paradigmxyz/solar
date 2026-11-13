#!/usr/bin/env bash
# Negative tests of scripts/certify itself, which fail before any Lean runs: each case must stop
# the driver with its error, never with a CERTIFIED line.
#
#   tools/certify/test.sh
#
# SOLAR names the solar binary (else target/debug/solar). The cases run against a scratch git
# checkout standing in for solidity-lean, so they need no network and no Lean.
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
    env -u CERTIFY_ALLOW_UNPINNED -u CERTIFY_ALLOW_FOREIGN_SOLAR SOLAR="$solar" "${envs[@]}" \
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

if [[ $failures -ne 0 ]]; then
    echo "certify driver tests: $failures failed" >&2
    exit 1
fi
echo "certify driver tests: all passed"
