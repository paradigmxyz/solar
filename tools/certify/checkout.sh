#!/usr/bin/env bash
# Prints the path of the solidity-lean checkout at the commit tools/certify/solidity-lean.rev
# pins, which holds all of the Lean that solar's certification and validator use.
#
#   dir="$(tools/certify/checkout.sh [INPUTS_DIR])"
#
# SOLIDITY_LEAN_DIR names an existing checkout, which must be at the pin, or, with CERTIFY_PIN_BASE
# (which must be the pin), one clean commit on it whose changes are all under INPUTS_DIR (such as
# Certified/NAME): its HEAD's only parent is the pin, and git status shows nothing; anything else
# is refused, naming the path or commit, and a line on stderr records the pin, the inputs commit and
# its tree. CERTIFY_ALLOW_UNPINNED=1 accepts any other commit (then a line on stderr says so; refused
# in CI and with CERTIFY_PIN_BASE). Without SOLIDITY_LEAN_DIR the script fetches the pin
# into target/certify/solidity-lean, a checkout it owns and resets on every run (keeping .lake).
set -euo pipefail
die() { echo "certify: error: $*" >&2; exit 1; }

here="$(cd "$(dirname "$(realpath "${BASH_SOURCE[0]}")")" && pwd)"

# Refuses, naming the paths, an inputs commit HEAD on BASE in the checkout DIR that changes a path
# outside INPUTS_DIR/, or leaves under INPUTS_DIR/ a file git ignores (one the run reads but the
# commit does not hold). The one exception is lakefile.lean, and only as tools/certify/
# lakefile_inputs.py allows: the base's bytes plus the driver's library stanza for the NAME of
# INPUTS_DIR (Certified/NAME), which the base does not declare. generic.py checks the inputs commit
# it makes with this same function, through checkout.sh --check-inputs DIR BASE HEAD INPUTS_DIR, so
# both accept the same files.
check_inputs() {
    local changed outside ignored why
    [[ "$4" =~ ^[A-Za-z0-9_][A-Za-z0-9_./-]*[A-Za-z0-9_]$ && "$4" != *..* ]] ||
        die "CERTIFY_PIN_BASE needs the inputs directory, not '$4'"
    changed="$(git -C "$1" diff --no-renames --name-only "$2" "$3")" || die "cannot diff $2..$3"
    outside="$(awk -v p="$4/" 'index($0, p) != 1' <<<"$changed")"
    if [[ "$outside" == lakefile.lean ]]; then
        if why="$(python3 -I "$here/lakefile_inputs.py" check "$1" "$2" "$3" "$4" 2>&1)"; then
            outside=""
        else
            die "the inputs commit $3 touches paths outside $4/: lakefile.lean ($why)"
        fi
    fi
    [[ -z "$outside" ]] ||
        die "the inputs commit $3 touches paths outside $4/: $(head -5 <<<"$outside" | tr '\n' ' ')"
    ignored="$(git -C "$1" ls-files --full-name --others --ignored --exclude-standard -- "$4/")" ||
        die "cannot list the ignored files of $4/"
    [[ -z "$ignored" ]] ||
        die "the inputs commit $3 leaves out files git ignores under $4/: $(head -5 <<<"$ignored" | tr '\n' ' ')"
}
if [[ "${1:-}" == --check-inputs ]]; then
    [[ $# -eq 5 ]] || die "usage: checkout.sh --check-inputs DIR BASE HEAD INPUTS_DIR"
    check_inputs "$2" "$3" "$4" "$5"
    exit 0
fi

if [[ "${CI:-}" == true || -n "${GITHUB_ACTIONS:-}" ]] && [[ -n "${CERTIFY_ALLOW_UNPINNED:-}" ]]; then
    echo "certify: error: CERTIFY_ALLOW_UNPINNED is refused in CI" >&2
    exit 1
fi
root="$(cd "$(dirname "$(realpath "${BASH_SOURCE[0]}")")/../.." && pwd)"
pin="$(tr -d '[:space:]' < "$root/tools/certify/solidity-lean.rev")"
[[ "$pin" =~ ^[0-9a-f]{40}$ ]] || die "tools/certify/solidity-lean.rev does not hold a full commit sha"
inputs_dir=${1:-}
if [[ -n "${CERTIFY_PIN_BASE:-}" ]]; then
    [[ "$CERTIFY_PIN_BASE" == "$pin" ]] ||
        die "CERTIFY_PIN_BASE $CERTIFY_PIN_BASE is not the pin $pin of tools/certify/solidity-lean.rev"
    [[ -z "${CERTIFY_ALLOW_UNPINNED:-}" ]] || die "CERTIFY_ALLOW_UNPINNED is refused with CERTIFY_PIN_BASE"
    [[ -n "${SOLIDITY_LEAN_DIR:-}" ]] || die "CERTIFY_PIN_BASE needs SOLIDITY_LEAN_DIR"
fi
if [[ -n "${SOLIDITY_LEAN_DIR:-}" ]]; then
    dir="$(realpath "$SOLIDITY_LEAN_DIR")"
    head="$(git -C "$dir" rev-parse HEAD)" || die "$dir is not a git checkout"
    if [[ "$head" != "$pin" && -n "${CERTIFY_PIN_BASE:-}" ]]; then
        read -r -a parents <<<"$(git -C "$dir" rev-list --parents -n 1 HEAD | cut -d' ' -f2-)"
        [[ ${#parents[@]} -eq 1 ]] ||
            die "solidity-lean HEAD $head has ${#parents[@]} parents (${parents[*]}): the inputs must be one commit on the pin $pin"
        [[ "${parents[0]}" == "$pin" ]] ||
            die "solidity-lean HEAD $head has parent ${parents[0]}, not the pin $pin: the inputs must be one commit on the pin"
        dirty="$(git -C "$dir" status --porcelain --untracked-files=all)" || die "cannot read the status of $dir"
        [[ -z "$dirty" ]] || die "the solidity-lean checkout $dir is not clean: $(head -5 <<<"$dirty" | tr '\n' ';')"
        check_inputs "$dir" "$pin" "$head" "$inputs_dir"
        tree="$(git -C "$dir" rev-parse "$head^{tree}")"
        echo "certify: solidity-lean $pin + inputs $head (tree $tree, $inputs_dir only)" >&2
    elif [[ "$head" != "$pin" ]]; then
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
