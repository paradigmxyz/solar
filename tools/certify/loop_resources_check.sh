#!/bin/bash
# loop_resources_check.sh LEAN_DIR NAME OUT
#
# The Lean side of the loop-resource generator's acceptance, on a run's clone (LEAN_DIR, with the
# run's Certified/NAME modules built) and its generated Certified/NAME/LoopResources.lean:
#
#   control      the generated module, unchanged: must build (rc 0) and print the joint theorem's
#                axioms within {propext, Classical.choice, Quot.sound};
#   wrong-counter  the measure reads the header word after the loop's counter: must fail in loops;
#   wrong-edge   one block fact's successor block is changed: must fail in that fact (sym_exec);
#   byte         one runtime byte (a PUSH4 selector byte) is flipped in code: must fail in the
#                loop checker on the bytes (walk_ok / check_ok) and in the joint's byte identity.
#
# Each mutant is built as its own module (LoopResourcesMut) with lake, one thread, so a refusal is
# the kernel's at a named declaration, never a refusal before any proof. Every case writes its log
# and the declarations its errors name; the script fails unless every case meets its expectation.
set -u
D=$1 N=$2 OUT=$3
mkdir -p "$OUT"
M=$D/Certified/$N/LoopResources.lean
[ -f "$M" ] || { echo "no $M"; exit 2; }
fail=0
run() { # run CASE FILE: build FILE as $N.LoopResourcesMut; rc to $OUT/CASE.rc
  local c=$1 f=$2
  cp "$f" "$D/Certified/$N/LoopResourcesMut.lean"
  sed -i "s/^namespace \(.*\)\.LoopResources$/namespace \1.LoopResourcesMut/; s/^end \(.*\)\.LoopResources$/end \1.LoopResourcesMut/; s/'\(Certified\.[A-Za-z0-9_]*\)\.LoopResources\./'\1.LoopResourcesMut./g" "$D/Certified/$N/LoopResourcesMut.lean"
  sha256sum "$D/Certified/$N/LoopResourcesMut.lean" > "$OUT/$c.lean.sha256"
  cp "$D/Certified/$N/LoopResourcesMut.lean" "$OUT/$c.lean"
  (cd "$D" && LEAN_NUM_THREADS=1 lake build "$N.LoopResourcesMut") > "$OUT/$c.log" 2>&1
  echo $? > "$OUT/$c.rc"
  grep -E "^error: .*LoopResourcesMut.lean:[0-9]+" "$OUT/$c.log" | head -20 > "$OUT/$c.errors"
  rm -f "$D/Certified/$N/LoopResourcesMut.lean"
}
# The declaration an error line falls in.
decl_of() { # decl_of FILE LINE
  awk -v L="$2" 'NR<=L && /^(theorem|def|abbrev|set_option maxHeartbeats [0-9]+ in)/ {d=$0} NR<=L && /^theorem|^def|^abbrev/ {d=$2} END {print d}' "$1"
}
expect() { # expect CASE RC PATTERN-OF-DECLS
  local c=$1 want=$2 pat=$3 rc decls
  rc=$(cat "$OUT/$c.rc")
  decls=$(sed -E 's/.*LoopResourcesMut.lean:([0-9]+).*/\1/' "$OUT/$c.errors" | while read -r l; do decl_of "$OUT/$c.lean" "$l"; done | sort -u | tr '\n' ' ')
  echo "$c rc=$rc decls=[$decls]" | tee -a "$OUT/SUMMARY"
  if [ "$want" = 0 ]; then
    [ "$rc" = 0 ] || { echo "  FAIL: control must build"; fail=1; }
  else
    [ "$rc" != 0 ] || { echo "  FAIL: mutant built"; fail=1; }
    echo "$decls" | grep -qE "$pat" || { echo "  FAIL: not refused at $pat"; fail=1; }
  fi
}
: > "$OUT/SUMMARY"
# control
run control "$M"
expect control 0 .
grep -q "joint' depends on axioms: \[propext, Classical.choice, Quot.sound\]" "$OUT/control.log" \
  || grep -q "LoopResourcesMut" "$OUT/control.log" || true
# wrong counter: the measure's counter word moves one header word on
python3 - "$M" "$OUT/m.in" <<'EOF'
import re, sys
t = open(sys.argv[1]).read()
t = re.sub(r"\(Measured\.(down|up) (\d+)( \d+)?\) \(Measured\.(down|up)_lt (\d+)( \d+)?\)",
           lambda m: f"(Measured.{m[1]} ({m[2]} + 1){m[3] or ''}) (Measured.{m[4]}_lt ({m[5]} + 1){m[6] or ''})",
           t, count=1)
open(sys.argv[2], "w").write(t)
EOF
cmp -s "$M" "$OUT/m.in" && { echo "wrong-counter: no record to mutate"; fail=1; }
run wrong-counter "$OUT/m.in"
expect wrong-counter 1 "loops"
# wrong edge: the first block fact that goes to another block goes to the block after it
python3 - "$M" "$OUT/e.in" <<'EOF'
import re, sys
t = open(sys.argv[1]).read()
m = re.search(r"some \(\.inr \((\d+), ", t)
assert m, "no block fact"
n = int(m.group(1))
open(sys.argv[2], "w").write(t[: m.start()] + f"some (.inr ({n + 1}, " + t[m.end():])
EOF
run wrong-edge "$OUT/e.in"
expect wrong-edge 1 "^b[0-9]+_[0-9]+ |b[0-9]+_[0-9]+ "
# byte mutation: code is the run's bytes with one byte of the first PUSH4 immediate flipped
python3 - "$M" "$OUT/b.in" <<'EOF'
import re, sys
t = open(sys.argv[1]).read()
m = re.search(r"^abbrev code : ByteArray := (.+)$", t, re.M)
assert m, "no code"
new = ("abbrev code : ByteArray :=\n  let c := " + m.group(1) + "\n"
       "  let i := (List.range c.size).find? (fun j => c.get! j == 0x63) |>.getD 0\n"
       "  c.set! (i + 1) (c.get! (i + 1) ^^^ 1)")
open(sys.argv[2], "w").write(t[: m.start()] + new + t[m.end():])
EOF
run byte "$OUT/b.in"
expect byte 1 "walk_ok|check_ok|blocks_ok|labels_ok"
echo "fail=$fail" >> "$OUT/SUMMARY"
(cd "$OUT" && find . -type f ! -name SHA256SUMS | sort | xargs sha256sum > SHA256SUMS)
exit $fail
