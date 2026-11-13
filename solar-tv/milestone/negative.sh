#!/usr/bin/env bash
# Negative tests of a certified contract: meaning-changing edits of its artifacts make the
# generated proofs fail with a mismatch.
#
#   solar-tv/milestone/negative.sh NAME CONTRACT
#
# Run after certify.sh built NAME's proofs. Each case writes an edited copy of one exported
# artifact of milestone/NAME/gen/CONTRACT, compiles it into a separate olean directory that
# shadows the exported module, and elaborates the unchanged generated proof that checks it
# against its neighbour; the proof must fail with its mismatch (the assembly walk evaluating to
# false, or sym_exec reaching a path whose runs end differently). Controls run the same harness
# on unedited copies, which must pass. The edits are found in the artifacts, not written per
# contract:
#
# - bytecode: the first ADD opcode (else LT, else EQ) of the linear decoding as another opcode;
# - final EVM IR: its first add as sub, the key N its first sstore pushes as N + 1, and its call
#   value check dropped (the call goes on instead of reverting), its jump to the Panic(0x11) block
#   of the overflow check dropped, its first log dropped, the signature topic of its first log
#   plus one, the data offset of its first log at the next word, its first keccak256 (a mapping
#   slot) as an add, the key that hash reads plus one (every mapping key moved, zero or not), the
#   mapping slot it reads plus one, its first mul as add, and its first range test of a narrow
#   product (`push 2^k - 1; dup n; gt`) as `push 0`, its first external call as another kind
#   of call and the check of that call's success dropped (a failed call goes on), each when
#   present;
# - scheduled EVM IR: its first add as sub, when present;
# - each lowered MIR pass output that changes code: in the lines the pass wrote, its first
#   branch taken the other way, else its first jump in place of a branch of its input sent to
#   the branch's other target (a folded branch folded the other way), else its first calldata
#   read at the next word, else its first constant plus one; as a second case, its first
#   cast (a zero extension) of a value as the constant 1 (0 for a constant 1), else its first
#   select with its arms swapped; and, among the lines it wrote that its input does not have in
#   the same block (an instruction it added or moved), the first two-input phi it added with its
#   inputs swapped (in live code: when every such phi is on the failure path of a lowered fact,
#   which never runs, the first test or subtraction it added with its operands swapped), and the
#   first pure operation as another one (xor as and, add as sub, lt as gt, ...);
# - last MIR stage, against the scheduled EVM IR: the values of its first return of several
#   words in reverse order, and the first further result a call binds left unbound, when
#   present.
#
# LEAN_MEMORY caps each Lean process (MB, default 4500).
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: negative.sh NAME CONTRACT" >&2; exit 1; }
name=$1 contract=$2
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tv="$(cd "$here/.." && pwd)"
cd "$tv"
export LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-2}"
memory="${LEAN_MEMORY:-4500}"
lean_path="$(lake env printenv LEAN_PATH)"
mod="$name/gen/$contract"
gen="milestone/$mod"
proofs="milestone/$name/Proofs"
last="$(python3 -c 'import json, sys; s = [s for s in json.load(open(sys.argv[1]))["stages"] if s["kind"] != "bytecode"]; print(s[-1]["file"][:-5])' "$gen/manifest.json")"
last_mir="$(python3 -c 'import json, sys; s = [s for s in json.load(open(sys.argv[1]))["stages"] if s["kind"] == "mir"]; print(s[-1]["file"][:-5])' "$gen/manifest.json")"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
failures=0
cases=0

# edit CASE FILE: copy FILE into the case's source tree, edited by the Python on stdin, which
# gets the text as s and sets it; it exits 3 when the edit's pattern is absent.
edit() {
    local case=$1 file=$2
    mkdir -p "$tmp/$case/src/$mod" "$tmp/$case/olean/$mod"
    python3 -c '
import sys
s = open(sys.argv[1]).read()
exec(sys.stdin.read())
open(sys.argv[2], "w").write(s)
' "$file" "$tmp/$case/src/$mod/$(basename "$file")"
}

# elaborate PROOF: elaborate PROOF against the case's modules into $out, setting status (uses
# check's dir, out and expect).
elaborate() {
    status=0
    LEAN_PATH="$dir/olean:$lean_path" lean -M "$memory" "$1" > "$out" 2>&1 || status=$?
    # An edit can make a run longer or its terms deeper than the proof's heartbeats and recursion
    # depth allow (a check dropped, so the edited program runs on where the other stops; a read
    # at another calldata word, whose word the procedure carries along unsimplified): run the
    # proof again with four times its heartbeats and a recursion depth of 100000, which can only
    # make it more likely to pass, so that it fails by its mismatch.
    if [[ $expect != pass && $status -ne 0 ]] &&
        grep -Eq 'deterministic\) timeout|maximum recursion depth' "$out"; then
        python3 -c '
import re, sys
s = open(sys.argv[1]).read()
s = re.sub(r"maxHeartbeats (\d+)", lambda m: "maxHeartbeats %d" % (4 * int(m.group(1))), s)
if "maxRecDepth" in s:
    s = re.sub(r"maxRecDepth \d+", "maxRecDepth 100000", s)
else:
    s = re.sub(r"^(set_option maxHeartbeats \d+ in\n)", r"\1set_option maxRecDepth 100000 in\n", s, flags=re.M)
open(sys.argv[2], "w").write(s)
' "$1" "$dir/proof.lean"
        status=0
        LEAN_PATH="$dir/olean:$lean_path" lean -M "$memory" "$dir/proof.lean" > "$out" 2>&1 || status=$?
    fi
}

# check CASE MODULE PROOF EXPECT [PATTERN]: compile the case's MODULE (a file name under the
# exported directory) and elaborate PROOF against it; EXPECT is pass, or fail with PATTERN (an
# extended regex) in the output with its lines and spaces joined.
check() {
    local case=$1 module=$2 proof=$3 expect=$4 pattern=${5:-}
    local dir="$tmp/$case" out="$tmp/$case/out"
    cases=$((cases + 1))
    if ! LEAN_PATH="$lean_path" lean -M "$memory" -R "$dir/src" -o "$dir/olean/$mod/$module.olean" \
        "$dir/src/$mod/$module.lean" > "$out" 2>&1; then
        echo "FAIL $case: the edited $module does not compile:"; sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1)); return
    fi
    # Lean finds a package's modules in the first search path entry with its directory, so the
    # case's directory links every other exported module.
    local built
    for built in .lake/build/lib/lean/$mod/*.olean; do
        [[ -e "$dir/olean/$mod/$(basename "$built")" ]] || ln -s "$tv/$built" "$dir/olean/$mod/"
    done
    # A run that certify.sh --split proved by cases is the directory of its cases: unedited,
    # every case must pass; edited, the first case that fails decides.
    local proofs=("$proof") each
    [[ -d "${proof%.lean}" ]] && proofs=("${proof%.lean}"/*.lean)
    status=0
    for each in "${proofs[@]}"; do
        elaborate "$each"
        [[ $status -eq 0 ]] || break
    done
    if [[ $expect == pass ]]; then
        if [[ $status -eq 0 ]]; then
            echo "ok   $case: $proof accepts the unedited $module"
        else
            echo "FAIL $case: $proof rejects the unedited $module:"; sed 's/^/  /' "$out" | head -20
            failures=$((failures + 1))
        fi
        return
    fi
    if grep -Eq 'unknown (module|constant|identifier)|object file|out of memory|deterministic timeout' "$out"; then
        echo "FAIL $case: $proof failed for another reason:"; sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1))
    elif [[ $status -ne 0 ]] && tr -s '\n ' '  ' < "$out" | grep -Eq "$pattern"; then
        echo "ok   $case: $proof rejects it: $(tr -s '\n ' '  ' < "$out" | grep -Eo "$pattern" | head -1)"
    else
        echo "FAIL $case: $proof did not fail with /$pattern/ (exit $status):"
        sed 's/^/  /' "$out" | head -20
        failures=$((failures + 1))
    fi
}

# try CASE FILE MODULE PROOF PATTERN: the edit on stdin, then the check, skipping a case whose
# pattern the artifact does not have.
try() {
    local case=$1 file=$2 module=$3 proof=$4 pattern=$5 status=0
    edit "$case" "$file" || status=$?
    if [[ $status -eq 3 ]]; then echo "skip $case: no such code in $module"; return; fi
    [[ $status -eq 0 ]] || { echo "FAIL $case: the edit failed"; failures=$((failures + 1)); return; }
    check "$case" "$module" "$proof" fail "$pattern"
}

# Replace the first occurrence of a text (Python literals).
replace_first() {
    cat <<EOF
old, new = $1, $2
if old not in s: sys.exit(3)
s = s.replace(old, new, 1)
EOF
}

# decide +kernel evaluates the walk to false: it reports that the proposition is false, or,
# on longer bytecode, the kernel rejects its proof.
walk_false='(Tactic .decide. proved that the proposition walk bytecode offset 0 items = some stream is false|'
walk_false+='\(kernel\) application type mismatch id \(Eq.refl true\) argument has type true = true but function has type decide \(walk bytecode offset 0 items = some stream\) = true)'
mismatch='sym_exec: the runs end in different results on this path'

edit bytecode-control "$gen/Bytecode.lean" <<<'pass'
check bytecode-control Bytecode "$proofs/Assembly.lean" pass

# The first ADD of the linear decoding as SUB, else LT as GT, else EQ as SUB (not equal).
try bytecode-opcode "$gen/Bytecode.lean" Bytecode "$proofs/Assembly.lean" "$walk_false" <<'EOF'
import re
body = re.search(r"def bytecode : List Nat :=\n  \[(.*?)\]", s, re.S)
code = [int(b, 16) for b in re.findall(r"0x[0-9a-f]+", body.group(1))]
ops, pc = [], 0
while pc < len(code):
    ops.append(pc)
    pc += 1 + (code[pc] - 0x5f if 0x60 <= code[pc] <= 0x7f else 0)
offset = next((pc for want in (0x01, 0x10, 0x14) for pc in ops if code[pc] == want), None)
if offset is None: sys.exit(3)
old = code[offset]
new = {0x01: 0x03, 0x10: 0x11, 0x14: 0x03}[old]
print(f"  bytecode offset {offset}: 0x{old:02x} as 0x{new:02x}")
code[offset] = new
rows = [", ".join("0x%02x" % b for b in code[i:i + 16]) for i in range(0, len(code), 16)]
s = s[:body.start(1)] + ",\n   ".join(rows) + s[body.end(1):]
hex_old = re.search(r'def bytecodeHex : String :=\n  "([0-9a-f]+)"', s).group(1)
s = s.replace(hex_old, hex_old[:2 * offset] + "%02x" % new + hex_old[2 * offset + 2:])
EOF

edit "$last-control" "$gen/$last.lean" <<<'pass'
check "$last-control" "$last" "$proofs/Runs/$last.lean" pass

replace_first '".op (.stack .add)"' '".op (.stack .sub)"' > "$tmp/add-sub.py"
try "$last-add-sub" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/add-sub.py"

# The key pushed for the first sstore that pushes its key, N, as N + 1.
cat > "$tmp/sstore.py" <<'EOF'
import re
m = re.search(r"\(\.push (0x[0-9a-f]+|\d+)\)\),\n\s*\.effect \.sstore 2", s)
if m is None: sys.exit(3)
key = int(m.group(1), 0)
new = hex(key + 1) if m.group(1).startswith("0x") else str(key + 1)
print(f"  sstore key {m.group(1)} as {new}")
s = s[:m.start(1)] + new + s[m.end(1):]
EOF
try "$last-sstore-key" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/sstore.py"

# The dispatcher's call value check dropped: the value is popped instead of reverting on it.
python3 - "$gen/$last.lean" > "$tmp/callvalue.py" <<'EOF'
import re, sys
m = re.search(r"\.effect \.callvalue 0,\n     \.jumpi \d+,", open(sys.argv[1]).read())
print(f"old, new = {m.group(0)!r}, '.effect .callvalue 0,\\n     .op (.stack .pop),'" if m else "sys.exit(3)")
print("if old not in s: sys.exit(3)\ns = s.replace(old, new, 1)" if m else "")
EOF
try "$last-no-callvalue-check" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/callvalue.py"

# The overflow check dropped: the jump to the Panic(0x11) block (the block that stores 17 and
# the selector 0x4e487b71) pops its condition instead, so an overflowing addition goes on.
python3 - "$gen/$last.lean" > "$tmp/overflow.py" <<'EOF'
import re, sys
text = open(sys.argv[1]).read()
blocks = re.split(r"\n    -- bb\d+\n", text.split("blocks := [", 1)[1])[1:]
panic = next((i for i, b in enumerate(blocks) if "(.push 17)" in b and "(.push 0x4e487b71)" in b), None)
m = re.search(r"\.jumpi %d," % panic, text) if panic is not None else None
print(f"old, new = {m.group(0)!r}, '.op (.stack .pop),'" if m else "sys.exit(3)")
print("if old not in s: sys.exit(3)\ns = s.replace(old, new, 1)" if m else "")
EOF
try "$last-no-overflow-check" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/overflow.py"

# The first log dropped: its operands popped instead, so the run logs nothing there.
python3 - "$gen/$last.lean" > "$tmp/log.py" <<'EOF'
import re, sys
m = re.search(r"\.effect \(\.log (\d)\) (\d),", open(sys.argv[1]).read())
pops = ",\n     ".join([".op (.stack .pop)"] * int(m.group(2))) + "," if m else ""
print(f"old, new = {m.group(0)!r}, {pops!r}" if m else "sys.exit(3)")
print("if old not in s: sys.exit(3)\ns = s.replace(old, new, 1)" if m else "")
EOF
try "$last-no-log" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/log.py"

# The first hash (a mapping slot) as the sum of its offset and size, a slot that is no hash.
replace_first '".effect .keccak256 2,"' '".op (.stack .add),"' > "$tmp/keccak.py"
try "$last-no-keccak" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/keccak.py"

# The first log's first constant topic (a 32-byte push before it, the event's signature) plus one.
cat > "$tmp/log-topic.py" <<'EOF'
import re
log = re.search(r"\.effect \(\.log \d\) \d,", s)
if log is None: sys.exit(3)
m = None
for m in re.finditer(r"\(\.push (0x[0-9a-f]{64})\)\)", s[:log.start()]): pass
if m is None: sys.exit(3)
new = "0x%064x" % ((int(m.group(1), 16) + 1) % 2**256)
print(f"  log topic {m.group(1)} as {new}")
s = s[:m.start(1)] + new + s[m.end(1):]
EOF
try "$last-log-topic" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/log-topic.py"

# The first log's data at the next word: the offset it pushes last, N, as N + 32.
cat > "$tmp/log-offset.py" <<'EOF'
import re
m = re.search(r"\(\.push (0x[0-9a-f]+|\d+)\)\),\n\s*\.effect \(\.log \d\) \d,", s)
if m is None: sys.exit(3)
offset = int(m.group(1), 0)
new = str(offset + 32)
print(f"  log data offset {m.group(1)} as {new}")
s = s[:m.start(1)] + new + s[m.end(1):]
EOF
try "$last-log-offset" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/log-offset.py"

# The first mapping key moved by one: the word the first hash reads at offset 0 (the key, which
# the code stores with push 0; store before the hash) plus one, so every key, zero or not, reads
# and writes the slot of the next one.
cat > "$tmp/mapping-key.py" <<'EOF'
import re
k = s.find(".effect .keccak256 2,")
if k < 0: sys.exit(3)
m = None
for m in re.finditer(r"\n(\s*)\.op \(\.stack \(\.push 0\)\),\n\s*\.op \.store,", s[:k]): pass
if m is None: sys.exit(3)
ind = m.group(1)
print("  mapping key stored at 0 as the key plus one")
s = s[:m.start()] + f"\n{ind}.op (.stack (.push 1)),\n{ind}.op (.stack .add)," + s[m.start():]
EOF
try "$last-mapping-key" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/mapping-key.py"

# The first mapping's slot moved: the slot number the first hash reads at offset 32, N, as
# N + 1 (another mapping's slots).
cat > "$tmp/mapping-slot.py" <<'EOF'
import re
k = s.find(".effect .keccak256 2,")
if k < 0: sys.exit(3)
m = None
for m in re.finditer(r"\(\.push (0x[0-9a-f]+|\d+)\)\),\n\s*\.op \(\.stack \(\.push 32\)\),\n\s*\.op \.store,", s[:k]): pass
if m is None: sys.exit(3)
slot = int(m.group(1), 0)
print(f"  mapping slot {m.group(1)} as {slot + 1}")
s = s[:m.start(1)] + str(slot + 1) + s[m.end(1):]
EOF
try "$last-mapping-slot" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/mapping-slot.py"

# Each hash (at most the first four) reading the wrong range: its offset, and separately its
# length, one word more (`push 32; add` before it, under a `swap` for the length), so that it
# reads a range starting or ending at the next word. Each check holds for literal and computed
# operands alike: a hash of `abi.encode` output at the free memory pointer, or of a copy of
# calldata of a variable length.
for i in 1 2 3 4; do
    for which in offset length; do
        cat > "$tmp/hash-$which-$i.py" <<EOF
import re
ks = [m.start() for m in re.finditer(r"\n(\s*)\.effect \.keccak256 2,", s)]
if len(ks) < $i: sys.exit(3)
k = ks[$i - 1]
ind = re.match(r"\n(\s*)", s[k:]).group(1)
ops = ["(.push 32)", ".add"] if "$which" == "offset" else ["(.swap 0)", "(.push 32)", ".add", "(.swap 0)"]
print("  hash $i: its $which one word more")
s = s[:k] + "".join(f"\n{ind}.op (.stack {op})," for op in ops) + s[k:]
EOF
        try "$last-hash$i-$which" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/hash-$which-$i.py"
    done
done

# Each constant of more than 160 bits that the code stores in memory (at most the first two: an
# EIP-712 type hash that `abi.encode` writes before hashing) plus one.
for i in 1 2; do
    cat > "$tmp/typehash-$i.py" <<EOF
import re
ms = list(re.finditer(r"\(\.push (0x[0-9a-f]{41,64})\)\),(?=(?:\n\s*\.op \(\.stack \(\.[a-z]+ \d+\)\),){0,3}\n\s*\.op \.store,)", s))
if len(ms) < $i: sys.exit(3)
m = ms[$i - 1]
new = "0x%064x" % ((int(m.group(1), 16) + 1) % 2**256)
print(f"  stored constant {m.group(1)} as {new}")
s = s[:m.start(1)] + new + s[m.end(1):]
EOF
    try "$last-typehash$i" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/typehash-$i.py"
done

# The first multiplication as an addition.
replace_first '".op (.stack .mul)"' '".op (.stack .add)"' > "$tmp/mul-add.py"
try "$last-mul-add" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/mul-add.py"

# The first range test of a narrow product dropped: `push 2^k - 1; dup n; gt` (the product
# exceeds the type's maximum, the condition of the jump to Panic(0x11)) as `push 0`, a test that
# never holds, so an overflowing narrow multiplication stores its truncated product.
cat > "$tmp/narrow.py" <<'EOF'
import re
masks = "|".join(str(2 ** k - 1) for k in range(8, 256, 8)) + "|" + "|".join(hex(2 ** k - 1) for k in range(8, 256, 8))
m = re.search(r"\.op \(\.stack \(\.push (%s)\)\),\n\s*\.op \(\.stack \(\.dup \d+\)\),\n\s*\.op \(\.stack \.gt\)," % masks, s)
if m is None: sys.exit(3)
print(f"  range test of {m.group(1)} dropped")
s = s[:m.start()] + ".op (.stack (.push 0))," + s[m.end():]
EOF
try "$last-no-narrow-check" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/narrow.py"

# The first external call as another kind with the same operands: a static call as a delegate
# call, which may change storage, and a delegate call as a static call, which may not.
python3 - "$gen/$last.lean" > "$tmp/call-kind.py" <<'EOF'
import re, sys
m = re.search(r"\.effect \(\.call \.(\w+)\) (\d)", open(sys.argv[1]).read())
other = {"staticcall": "delegatecall", "delegatecall": "staticcall"}.get(m.group(1)) if m else None
print(f"old, new = {m.group(0)!r}, '.effect (.call .{other}) {m.group(2)}'" if other else "sys.exit(3)")
print("if old not in s: sys.exit(3)\ns = s.replace(old, new, 1)" if other else "")
EOF
try "$last-call-kind" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/call-kind.py"

# The check of the first external call's success dropped: the jump on its failure pops its
# condition instead, so a failed call goes on as a successful one.
python3 - "$gen/$last.lean" > "$tmp/call-check.py" <<'EOF'
import re, sys
m = re.search(r"\.effect \(\.call \.\w+\) \d,\n     \.op \(\.stack \.iszero\),\n     (\.jumpi \d+,)",
              open(sys.argv[1]).read())
new = m.group(0)[: m.start(1) - m.start(0)] + ".op (.stack .pop)," if m else ""
print(f"old, new = {m.group(0)!r}, {new!r}" if m else "sys.exit(3)")
print("if old not in s: sys.exit(3)\ns = s.replace(old, new, 1)" if m else "")
EOF
try "$last-no-call-check" "$gen/$last.lean" "$last" "$proofs/Runs/$last.lean" "$mismatch" < "$tmp/call-check.py"

try sched-add-sub "$gen/Sched.lean" Sched "$proofs/Runs/Sched.lean" "$mismatch" < "$tmp/add-sub.py"

# Each lowered MIR pass that changes code, against its input: the first branch it wrote taken
# the other way, else the first jump it wrote in place of a branch sent to the branch's other
# target, else the calldata it reads first at the next word, else its first constant plus one,
# each in the lines the pass wrote.
mapfile -t passes < <(python3 -c '
import json, sys
s = [s for s in json.load(open(sys.argv[1]))["stages"] if s["kind"] == "mir"]
for a, b in zip(s, s[1:]): print(a["file"][:-5], b["file"][:-5])
' "$gen/manifest.json")
for pair in "${passes[@]}"; do
    read -r input output <<<"$pair"
    edit "$output-control" "$gen/$output.lean" <<<'pass'
    check "$output-control" "$output" "$proofs/Runs/$output.lean" pass
    for kind in wrote cast phi op; do
        python3 - "$gen/$input.lean" "$kind" > "$tmp/$output-$kind.py" <<'EOF'
import sys
print(f"input = open({sys.argv[1]!r}).read()")
print(f"kind = {sys.argv[2]!r}")
print(r'''
import difflib, re
before, after = input.splitlines(keepends=True), s.splitlines(keepends=True)
ops = difflib.SequenceMatcher(None, before, after, autojunk=False).get_opcodes()
wrote = [j for tag, _, _, j1, j2 in ops if tag in ("replace", "insert") for j in range(j1, j2)
         if after[j].startswith("  ") and not after[j].lstrip().startswith(("--", "/-", "def "))]
replaced = {j: before[i1:i2] for tag, i1, i2, j1, j2 in ops if tag == "replace" for j in range(j1, j2)}
def branch(j, m): return f".branch {m.group(1)} {m.group(3)} {m.group(2)}" if m.group(2) != m.group(3) else None
def folded(j, m):
    for line in replaced.get(j, []):
        for b in re.finditer(r"\.branch \([^()]*\) (\d+) (\d+)", line):
            yes, no = b.group(1), b.group(2)
            if yes != no and m.group(1) in (yes, no): return f".jump {no if m.group(1) == yes else yes}"
def read(j, m): return f".calldataload [.const {int(m.group(1), 0) + 32}]"
def const(j, m): return f".const {int(m.group(1), 0) + 1}"
rules = [(r"\.branch (\([^()]*\)) (\d+) (\d+)", branch),
         (r"\.jump (\d+)", folded),
         (r"\.calldataload \[\.const (0x[0-9a-f]+|\d+)\]", read),
         (r"\.const (0x[0-9a-f]+|\d+)", const)]
def cast(j, m): return f".word {m.group(1)} (.const {0 if m.group(2) == '.const 1' else 1})"
def select(j, m):
    return f".select {m.group(1)} {m.group(2)} {m.group(4)} {m.group(3)}" if m.group(3) != m.group(4) else None
if kind == "cast":
    rules = [(r"\.word (\d+) \(([^()]*)\)", cast),
             (r"\.select (\d+) (\([^()]*\)) (\([^()]*\)) (\([^()]*\))", select)]
def keyed(lines):
    """Each line with its function and block, so that a line moved to another block is new."""
    fn = bb = None
    for line in lines:
        t = line.strip()
        if t.startswith("-- @"): fn, bb = t, None
        elif re.fullmatch(r"-- bb\d+", t): bb = t
        yield (fn, bb, t.lstrip("[⟨").rstrip(",]⟩"))
moved = set(keyed(before))
fresh = [j for j, k in enumerate(keyed(after)) if j in set(wrote) and k not in moved]
def phi(j, m):
    arms = re.findall(r"\((\d+), (\.(?:value|const) (?:0x[0-9a-f]+|\d+))\)", m.group(2))
    if m.group(0) in input or len(arms) != 2 or arms[0][1] == arms[1][1]: return None
    return f"({m.group(1)}, [({arms[0][0]}, {arms[1][1]}), ({arms[1][0]}, {arms[0][1]})])"
others = {"xor": "and", "and": "or", "or": "xor", "add": "sub", "sub": "add", "mul": "add",
          "lt": "gt", "gt": "lt", "slt": "sgt", "sgt": "slt", "shl": "shr", "shr": "shl"}
def op(j, m): return f".pure {m.group(1)} .{others[m.group(2)]} [" if m.group(2) in others else None
def dead():
    """The lines of blocks that only the failure edges of lowered facts reach. A fact
    (`assume`) holds on every run the stage proof covers, so a pass that checks it with a
    branch to a revert writes code that never runs, and an edit there is no mutant."""
    blocks, fn, bb = {}, None, None
    for j, line in enumerate(after):
        t = line.strip()
        if t.startswith("-- @"): fn, bb = t, None
        elif re.fullmatch(r"-- bb\d+", t): bb = t; blocks[(fn, bb)] = [j, j]
        elif bb is not None: blocks[(fn, bb)][1] = j
    edges = {}
    for (f, b), (j1, j2) in blocks.items():
        text = "".join(after[j1:j2 + 1])
        facts = {m.group(1) for j in range(j1, j2 + 1)
                 if any(".assume" in line for line in replaced.get(j, []))
                 for m in re.finditer(r"\.(?:pure|ne) (\d+) ", after[j])}
        targets = re.findall(r"\.jump (\d+)", text)
        for m in re.finditer(r"\.branch \(\.(\w+) ([^()]*)\) (\d+) (\d+)", text):
            targets += [m.group(4)] if m.group(1) == "value" and m.group(2) in facts else [m.group(3), m.group(4)]
        for m in re.finditer(r"\.switch \([^()]*\) (\d+) \[([^\[\]]*)\]", text):
            targets += [m.group(1)] + re.findall(r", (\d+)\)", m.group(2))
        edges[(f, b)] = [(f, "-- bb" + t) for t in targets]
    seen, todo = set(), [k for k in blocks if k[1] == "-- bb0"]
    while todo:
        k = todo.pop()
        if k in seen or k not in blocks: continue
        seen.add(k)
        todo += edges[k]
    return {j for k, (j1, j2) in blocks.items() if k not in seen for j in range(j1, j2 + 1)}
def swap(j, m):
    return f".pure {m.group(1)} .{m.group(2)} [{m.group(4)}, {m.group(3)}]" if m.group(3) != m.group(4) else None
if kind == "phi":
    # A phi of live code with its inputs swapped; when every phi the pass added is dead code,
    # the first test or subtraction it wrote in live code with its operands swapped instead.
    live = set(fresh) - dead()
    pattern = r"\((\d+), \[(\(\d+, [^()]*\), \(\d+, [^()]*\))\]\)"
    has = lambda lines: any(phi(j, m) for j in lines for m in re.finditer(pattern, after[j]))
    wrote, rules = [j for j in fresh if j in live], [(pattern, phi)]
    if has(fresh) and not has(live):
        rules.append((r"\.pure (\d+) \.(lt|gt|slt|sgt|sub|div|sdiv|mod|smod|shl|shr|sar) "
                      r"\[(\.\w+ [^\[\],]*), (\.\w+ [^\[\],]*)\]", swap))
if kind == "op":
    wrote, rules = fresh, [(r"\.pure (\d+) \.(\w+) \[", op)]
def first():
    for pattern, new in rules:
        for j in wrote:
            for m in re.finditer(pattern, after[j]):
                text = new(j, m)
                if text is not None: return j, m, text
hit = first()
if hit is None: sys.exit(3)
j, m, text = hit
print(f"  line {j + 1}: {m.group(0)} as {text}")
after[j] = after[j][:m.start()] + text + after[j][m.end():]
s = "".join(after)
''')
EOF
        label=$output-wrote
        [[ $kind == wrote ]] || label=$output-wrote-$kind
        try "$label" "$gen/$output.lean" "$output" "$proofs/Runs/$output.lean" "$mismatch" \
            < "$tmp/$output-$kind.py"
    done
done

# Only a program that returns several words from an internal call has these cases.
if grep -q '\.leave \[[^]]*,' "$gen/$last_mir.lean"; then
edit "$last_mir-control" "$gen/$last_mir.lean" <<<'pass'
check "$last_mir-control" "$last_mir" "$proofs/Runs/Sched.lean" pass

# The first return of several words with its values in reverse order: the caller binds each
# further result to another word.
try "$last_mir-swap-results" "$gen/$last_mir.lean" "$last_mir" "$proofs/Runs/Sched.lean" "$mismatch" <<'EOF'
import re
m = next((m for m in re.finditer(r"\.leave \[([^\[\]]*)\]", s) if "," in m.group(1)), None)
if not m: sys.exit(3)
values = [v.strip() for v in m.group(1).split(",")]
s = s[:m.start(1)] + ", ".join(reversed(values)) + s[m.end(1):]
EOF

# The first further result a call binds left unbound: the value that reads it keeps its old
# word.
try "$last_mir-unbound-result" "$gen/$last_mir.lean" "$last_mir" "$proofs/Runs/Sched.lean" "$mismatch" <<'EOF'
import re
m = re.search(r"(\.call \S+(?: \S+)? \d+ \[[^\[\]]*\] \[[^\[\]]*)some \d+", s)
if not m: sys.exit(3)
s = s[:m.end(1)] + "none" + s[m.end():]
EOF
fi

if [[ $failures -ne 0 ]]; then
    echo "$name negative tests: $failures of $cases failed" >&2
    exit 1
fi
echo "$name negative tests: all $cases passed"
