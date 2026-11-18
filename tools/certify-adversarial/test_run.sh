#!/usr/bin/env bash
# Usage: bash test_ns.sh test_run.sh "$PWD" (needs a finite memory.max: test_ns.sh stands one in).
# Run-level stub: a fake driver (certify) and a fake cargo whose solar embeds the checkout's
# commit. FAIL_CASE names the source file whose certify run prints a failing control verdict.
set -u
H=$1
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
S=$T/solar; mkdir -p "$S/tools/certify" "$T/bin"
cat > "$S/tools/certify/certify" <<'EOF'
#!/usr/bin/env bash
if [[ $1 == --replay ]]; then echo "replay args: $*"; exit 0; fi
src=$1; rd=
while (($#)); do [[ $1 == --record-dir ]] && rd=$2; shift; done
mkdir -p "$rd"
sha=$(sha256sum "$HOME/solar-bin" | cut -c1-64)
case "$(basename "$src")" in
  FlagControl.sol|SiblingOk.sol|FlagRenamed.sol)
    if [[ "$(basename "$src")" == "${FAIL_CASE:-none}" ]]; then
        printf '{"solar": {"binary_sha256": "%s", "binary_commit": "%s"}, "strict": {"tier": "certified", "holds": false}, "tier_reached": "functional"}\n' "$sha" "$BC" > "$rd/record.json"
        echo "REJECTED: strict tier: reached functional"; exit 2
    fi
    printf '{"solar": {"binary_sha256": "%s", "binary_commit": "%s"}, "strict": {"tier": "certified", "holds": true}, "tier_reached": "certified", "replay": {"closure": 3, "replayed_count": 3, "trusted_count": 0, "unaccounted": []}}\n' "$sha" "$BC" > "$rd/record.json"
    echo "CERTIFIED: x: the requested tier holds (record r)"; exit 0 ;;
  *) sleep "${NEG_SLEEP:-1}"
     printf '{"solar": {"binary_sha256": "%s", "binary_commit": "%s"}}\n' "$sha" "$BC" > "$rd/record.json"
     echo "REJECTED: spec: raise_meets0 does not check: x"; exit 1 ;;
esac
EOF
chmod +x "$S/tools/certify/certify"
printf 'target/\n' > "$S/.gitignore"
cp "$H/../certify/generic.py" "$S/tools/certify/generic.py" 2>/dev/null || touch "$S/tools/certify/generic.py"
git -C "$S" init -q; git -C "$S" add -A; git -C "$S" -c user.email=t@t -c user.name=t commit -qm init
export BC=$(git -C "$S" rev-parse HEAD)
cat > "$T/bin/cargo" <<EOF
#!/usr/bin/env bash
mkdir -p target/debug
printf '#!/usr/bin/env bash\necho "solar 0.0"; echo "Commit SHA: %s"\n' "\${FAKE_EMBED:-$BC}" > target/debug/solar
chmod +x target/debug/solar; cp target/debug/solar "\$HOME/solar-bin"
EOF
cat > "$T/bin/rustc" <<'EOF'
#!/usr/bin/env bash
echo rustc-fake
EOF
chmod +x "$T/bin/"*
export PATH="$T/bin:$PATH" HOME=$T
fails=0
ok() { echo "ok $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
go() { # go OUTNAME [env...]
  env SLOTS=3 "${@:2}" OUT=$T/$1 SOLAR_DIR=$S SOLAR_SHA=$BC LEAN_DIR=$T LEAN_SHA=lean PACKAGES_FROM=$T/pk \
      CASES=$H/cases.tsv BOX_CREATED=$(date -u +%FT%TZ) EST_MIN=1 bash "$H/run.sh" > "$T/$1.out" 2>&1; }
go a
grep -q "ORDER control-flag control-siblingok control-flagrenamed siblingleak" "$T/a/run.log" && ok "controls admitted first" || bad "order: $(grep ORDER "$T/a/run.log")"
grep -q -- "--packages-from $T/pk" "$T/a/control-flag/replay.command" && grep -q -- "--solidity-lean $T" "$T/a/control-flag/replay.command" && ok "replay gets --solidity-lean and --packages-from" || bad "replay command: $(cat "$T/a/control-flag/replay.command")"
grep -q '^Control gate: PASS' "$T/a/table.md" && ok "all controls pass: gate PASS" || bad "gate a: $(grep 'Control gate' "$T/a/table.md")"
grep -q 'Semantic rejections counted: 2 of 13: flag-branchleak, flag-specwrong' "$T/a/table.md" && ok "only the 2 cases registered at raise_meets0 count" || bad "count a: $(grep 'Semantic' "$T/a/table.md")"
[[ ! -e "$T/a/STOP" ]] && ok "no stop when controls pass" || bad "stop a"
[[ -s "$T/a/run-scope.txt" ]] && ok "run scope recorded" || bad "run-scope"
go b SLOTS=5 FAIL_CASE=SiblingOk.sol NEG_SLEEP=20
[[ -e "$T/b/STOP" ]] && grep -q 'control control-siblingok' "$T/b/STOP" && ok "failing control writes STOP" || bad "STOP b: $(cat "$T/b/STOP" 2>&1)"
n=$(grep -l '^NOT-RUN' "$T"/b/*/status | wc -l); k=$(grep -l '^KILLED-STOP' "$T"/b/*/status | wc -l)
(( n > 0 )) && ok "later cases NOT-RUN after a control failure ($n)" || bad "NOT-RUN b: $n"
(( k > 0 )) && ok "running negatives end KILLED-STOP, no verdict ($k)" || bad "KILLED-STOP b: $k"
grep -q '^Control gate: FAIL (.*control-siblingok no' "$T/b/table.md" && grep -q 'Semantic rejections counted: 0 of' "$T/b/table.md" && ok "gate FAIL and nothing counted" || bad "gate b: $(grep -A1 'Control gate' "$T/b/table.md")"
go c FAKE_EMBED=0000000000000000000000000000000000000000
grep -q 'RUN-ABORT producer failed' "$T/c/run.log" && grep -q 'embeds commit 0000' "$T/c/producer.log" && ok "producer refuses a binary embedding another commit" || bad "producer c: $(tail -2 "$T/c/run.log")"
echo "fails=$fails"; exit $fails
