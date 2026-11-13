#!/usr/bin/env bash
# Prints the path of the solidity-lean checkout at the commit tools/certify/solidity-lean.rev
# pins, which holds all of the Lean that solar's certification and validator use.
#
#   dir="$(tools/certify/checkout.sh)"
#
# SOLIDITY_LEAN_DIR names an existing checkout, which must be at the pin unless
# CERTIFY_ALLOW_UNPINNED=1 (then a line on stderr says so; refused in CI); otherwise the script fetches the pin
# into target/certify/solidity-lean, a checkout it owns and resets on every run (keeping .lake).
set -euo pipefail

if [[ "${CI:-}" == true || -n "${GITHUB_ACTIONS:-}" ]] && [[ -n "${CERTIFY_ALLOW_UNPINNED:-}" ]]; then
    echo "certify: error: CERTIFY_ALLOW_UNPINNED is refused in CI" >&2
    exit 1
fi
root="$(cd "$(dirname "$(realpath "${BASH_SOURCE[0]}")")/../.." && pwd)"
die() { echo "certify: error: $*" >&2; exit 1; }
pin="$(tr -d '[:space:]' < "$root/tools/certify/solidity-lean.rev")"
[[ "$pin" =~ ^[0-9a-f]{40}$ ]] || die "tools/certify/solidity-lean.rev does not hold a full commit sha"
if [[ -n "${SOLIDITY_LEAN_DIR:-}" ]]; then
    dir="$(realpath "$SOLIDITY_LEAN_DIR")"
    head="$(git -C "$dir" rev-parse HEAD)" || die "$dir is not a git checkout"
    if [[ "$head" != "$pin" ]]; then
        [[ "${CERTIFY_ALLOW_UNPINNED:-}" == 1 ]] ||
            die "$dir is at $head, not the pinned $pin (CERTIFY_ALLOW_UNPINNED=1 accepts it)"
        echo "certify: warning: solidity-lean at $head, not the pinned $pin" >&2
    fi
else
    dir="$root/target/certify/solidity-lean"
    # A .git that is not a repository of its own (such as one a cache restored hollow) would let
    # git walk up to the enclosing checkout: start that one afresh.
    if [[ "$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null)" != "$dir" ]]; then
        rm -rf "$dir/.git"
        mkdir -p "$dir"
        git -C "$dir" init -q
        git -C "$dir" remote add origin https://github.com/paradigmxyz/solidity-lean.git
    fi
    [[ "$(git -C "$dir" rev-parse --show-toplevel)" == "$dir" ]] || die "$dir is not a git checkout of its own"
    if ! git -C "$dir" cat-file -e "$pin^{commit}" 2>/dev/null; then
        git -C "$dir" fetch -q --depth 1 origin "$pin" >&2 || die "cannot fetch solidity-lean $pin"
    fi
    git -C "$dir" checkout -q --force --detach "$pin" >&2
    git -C "$dir" clean -q -fdx -e .lake >&2
    git -C "$dir" submodule update -q --init --recursive >&2
fi
echo "$dir"
