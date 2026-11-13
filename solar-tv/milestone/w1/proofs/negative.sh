#!/usr/bin/env bash
# Negative tests: meaning-changing edits of w1's artifacts make its proofs fail with a mismatch.
#
# Each case writes an edited copy of one exported artifact (the bytecode, or one EVM IR stage),
# compiles it into a separate olean directory that shadows the exported module, and elaborates
# the unchanged proof file that checks it against its neighbour. The edited artifact must
# compile (it is a well-formed program or byte list), and the proof must fail with its specific
# mismatch: the assembly walk evaluating to false, or sym_exec reaching a path where the two runs
# end differently. Control cases run the same harness on unedited copies, which must pass, so a
# failure is the edit's and not the harness's. Run from anywhere after
# `lake build W1Proofs` in solar-tv; LEAN_MEMORY caps each Lean process (MB, default 4500).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tv="$(cd "$here/../../.." && pwd)"
cd "$tv"
export LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-2}"
memory="${LEAN_MEMORY:-4500}"
lean_path="$(lake env printenv LEAN_PATH)"
gen=milestone/w1/gen/Main
proofs=milestone/w1/proofs/W1Proofs
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
failures=0

# edit NAME FILE: copy FILE to the case's source tree, edited by the Python on stdin, which gets
# the text as `s` and must set it.
edit() {
    local name=$1 file=$2
    mkdir -p "$tmp/$name/src/gen/Main" "$tmp/$name/olean/gen/Main"
    python3 -c '
import sys
s = open(sys.argv[1]).read()
exec(sys.stdin.read())
open(sys.argv[2], "w").write(s)
' "$file" "$tmp/$name/src/gen/Main/$(basename "$file")"
}

# check NAME MODULE PROOF EXPECT [PATTERN]: compile the case's MODULE (gen/Main/X) and
# elaborate PROOF against it; EXPECT is pass, or fail with PATTERN (an extended regex) in an
# error, matched against the output with its lines and spaces joined into single spaces.
check() {
    local name=$1 module=$2 proof=$3 expect=$4 pattern=${5:-}
    local dir="$tmp/$name" out="$tmp/$name/out"
    if ! LEAN_PATH="$lean_path" lean -M "$memory" -R "$dir/src" -o "$dir/olean/$module.olean" \
        "$dir/src/$module.lean" > "$out" 2>&1; then
        echo "FAIL $name: the edited $module does not compile:"; sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1)); return
    fi
    # Lean finds a package's modules in the first search path entry with its directory, so the
    # case's directory holds every other exported module too, as links to the built ones.
    local built
    for built in .lake/build/lib/lean/gen/Main/*.olean; do
        [[ -e "$dir/olean/gen/Main/$(basename "$built")" ]] || ln -s "$tv/$built" "$dir/olean/gen/Main/"
    done
    local status=0
    LEAN_PATH="$dir/olean:$lean_path" lean -M "$memory" "$proof" > "$out" 2>&1 || status=$?
    if [[ $expect == pass ]]; then
        if [[ $status -eq 0 ]]; then
            echo "ok   $name: $proof accepts the unedited $module"
        else
            echo "FAIL $name: $proof rejects the unedited $module:"; sed 's/^/  /' "$out" | head -20
            failures=$((failures + 1))
        fi
        return
    fi
    # An import, a name or memory going wrong is not the mismatch the case expects.
    if grep -Eq 'unknown (module|constant|identifier)|object file|out of memory|deterministic timeout' "$out"; then
        echo "FAIL $name: $proof failed for another reason:"; sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1))
    elif [[ $status -ne 0 ]] && tr -s '\n ' '  ' < "$out" | grep -Eq "$pattern"; then
        echo "ok   $name: $proof rejects it: $(tr -s '\n ' '  ' < "$out" | grep -Eo "$pattern" | head -1)"
    else
        echo "FAIL $name: $proof did not fail with /$pattern/ (exit $status):"
        sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1))
    fi
}

# The bytecode at one offset, from one byte to another, in both the byte list and the hex.
mutate_byte() {
    local offset=$1 from=$2 to=$3
    cat <<EOF
import re
offset, old, new = $offset, $from, $to
body = re.search(r"def bytecode : List Nat :=\n  \[(.*?)\]", s, re.S)
bytes_ = [int(b, 16) for b in re.findall(r"0x[0-9a-f]+", body.group(1))]
assert bytes_[offset] == old, (offset, bytes_[offset])
bytes_[offset] = new
rows = [", ".join("0x%02x" % b for b in bytes_[i:i + 16]) for i in range(0, len(bytes_), 16)]
s = s[:body.start(1)] + ",\n   ".join(rows) + s[body.end(1):]
hex_old = re.search(r'def bytecodeHex : String :=\n  "([0-9a-f]+)"', s).group(1)
assert hex_old[2 * offset:2 * offset + 2] == "%02x" % old
s = s.replace(hex_old, hex_old[:2 * offset] + "%02x" % new + hex_old[2 * offset + 2:])
EOF
}

# Replace the one occurrence of a text (Python literals).
replace_once() {
    cat <<EOF
old, new = $1, $2
assert s.count(old) == 1, s.count(old)
s = s.replace(old, new)
EOF
}

# decide +kernel evaluates the walk to false: it reports that the proposition is false, or,
# on longer bytecode, the kernel rejects its proof.
walk_false='(Tactic .decide. proved that the proposition walk bytecode offset 0 items = some stream is false|'
walk_false+='\(kernel\) application type mismatch id \(Eq.refl true\) argument has type true = true but function has type decide \(walk bytecode offset 0 items = some stream\) = true)'

# EVM IR to bytecode: the walk over the bytecode against evm4's items.
edit bytecode-control "$gen/Bytecode.lean" <<<'pass'
check bytecode-control gen/Main/Bytecode "$proofs/Assembly.lean" pass

# ADD (0x01) at offset 33, the sum, as SUB (0x03).
mutate_byte 33 0x01 0x03 | edit bytecode-add-sub "$gen/Bytecode.lean"
check bytecode-add-sub gen/Main/Bytecode "$proofs/Assembly.lean" fail "$walk_false"

# LT (0x10) at offset 36, the overflow check, as GT (0x11).
mutate_byte 36 0x10 0x11 | edit bytecode-lt-gt "$gen/Bytecode.lean"
check bytecode-lt-gt gen/Main/Bytecode "$proofs/Assembly.lean" fail "$walk_false"

mismatch='sym_exec: the runs end in different results on this path'

# A known-bad pass output: share-reverts (evm1 to evm2).
edit evm2-control "$gen/Evm2.lean" <<<'pass'
check evm2-control gen/Main/Evm2 "$proofs/Runs/Evm2.lean" pass

# The overflow branch jumps to the empty revert (block 4) instead of the panic (block 3).
replace_once '".jumpi 3,"' '".jumpi 4,"' | edit evm2-jump-target "$gen/Evm2.lean"
check evm2-jump-target gen/Main/Evm2 "$proofs/Runs/Evm2.lean" fail "$mismatch"

# The overflow check's operands swapped: lt(a, a + b) instead of lt(a + b, a).
replace_once '"(.dup 1)),\n     .op (.stack .lt),\n     .jumpi 3"' \
    '"(.dup 1)),\n     .op (.stack (.swap 0)),\n     .op (.stack .lt),\n     .jumpi 3"' |
    edit evm2-swapped-operands "$gen/Evm2.lean"
check evm2-swapped-operands gen/Main/Evm2 "$proofs/Runs/Evm2.lean" fail "$mismatch"

# A known-bad scheduling, the corruption -Zvalidate-backend-fault=opcode makes: the scheduled
# EVM IR's add as sub (stage0 to sched).
replace_once '".op (.stack .add)"' '".op (.stack .sub)"' | edit sched-add-sub "$gen/Sched.lean"
check sched-add-sub gen/Main/Sched "$proofs/Runs/Sched.lean" fail "$mismatch"

if [[ $failures -ne 0 ]]; then
    echo "w1 negative tests: $failures failed" >&2
    exit 1
fi
echo "w1 negative tests: all passed"
