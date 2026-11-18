#!/usr/bin/env bash
# verify_token through the generic certify driver on a memory-limited box.
#
#   vt_box.sh MEMORY_MAX_BYTES [OUT_DIR]
#
# Pins: solar 085fbaa853a32f1a9a4cab77cb8d892fd2eb6e2f (driver and producer), solidity-lean
# 334d1d924f8f6c29ac827fd81d1cc739721b479a (the driver's solidity-lean.rev), solc 0.8.35,
# source Certified/verify_token/verify_token.sol (sha256 273bfc34...), contract Token, SPEC natspec.
#
# The outer call re-runs this script inside a transient systemd scope with MemoryMax set to
# MEMORY_MAX_BYTES and MemorySwapMax=0. Inside the scope it reads memory.max back (a finite byte
# count, never "max") with tools/certify/box_admit.sh admit, and reads memory.peak and
# memory.events before it exits. The whole run is under a 2 h wall gate: each step is cut off at
# the deadline, and a step still running or not started at 1 h 50 min is recorded as OPEN.
# CERTIFY_ALLOW_* are unset throughout. CARGO_NET_OFFLINE is on after cargo fetch.
#
# Steps, each recorded in OUT_DIR/steps with its exit status:
#   producer  cargo build --locked --bin solar at the pin (the driver's own build_solar command),
#             with the binary's sha256, path, embedded commit (equal to the pin), flags and the
#             Cargo.lock sha256
#   0         the source envelope (natspec) and the bytecode envelope on the producer's runtime
#   1         generic.py SOURCE natspec --contract Token --base <pin>; the caller route verbatim.
#             BLOCKED (msg.sender) on mint is a terminal result: the source is never edited
#   2         generic.py --replay (the driver's fresh cold run of the record): trusted 0, axioms
#   3         negatives, each of which must be refused (a refusal is a PASS):
#             wrong-spec (transfer's self-transfer ensures flipped, a full run that must not
#             reach CERTIFIED), swapped-bytecode (another contract's runtime through final.py and
#             the kernel) and hsep-slot (the generated MapSlotsSeparated hypothesis moved to
#             another slot, through the kernel)
# Always reported: hsep (MapSlotsSeparated) is an OPEN assumption; deployment is excluded.
set -uo pipefail

SOLAR_PIN=085fbaa853a32f1a9a4cab77cb8d892fd2eb6e2f
SL_PIN=334d1d924f8f6c29ac827fd81d1cc739721b479a
SOLC_VERSION=0.8.35
SOLC_SHA256=fa8ac9a32d301ad023a36ee5a29f8e291fe3200c60244e43c142539e82a617f4
SRC_SHA256=273bfc34729a2a7e3301fc37589096b7efb80480b17636e8ba09b295e537b4a6
NAME=verify_token_273bfc34
WALL_S=7200 OPEN_S=6600

for v in $(compgen -e | grep '^CERTIFY_ALLOW_' || true); do unset "$v"; done

if [[ "${1:-}" != --inner ]]; then
    [[ "${1:-}" =~ ^[0-9]+$ ]] || { echo "usage: vt_box.sh MEMORY_MAX_BYTES [OUT_DIR]" >&2; exit 2; }
    mem=$1 out=$(realpath -m "${2:-$HOME/vt-box}")
    [[ -e "$out" ]] && { echo "vt_box: $out exists: give a fresh OUT_DIR" >&2; exit 2; }
    mkdir -p "$out"
    unit="vt-box-$(date -u +%Y%m%dT%H%M%SZ)"
    exec sudo -n systemd-run --scope --quiet --unit "$unit" -p MemoryMax="$mem" -p MemorySwapMax=0 \
        --uid="$(id -u)" --gid="$(id -g)" \
        /usr/bin/env -i HOME="$HOME" USER="$USER" PATH="$PATH" LANG=C.UTF-8 \
        bash "$(realpath "$0")" --inner "$mem" "$out" "$(date +%s)"
fi

mem=$2 out=$3 t0=$4
w=$out/work steps=$out/steps
mkdir -p "$w"
exec > >(tee -a "$out/vt_box.log") 2>&1
log() { echo "$* $(date -u +%FT%TZ)" | tee -a "$steps"; }
elapsed() { echo $(( $(date +%s) - t0 )); }
left() { echo $(( WALL_S - $(elapsed) )); }

# run_step LABEL CMD...: CMD under the remaining wall time; OPEN if not started by, or still
# running at, 1 h 50 min. Returns the exit status: 124 is a cut-off, 125 not started.
run_step() {
    local label=$1; shift
    if (( $(elapsed) >= OPEN_S )); then log "OPEN $label: not started (past 1h50)"; return 125; fi
    local rc
    timeout --kill-after=30 "$(left)" "$@"; rc=$?
    if (( $(elapsed) >= OPEN_S )); then log "OPEN $label: exit=$rc at $(elapsed)s (past 1h50)"
    else log "step $label exit=$rc elapsed_s=$(elapsed)"; fi
    return $rc
}

finish() {
    # Read the scope's peak and events before the scope exits (this shell is its last process).
    local cg; cg=$(sed -n 's/^0:://p' /proc/self/cgroup)
    {
        echo "cgroup $cg"
        echo "memory.max $(cat "/sys/fs/cgroup$cg/memory.max" 2>&1)"
        echo "memory.peak $(cat "/sys/fs/cgroup$cg/memory.peak" 2>&1)"
        echo "memory.swap.max $(cat "/sys/fs/cgroup$cg/memory.swap.max" 2>&1)"
        echo "== memory.events"; cat "/sys/fs/cgroup$cg/memory.events" 2>&1
    } > "$out/memory.txt"
    log "memory $(tr '\n' ' ' < "$out/memory.txt")"
    log "ASSUMPTION OPEN: hsep (SolidCore.Spec.MapSlotsSeparated on the balanceOf slot, keccak256 separation) is a hypothesis of the statement, not discharged"
    log "EXCLUDED: deployment (the init code; the statement is about the runtime from storage holding the constructor's slots)"
    log "TERMINAL $1 elapsed_s=$(elapsed)"
    (cd "$out" && find . -type f ! -path './work/*' ! -name pack.sha256 -print0 | sort -z | xargs -0 sha256sum > pack.sha256)
    exit "${2:-0}"
}

log "start t0=$t0 memory_max_want=$mem uid=$(id -u) solar=$SOLAR_PIN solidity-lean=$SL_PIN"
cg=$(sed -n 's/^0:://p' /proc/self/cgroup)

# Sources.
git clone -q https://github.com/paradigmxyz/solar.git "$w/solar" && git -C "$w/solar" checkout -q "$SOLAR_PIN" ||
    finish "FAILED: cannot check out solar $SOLAR_PIN" 1
git clone -q https://github.com/paradigmxyz/solidity-lean.git "$w/sl" &&
    git -C "$w/sl" fetch -q origin "$SL_PIN" && git -C "$w/sl" checkout -q "$SL_PIN" ||
    finish "FAILED: cannot check out solidity-lean $SL_PIN" 1
C=$w/solar/tools/certify
[[ $(cat "$C/solidity-lean.rev") == "$SL_PIN" ]] || finish "FAILED: solidity-lean.rev is not $SL_PIN" 1

# Admission: memory.max must read back as the finite byte count asked for, swap 0, this shell in it.
bash "$C/box_admit.sh" admit "$cg" $$ "$mem" > "$out/admit.txt" 2>&1 || { cat "$out/admit.txt"; finish "FAILED: admission" 1; }
log "admitted cgroup=$cg memory.max=$(cat "/sys/fs/cgroup$cg/memory.max") swap.max=$(cat "/sys/fs/cgroup$cg/memory.swap.max")"

# Tools: solc 0.8.35 by digest, Python 3.14 (generic.py needs it) first on PATH, rustup per the
# repository's toolchain file, elan for the base's lean-toolchain.
mkdir -p "$HOME/.cache/solc" "$w/bin"
SOLC=$HOME/.cache/solc/solc-$SOLC_VERSION
[[ -x $SOLC ]] || curl -fsSL -o "$SOLC" "https://github.com/argotorg/solidity/releases/download/v$SOLC_VERSION/solc-static-linux"
[[ $(sha256sum < "$SOLC" | cut -c1-64) == "$SOLC_SHA256" ]] || finish "FAILED: solc digest" 1
chmod +x "$SOLC"; export SOLC
export PATH=$HOME/.local/bin:$PATH
command -v uv > /dev/null || curl -LsSf https://astral.sh/uv/install.sh | sh > /dev/null 2>&1
uv python install -q 3.14 || finish "FAILED: python 3.14" 1
ln -sf "$(uv python find 3.14)" "$w/bin/python3"; export PATH=$w/bin:$HOME/.cargo/bin:$HOME/.elan/bin:$PATH
command -v cargo > /dev/null || curl -sSfL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain none --no-modify-path > /dev/null
command -v lake > /dev/null || curl -sSfL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh | sh -s -- -y --default-toolchain none --no-modify-path > /dev/null
export LEAN_MEMORY=${LEAN_MEMORY:-6000}
{ python3 -VV; cargo -V; (cd "$w/solar" && rustc -vV); } > "$out/tools.txt" 2>&1

# Producer: the driver's build (cargo build --locked --bin solar, debug, at the pin), fetched once
# and then built offline.
cd "$w/solar" || finish "FAILED: no solar checkout" 1
run_step fetch cargo fetch --locked || finish "FAILED: cargo fetch" 1
export CARGO_NET_OFFLINE=true
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR
run_step producer env CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" cargo build --locked --bin solar > "$out/producer-build.log" 2>&1 ||
    finish "FAILED: producer build" 1
B=$w/solar/target/debug/solar
[[ -x $B ]] || finish "FAILED: producer binary missing at $B" 1
ver=$("$B" --version 2>&1)
commit=$(sed -n 's/^Commit SHA: \([0-9a-f]\{12,40\}\)$/\1/p' <<< "$ver")
[[ ${#commit} -ge 12 && $SOLAR_PIN == "$commit"* ]] || finish "FAILED: producer embeds commit '$commit', not $SOLAR_PIN" 1
cat > "$out/producer.json" <<EOF
{"path": "$B", "sha256": "$(sha256sum < "$B" | cut -c1-64)", "embedded_commit": "$commit",
 "driver_commit": "$(git rev-parse HEAD)", "dirty": $(git status --porcelain | wc -l),
 "command": "cargo build --locked --bin solar", "profile": "dev", "RUSTFLAGS": "unset",
 "CARGO_BUILD_JOBS": "${CARGO_BUILD_JOBS:-4}", "CARGO_NET_OFFLINE": "true",
 "cargo_lock_sha256": "$(sha256sum < Cargo.lock | cut -c1-64)", "rustc": "$(rustc -V)"}
EOF
log "producer $(tr -d '\n' < "$out/producer.json")"
CERTIFY_SOLAR_SHA256=$(sha256sum < "$B" | cut -c1-64); export CERTIFY_SOLAR_SHA256

SRC=$w/sl/Certified/verify_token/verify_token.sol
[[ $(sha256sum < "$SRC" | cut -c1-64) == "$SRC_SHA256" ]] || finish "FAILED: source digest" 1

# Step 0: the source envelope (natspec) and the bytecode envelope, with the driver's own
# functions (envelope, solar_runtime, scan_bytecode).
cat > "$w/env0.py" <<'EOF'
import sys, pathlib
sys.path.insert(0, sys.argv[1])
import generic as g
src = pathlib.Path(sys.argv[2])
ast, contracts = g.solc_ast(sys.argv[3], src)
try:
    c, fns, ev = g.envelope(src.read_text(), ast, contracts, "Token", None, True, False)
except g.Blocked as b:
    print("BLOCKED source envelope:", b); sys.exit(3)
print("source envelope: inside", c, [f["sig"] for f in fns])
plain, err = g.solar_runtime(pathlib.Path(sys.argv[4]), src, "Token")
if plain is None:
    print("solar does not compile the source:", err[:400]); sys.exit(1)
bad = g.scan_bytecode(plain)
print("bytecode envelope:", "runtime bytes", len(plain) // 2, "refused opcodes", bad or "none")
sys.exit(3 if bad else 0)
EOF
run_step 0-envelope python3 -I "$w/env0.py" "$C" "$SRC" "$SOLC" "$B" > "$out/step0.txt" 2>&1
rc0=$?; cat "$out/step0.txt"
(( rc0 == 0 )) || finish "BLOCKED at step 0 (exit $rc0; see step0.txt)" 3

# Step 1: the driver.
cd "$w/solar" || exit 1
run_step 1-generic python3 tools/certify/generic.py "$SRC" natspec --contract Token \
    --solidity-lean "$w/sl" --base "$SL_PIN" --work "$w/run1" --record-dir "$out/record" > "$out/step1.txt" 2>&1
rc1=$?
grep -E 'caller route' "$out/step1.txt" > "$out/route.txt"
sed 's/^/route: /' "$out/route.txt" | tee -a "$steps"
tail -3 "$out/step1.txt" | sed 's/^/verdict: /' | tee -a "$steps"
if grep -q 'caller route: BLOCKED (msg.sender)' "$out/route.txt"; then
    finish "BLOCKED (msg.sender) at step 1, exit $rc1: terminal; the source is not edited" 3
fi
(( rc1 == 0 )) || finish "step 1 exit $rc1 (see step1.txt)" "$rc1"

# Step 2: the driver's fresh cold replay of the record.
run_step 2-replay python3 tools/certify/generic.py --replay "$out/record" --solidity-lean "$w/sl" \
    --work "$w/run2" > "$out/step2.txt" 2>&1
rc2=$?; tail -2 "$out/step2.txt" | sed 's/^/replay: /' | tee -a "$steps"
python3 - "$out/record/record.json" > "$out/replay-summary.txt" <<'EOF'
import json, sys
r = json.load(open(sys.argv[1])).get("replay") or {}
print("trusted", r.get("trusted"), "unaccounted", r.get("unaccounted"), "axioms", r.get("axioms"))
sys.exit(0 if r.get("trusted") == 0 and not r.get("unaccounted") else 1)
EOF
rc2b=$?; log "replay-summary $(cat "$out/replay-summary.txt") ok=$(( rc2b == 0 ))"

# Step 3: negatives. A refusal is a PASS; an acceptance, skip or cut-off is not.
L=$w/run1/solidity-lean
shape=$L/Certified/$NAME/shape.json
[[ -f $shape ]] || finish "FAILED: $shape missing after step 1" 1
neg_fail=0
# wrong-spec: transfer's self-transfer ensures flipped; a full run must not reach CERTIFIED.
mkdir -p "$w/neg-spec"; WS=$w/neg-spec/verify_token.sol
sed 's|msg.sender == to ==> balanceOf\[to\] == old(balanceOf\[to\])$|msg.sender == to ==> balanceOf[to] == old(balanceOf[to]) + amount|' "$SRC" > "$WS"
if cmp -s "$SRC" "$WS"; then log "NEG wrong-spec: SKIP (clause not found) = FAIL"; neg_fail=1
else
    run_step 3-neg-wrong-spec python3 tools/certify/generic.py "$WS" natspec --contract Token \
        --solidity-lean "$w/sl" --base "$SL_PIN" --work "$w/run-neg-spec" --record-dir "$out/neg-spec-record" \
        --no-negatives --packages-from "$L" > "$out/neg-spec.txt" 2>&1
    rc=$?; last=$(tail -1 "$out/neg-spec.txt")
    if (( rc == 124 || rc == 125 )); then log "NEG wrong-spec: OPEN (exit $rc)"; neg_fail=1
    elif (( rc == 0 )); then log "NEG wrong-spec: ACCEPTED = FAIL (${last:0:200})"; neg_fail=1
    else log "NEG wrong-spec: refused = PASS (exit $rc: ${last:0:200})"; fi
fi
# swapped-bytecode and hsep-slot through final_neg.sh's own variant machinery (its preamble and
# variant(), unchanged, then these cases).
n=$(grep -n '^flip=' "$C/final_neg.sh" | cut -d: -f1)
[[ -n $n ]] || finish "FAILED: final_neg.sh has no flip= line to cut at" 1
head -n "$(( n - 1 ))" "$C/final_neg.sh" > "$w/neg_variants.sh"
cat >> "$w/neg_variants.sh" <<'EOF'
other="$(ls "$lean_dir/$examples"/*/bytecode.hex | grep -v "/$name/bytecode.hex" | head -n 1)"
[[ -n "$other" ]] || die "no other contract's bytecode to swap in"
variant swapped-bytecode "$(tr -d '[:space:]' < "$other")" "pass"
# hsep-slot: each generated module that states MapSlotsSeparated, with its first slot moved by
# one, compiled into an overlay; the unchanged Final over it must be refused.
d="$work/hsep-slot" olean="$work/hsep-slot/olean"; mkdir -p "$olean"
cp -rs "$lean_dir/.lake/build/lib/lean/$name" "$olean/$name"
mapfile -t hs < <(grep -rl 'MapSlotsSeparated \[[0-9]' "$lean_dir/$examples/$name" --include='*.lean')
[[ ${#hs[@]} -gt 0 ]] || { echo "hsep-slot: SKIP (no MapSlotsSeparated in the generated modules) = FAIL" >&2; exit 1; }
for f in "${hs[@]}"; do
    rel=${f#"$lean_dir/$examples/"}; mod=${rel%.lean}; mod=${mod//\//.}
    o="$d/${mod//./_}.lean"
    sed -E 's/MapSlotsSeparated \[([0-9]+)/MapSlotsSeparated [(\1 + 1)/g' "$f" > "$o"
    if ! compile "$olean" "$mod" "$o" > "$o.out" 2>&1; then
        echo "hsep-slot: refused by the kernel in $mod ($(grep -m1 error "$o.out" | clip))"; exit $(( fails > 0 ))
    fi
done
if ! compile "$olean" "$name.Final" "$lean_dir/$examples/$name/Final.lean" > "$d/final.out" 2>&1; then
    echo "hsep-slot: refused by the kernel, Final ($(grep -m1 error "$d/final.out" | clip))"
else
    echo "hsep-slot: ACCEPTED (it must not be)" >&2; fails=$((fails + 1))
fi
exit $(( fails > 0 ))
EOF
HEX=$(tr -d '[:space:]' < "$L/Certified/$NAME/bytecode.hex")
run_step 3-neg-final env CERTIFY_LEAN_MEMORY="$LEAN_MEMORY" bash "$w/neg_variants.sh" "$L" Certified "$NAME" Token "$HEX" > "$out/neg-final.txt" 2>&1
rc=$?; sed 's/^/NEG /' "$out/neg-final.txt" | tee -a "$steps"
(( rc == 0 )) || neg_fail=1
for c in swapped-bytecode hsep-slot; do grep -q "^$c: refused" "$out/neg-final.txt" || neg_fail=1; done
log "negatives $([[ $neg_fail == 0 ]] && echo 'all refused = PASS' || echo 'FAIL or OPEN')"

route=$(sed -n 's/.*caller route: //p' "$out/route.txt" | head -1)
finish "step1 exit=$rc1 route=[$route] replay exit=$rc2 trusted-ok=$(( rc2b == 0 )) negatives=$([[ $neg_fail == 0 ]] && echo PASS || echo FAIL)" \
    $(( rc1 || rc2 || rc2b || neg_fail ))
