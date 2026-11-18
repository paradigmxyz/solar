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
        printf '{"solar": {"binary_sha256": "%s", "binary_commit": "%s", "build": {"provenance_version": 2}}, "strict": {"tier": "certified", "holds": false}, "tier_reached": "functional"}\n' "$sha" "$BC" > "$rd/record.json"
        echo "REJECTED: strict tier: reached functional"; exit 2
    fi
    printf '{"solar": {"binary_sha256": "%s", "binary_commit": "%s", "build": {"provenance_version": 2}}, "strict": {"tier": "certified", "holds": true}, "tier_reached": "certified", "replay": {"closure": 3, "replayed_count": 3, "trusted_count": 0, "unaccounted": []}}\n' "$sha" "$BC" > "$rd/record.json"
    echo "CERTIFIED: x: the requested tier holds (record r)"; exit 0 ;;
  *) sleep "${NEG_SLEEP:-1}"
     printf '{"solar": {"binary_sha256": "%s", "binary_commit": "%s", "build": {"provenance_version": 2}}}\n' "$sha" "$BC" > "$rd/record.json"
     echo "REJECTED: spec: raise_meets0 does not check: x"; exit 1 ;;
esac
EOF
chmod +x "$S/tools/certify/certify"
printf 'target/\n' > "$S/.gitignore"
echo lock > "$S/Cargo.lock"
echo 'PROVENANCE_VERSION = 2' > "$S/tools/certify/generic.py"
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
  rm -rf "$S/target"
  env SLOTS=3 CASES=$H/cases.tsv "${@:2}" OUT=$T/$1 SOLAR_DIR=$S SOLAR_SHA=$BC LEAN_DIR=$T LEAN_SHA=lean PACKAGES_FROM=$T/pk \
      BOX_CREATED=$(date -u +%FT%TZ) EST_MIN=1 bash "$H/run.sh" > "$T/$1.out" 2>&1; }
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
grep -P '^control-' "$H/cases.tsv" > "$T/c1.tsv"
go d PHASE=c1 CASES=$T/c1.tsv
[[ $(cat "$T/d/control-flag/replay.exit") == skipped-c1 && ! -e "$T/d/control-flag/replay.command" ]] && ok "PHASE=c1: certify only, no replay" || bad "c1 replay: $(cat "$T/d/control-flag/replay.exit" 2>&1)"
[[ $(ls "$T"/d/outbox/*.tgz | wc -l) == 3 && ! -d "$T/d/siblingleak" ]] && ok "PHASE=c1: 3 control archives shipped, no negatives" || bad "c1 outbox: $(ls "$T"/d/outbox)"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert len(d["cargo_lock_sha256"])==64 and len(d["toolchain_dir_sha256"])==64' "$T/d/producer.json" && ok "producer records Cargo.lock and toolchain dir sha256" || bad "producer fields: $(cat "$T/d/producer.json")"
pr() { rm -rf "$S/target"; env "$@" SOLAR_DIR=$S SOLAR_SHA=$BC bash "$H/producer.sh" "$T/p.json" 2>&1; }
pr RUSTFLAGS=-Copt-level=3 | grep -q 'RUSTFLAGS is set; refusing' && ok "producer refuses RUSTFLAGS" || bad "RUSTFLAGS"
pr CARGO_BUILD_TARGET=x86_64-unknown-linux-musl | grep -q 'CARGO_BUILD_TARGET is set; refusing' && ok "producer refuses CARGO_BUILD_TARGET" || bad "CARGO_BUILD_TARGET"
pr RUSTC_WRAPPER=sccache | grep -q 'RUSTC_WRAPPER is set; refusing' && ok "producer refuses RUSTC_WRAPPER" || bad "RUSTC_WRAPPER"
pr CARGO_PROFILE_DEV_OPT_LEVEL=3 | grep -q 'CARGO_PROFILE_DEV_OPT_LEVEL is set; refusing' && ok "producer refuses a profile override" || bad "profile"
mkdir -p "$T/.cargo"; printf '[patch.crates-io]\n' > "$T/.cargo/config.toml"
pr | grep -q "external Cargo config $T/.cargo/config.toml" && ok "producer refuses a parent Cargo config" || bad "parent config"
rm -rf "$T/.cargo"; mkdir -p "$T/ch"; printf '[env]\n' > "$T/ch/config.toml"
pr CARGO_HOME=$T/ch | grep -q "external Cargo config $T/ch/config.toml" && ok "producer refuses a CARGO_HOME config" || bad "cargo_home config"
rm -rf "$T/ch"
mkdir -p "$S/target"; env SOLAR_DIR=$S SOLAR_SHA=$BC bash "$H/producer.sh" "$T/p.json" 2>&1 | grep -q 'target exists; the build must start empty' && ok "producer refuses a non-empty target" || bad "target"
rm -rf "$S/target"
# c3: negatives only, then the controls joined from stand-in c1 and c2 archives.
grep -vP '^control-' "$H/cases.tsv" > "$T/c3.tsv"
J=$T/join; mkdir -p "$J/c1" "$J/c2"
for n in control-flag control-siblingok control-flagrenamed; do
  mkdir -p "$J/s1/$n/rec" "$J/s2/$n/rec"
  printf '{"solar": {"build": {"provenance_version": 2}}, "strict": {"tier": "certified", "holds": true}, "tier_reached": "certified", "replay": {"closure": 3, "replayed_count": 3, "trusted_count": 0, "unaccounted": []}}
' > "$J/s1/$n/rec/record.json"
  cp "$J/s1/$n/rec/record.json" "$J/s2/$n/rec/record.json"
  printf 'START x
DONE exit=0 expect=0 match=yes class=control stage_ok=no replay=skipped-c1 producer=ok
' > "$J/s1/$n/status"
  printf '== x
CERTIFIED: %s: the requested tier holds (record r)
' "$n" > "$J/s1/$n/certify.log"
  echo '{"result": "ok"}' > "$J/s1/$n/producer-check.json"
  printf 'START x
DONE exit=0 replay=PASS closure=3
' > "$J/s2/$n/status"
  tar -C "$J/s1" -czf "$J/c1/$n.tgz" "$n"; tar -C "$J/s2" -czf "$J/c2/$n.tgz" "$n"
done
(cd "$J/c1" && sha256sum *.tgz) > "$J/c1.sha256"; (cd "$J/c2" && sha256sum *.tgz) > "$J/c2.sha256"
go g CASES=$T/c3.tsv SLOTS=5 JOIN_C1_DIR=$J/c1 JOIN_C1_SHAS=$J/c1.sha256 JOIN_C2_DIR=$J/c2 JOIN_C2_SHAS=$J/c2.sha256
grep -q 'JOIN exit=0 JOIN PASS' "$T/g/run.log" && ok "c3: controls joined from c1 and c2" || bad "c3 join: $(grep JOIN "$T/g/run.log"; cat "$T/g/join.log" 2>/dev/null)"
grep -q '^Control gate: PASS' "$T/g/table.md" && grep -q 'Semantic rejections counted: 2 of 13' "$T/g/table.md" && ok "c3: gate PASS over joined controls, 2 counted" || bad "c3 table: $(grep -A1 'Control gate' "$T/g/table.md")"
go h CASES=$T/c3.tsv SLOTS=5 JOIN_C1_DIR=$J/c1 JOIN_C1_SHAS=$J/c1.sha256 JOIN_C2_DIR=$J/c1 JOIN_C2_SHAS=$J/c1.sha256
grep -q '^Control gate: FAIL' "$T/h/table.md" && grep -q 'Semantic rejections counted: 0 of 13' "$T/h/table.md" && ok "c3: a c2 without replay=PASS leaves the gate FAIL, nothing counted" || bad "c3 h: $(grep -A1 'Control gate' "$T/h/table.md")"
echo "fails=$fails"; exit $fails
