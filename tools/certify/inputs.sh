#!/usr/bin/env bash
# Helpers scripts/certify sources (and tools/certify/test.sh tests directly): reading what a
# producer prints without losing its exit status, and the checks of the run's inputs. Each fails
# the run through the caller's die with an error that names what is wrong.
#
#   source tools/certify/inputs.sh

declare -F die > /dev/null || die() { echo "certify: error: $*" >&2; exit 1; }

# capture ARRAY WHAT CMD...: runs CMD (a command or a function) with its output to a file, as a
# job of its own that is waited for, so that set -e holds inside it and its exit status is its own
# (mapfile < <(CMD) and a pipe into while read both discard it), fails the run with WHAT unless it
# exits 0, and only then reads its lines into ARRAY.
capture() {
    local -n capture_into=$1
    local what=$2 out status=0
    shift 2
    out="$(mktemp "${TMPDIR:-/tmp}/certify-capture.XXXXXX")" || die "cannot make a temporary file for $what"
    "$@" > "$out" &
    wait "$!" || status=$?
    if [[ $status -ne 0 ]]; then
        rm -f "$out"
        die "$what fails (exit $status)"
    fi
    # capture_into is a nameref: this fills the caller's array.
    # shellcheck disable=SC2034
    mapfile -t capture_into < "$out"
    rm -f "$out"
}

# capture_nonempty ARRAY WHAT CMD...: capture, and fail the run when CMD prints nothing.
capture_nonempty() {
    capture "$@"
    local -n capture_seen=$1
    [[ ${#capture_seen[@]} -gt 0 ]] || die "$2 prints nothing"
}

# entries FILE...: the lines of the FILEs that are neither blank nor comments (# ...); none is an
# empty list, an unreadable file fails.
entries() {
    local status=0
    [[ $# -gt 0 ]] || { echo "certify: error: entries: no file" >&2; return 2; }
    grep -hvE '^\s*(#|$)' "$@" || status=$?
    [[ $status -le 1 ]] || return "$status"
}

# tree_diff VAR WHAT CMD...: runs tools/certify/clean_tree.py (CMD), which exits 0 with nothing
# printed for a clean tree and 1 with one line per difference; any other outcome (another status,
# or 1 with nothing printed, as an uncaught exception gives) fails the run, never reads as clean.
tree_diff() {
    local -n tree_into=$1
    local what=$2 out status=0
    shift 2
    out="$(mktemp "${TMPDIR:-/tmp}/certify-tree.XXXXXX")" || die "cannot make a temporary file for $what"
    "$@" > "$out" &
    wait "$!" || status=$?
    tree_into="$(cat "$out")"
    rm -f "$out"
    case "$status" in
        0) [[ -z "$tree_into" ]] || die "$what exits 0 but lists differences" ;;
        1) [[ -n "$tree_into" ]] || die "$what fails (exit 1 with no difference listed)" ;;
        *) die "$what fails (exit $status)" ;;
    esac
}

# check_options WHAT OPTION...: the solar options of a run: at least one, each one word of visible
# characters (no empty line, space, tab or carriage return), none of those the driver sets itself
# (what solar emits and where), and no word that is not a flag or the value of the flag before it
# (such as a second source file).
check_options() {
    local what=$1 o prev=
    shift
    [[ $# -gt 0 ]] || die "$what: no solar options (the defaults are -Ogas --evm-version osaka)"
    for o in "$@"; do
        [[ "$o" =~ ^[[:graph:]]+$ ]] || die "$what: '$(printf '%q' "$o")' is not one word of visible characters"
        case "$o" in
            --emit|--emit=*|-Zemit-lean-stages|-Zemit-lean-stages=*|-o|--out-dir|--out-dir=*|-|--stdin)
                die "$what: $o is set by the driver, not an option of the run" ;;
        esac
        if [[ "$o" != -* ]]; then
            [[ -n "$prev" && "$prev" != *=* ]] || die "$what: $o is neither a flag nor the value of one"
            [[ "$o" != *.sol && "$o" != *.yul && "$o" != *.json && "$o" != */* ]] ||
                die "$what: $o names a file, not an option value"
            prev=
        else
            prev=$o
        fi
    done
}

# check_number WHAT VALUE: a decimal number.
check_number() {
    [[ "$2" =~ ^[0-9]+$ ]] || die "$1 is not a number: '$(printf '%q' "$2")'"
}

# check_hex WHAT VALUE: bytecode as solar prints it: a non-empty, even-length run of lowercase hex
# digits.
check_hex() {
    [[ -n "$2" ]] || die "$1 is empty"
    [[ "$2" =~ ^([0-9a-f]{2})+$ ]] || die "$1 is not hex bytes: '$(head -c 80 <<<"$2")'"
}

# check_final_record FILE: the record tools/certify/final.py writes of the published statement: a
# JSON object with the theorem, the statement's kind and text, each status as text and strongest
# as a boolean. A missing, empty, truncated or incomplete record fails the run; the statuses are
# never taken from anything else.
check_final_record() {
    python3 - "$1" <<'PYEOF' || die "the record of the published statement $1 is missing or malformed (above)"
import json, sys
path = sys.argv[1]
try:
    text = open(path).read()
except OSError as e:
    sys.exit(f"certify: error: no record: {e}")
if not text.strip():
    sys.exit(f"certify: error: {path} is empty")
try:
    d = json.loads(text)
except ValueError as e:
    sys.exit(f"certify: error: {path} is not JSON: {e}")
if not isinstance(d, dict):
    sys.exit(f"certify: error: {path} is not a JSON object")
need = {"module": str, "statement_module": str, "theorem": str, "kind": str, "statement": str,
        "statuses": dict, "strongest": bool, "excluded": list}
bad = [k for k, t in need.items() if not isinstance(d.get(k), t) or (t is str and not d[k].strip())]
if bad:
    sys.exit(f"certify: error: {path} lacks {', '.join(bad)}")
for k in ("equivalence", "termination", "spec", "resources"):
    if not isinstance(d["statuses"].get(k), str) or not d["statuses"][k]:
        sys.exit(f"certify: error: {path} has no status {k}")
if "assumptions" not in d:
    sys.exit(f"certify: error: {path} lacks assumptions")
PYEOF
}

# check_run_record FILE: the run's record: a JSON object whose statuses say strongest as a boolean.
check_run_record() {
    python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))
assert isinstance(d, dict) and isinstance(d.get("statuses"), dict), "no statuses"
assert isinstance(d["statuses"].get("strongest"), bool), "strongest is not a boolean"
' "$1" || die "the run's record $1 is missing or malformed"
}
