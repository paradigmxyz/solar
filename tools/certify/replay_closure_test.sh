#!/usr/bin/env bash
# Tests of the closure-scoped kernel replay (tools/certify/replay_closure.lean) and its accounting
# (tools/certify/replay.py closure) on small compiled fixtures: a published theorem whose proof
# reaches Init's lemmas through two modules. Needs the toolchain's lean on PATH. Each negative
# must fail closed: a deep library tamper, a sorry or an extra axiom deep in the closure, an omitted
# dependency, an omitted, overlapping or incomplete shard, a test-only defect in a receipt, and a
# theorem whose type is not the statement.
#   bash tools/certify/replay_closure_test.sh
set -euo pipefail
export LC_ALL=C
root="$(cd "$(dirname "$0")/../.." && pwd)"
tool="$root/tools/certify/replay_closure.lean"
acct="$root/tools/certify/replay.py"
lean_bin="$(command -v lean)" || { echo "no lean on PATH" >&2; exit 1; }
toolchain="$(cd "$(dirname "$(realpath "$lean_bin")")/.." && pwd)"
tmp="$(mktemp -d)"
trap '[[ -n "${KEEP:-}" ]] || rm -rf "$tmp"' EXIT
fails=0
ok() { echo "ok: $*"; }
bad() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }

# fixture DIR DEEP_BODY: Deep -> Mid -> Final, with Statement beside them.
fixture() {
    local d="$tmp/$1"
    mkdir -p "$d"
    printf '%s\n' "$2" > "$d/Deep.lean"
    printf 'import Deep\ntheorem midLemma (a b : Nat) : (a + 0) + b = a + b := by rw [deepLemma]\n' > "$d/Mid.lean"
    printf 'def Statement : Prop := (3 + 0) + 4 = 3 + 4\ndef Other : Prop := 3 + 4 = 4 + 3\n' > "$d/Statement.lean"
    printf 'import Mid\nimport Statement\ntheorem final : Statement := midLemma 3 4\n' > "$d/Final.lean"
    for m in Deep Mid Statement Final; do
        (cd "$d" && LEAN_PATH="$d" lean -o "$m.olean" "$m.lean" > /dev/null 2>&1) || { echo "cannot build fixture $1 ($m)" >&2; exit 1; }
    done
}
fixture good 'theorem deepLemma (n : Nat) : n + 0 = n := Nat.add_zero n'
fixture sorry 'theorem deepLemma (n : Nat) : n + 0 = n := by sorry'
fixture axiom $'axiom deepFact (n : Nat) : n + 0 = n\ntheorem deepLemma (n : Nat) : n + 0 = n := deepFact n'

# run FIXTURE PREFIX N [TEST FLAGS...]: every shard of N in parallel, each leaving its record.
run() {
    local fx="$1" prefix="$2" n="$3"
    shift 3
    local k pids=()
    for ((k = 0; k < n; k++)); do
        (
            st=0
            LEAN_PATH="$tmp/$fx" lean --run "$tool" "$@" Final final Statement "$prefix.$k-of-$n" $k $n \
                > "$prefix.$k-of-$n.out" 2>&1 || st=$?
            echo "$st" > "$prefix.$k-of-$n.status"
        ) &
        pids+=($!)
    done
    for p in "${pids[@]}"; do wait "$p"; done
}
account() { python3 "$acct" closure "$1" "$2" "$1.receipt" --theorem final --statement Statement \
    --toolchain "$toolchain" --checker "$lean_bin" --tool "$tool"; }

# Positive: one shard and three shards account for the same closure.
run good "$tmp/g1" 1
if account "$tmp/g1" 1 > "$tmp/out" 2>&1; then ok "the good closure replays in one shard: $(tail -1 "$tmp/out")"; else cat "$tmp/out" >&2; bad "the good closure is refused"; fi
run good "$tmp/g3" 3
if account "$tmp/g3" 3 > "$tmp/out" 2>&1; then ok "the good closure replays in three shards"; else cat "$tmp/out" >&2; bad "the good closure is refused in three shards"; fi
cmp -s "$tmp/g1.0-of-1.plan" "$tmp/g3.0-of-3.plan" || bad "one shard and three shards plan different closures"
python3 - "$tmp/g1.receipt" "$tmp/g3.receipt" <<'EOF' || bad "the receipts disagree or miss a field"
import json, sys
a, b = (json.load(open(p)) for p in sys.argv[1:])
assert a["closure_sha256"] == b["closure_sha256"] and a["closure"] == b["closure"] > 100
names = {c for c, _ in a["constants"]}
assert {"final", "midLemma", "deepLemma", "Nat.add_zero", "Statement", "Nat", "Eq"} <= names
assert set(a["axioms"]) <= {"propext", "Classical.choice", "Quot.sound"}
assert a["sorry_present"] is False and sum(s["proofs_checked"] for s in b["shards"]) == a["theorem_proofs"]
EOF

# expect_refused NAME PREFIX N: the accounting must fail.
expect_refused() {
    if account "$2" "$3" > "$tmp/out" 2>&1; then bad "$1 is accepted"; else ok "$1 is refused: $(grep -m1 error "$tmp/out" | cut -c1-160)"; fi
}
run sorry "$tmp/s" 1; expect_refused "a sorry deep in the closure" "$tmp/s" 1
grep -q 'sorryAx' "$tmp/s.0-of-1.out" || bad "the sorry is not named"
run axiom "$tmp/a" 1; expect_refused "an axiom deep in the closure" "$tmp/a" 1
grep -q 'deepFact' "$tmp/a.0-of-1.out" || bad "the axiom is not named"
run good "$tmp/t" 1 --tamper Nat.add_zero Nat.le_refl; expect_refused "a tampered Init lemma (Nat.add_zero)" "$tmp/t" 1
grep -q "declaration type mismatch, 'Nat.add_zero'" "$tmp/t.0-of-1.out" || bad "the kernel does not reject the tampered Nat.add_zero"
run good "$tmp/t3" 3 --tamper Nat.add_zero Nat.le_refl; expect_refused "a tampered Init lemma in three shards" "$tmp/t3" 3
[[ "$(cat "$tmp"/t3.*-of-3.status | sort | tr '\n' ' ')" == "0 0 1 " ]] || bad "the tamper is not caught by exactly its own shard"
run good "$tmp/o" 1 --omit Nat.add_zero; expect_refused "an omitted dependency (Nat.add_zero)" "$tmp/o" 1
grep -q 'the kernel rejects the closure' "$tmp/o.0-of-1.out" || bad "the kernel does not reject the closure without Nat.add_zero"
run good "$tmp/same" 1 --tamper deepLemma deepLemma; expect_refused "a receipt from a test-only run" "$tmp/same" 1
grep -q '^REPLAYED' "$tmp/same.0-of-1.out" || bad "the harmless test-only run should replay"
if LEAN_PATH="$tmp/good" lean --run "$tool" Final final Other "$tmp/ty" > "$tmp/out" 2>&1 ||
    ! grep -q 'the type of final is not exactly Other' "$tmp/out"; then bad "a theorem whose type is not the statement is accepted"
else ok "an expected-type mismatch is refused: $(head -1 "$tmp/out" | cut -c1-120)"; fi
# Shard accounting: an omitted shard, a wrong count, an overlap, a hole and a different closure.
mk() { for f in "$tmp"/g3.*-of-3.*; do cp "$f" "$tmp/$1${f#"$tmp"/g3}"; done; }
mk m; rm "$tmp"/m.2-of-3.*; expect_refused "an omitted shard" "$tmp/m" 3
mk w; expect_refused "three shards accounted as two" "$tmp/w" 2
mk v; head -1 "$tmp/v.0-of-3.proofs" >> "$tmp/v.1-of-3.proofs"; sort -o "$tmp/v.1-of-3.proofs" "$tmp/v.1-of-3.proofs"
n1="$(grep -c . "$tmp/v.1-of-3.proofs")"
sed -i -E "s/^(REPLAYED [0-9]+ constants, )[0-9]+/\1$n1/" "$tmp/v.1-of-3.out"; expect_refused "overlapping shards" "$tmp/v" 3
mk h; sed -i 1d "$tmp/h.0-of-3.proofs"
n0="$(grep -c . "$tmp/h.0-of-3.proofs")"
sed -i -E "s/^(REPLAYED [0-9]+ constants, )[0-9]+/\1$n0/" "$tmp/h.0-of-3.out"; expect_refused "shards that leave a proof unchecked" "$tmp/h" 3
mk p; echo "Extra.c Extra.c def" >> "$tmp/p.1-of-3.plan"; expect_refused "shards with different closures" "$tmp/p" 3
if [[ $fails -ne 0 ]]; then echo "$fails failure(s)" >&2; exit 1; fi
echo "all closure replay tests pass"
