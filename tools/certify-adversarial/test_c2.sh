#!/usr/bin/env bash
# Usage: bash test_ns.sh test_c2.sh "$PWD". Stub test of c2.sh with a fake driver whose --replay
# writes a fresh record (axioms from FAKE_AX_<name>) and exits FAKE_RC_<name>.
set -u
H=$1
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
S=$T/solar; mkdir -p "$S/tools/certify"
cat > "$S/tools/certify/certify" <<'EOF'
#!/usr/bin/env bash
d=$2; n=$(basename "$(dirname "$d")")
ax=FAKE_AX_${n//-/_}; rc=FAKE_RC_${n//-/_}
mkdir -p "$d.replay-1"
printf '{"replay": {"closure": 5, "replayed_count": 5, "trusted_count": 0, "axioms": %s}}\n' "${!ax:-[\"propext\", \"Classical.choice\", \"Quot.sound\"]}" > "$d.replay-1/record.json"
if [[ "${!rc:-0}" == 0 ]]; then echo "CERTIFIED: replay of $d: same pinned source/build inputs and identical checked outputs ($d.replay-1/record.json)"; else echo "REJECTED: replay: solar.binary_sha256 differs"; fi
exit "${!rc:-0}"
EOF
chmod +x "$S/tools/certify/certify"
git -C "$S" init -q; git -C "$S" add -A; git -C "$S" -c user.email=t@t -c user.name=t commit -qm init
A=$T/arch; mkdir -p "$A"
for n in control-flag control-siblingok control-flagrenamed; do
  mkdir -p "$T/src/$n/rec"; echo '{}' > "$T/src/$n/rec/record.json"; tar -C "$T/src" -czf "$A/$n.tgz" "$n"
done
(cd "$A" && sha256sum *.tgz > "$T/archives.sha256")
fails=0; ok() { echo "ok $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
go() { env "${@:2}" OUT=$T/$1 SOLAR_DIR=$S SOLAR_SHA=$(git -C "$S" rev-parse HEAD) LEAN_DIR=$T PACKAGES_FROM=$T/pk ARCHIVE_DIR=$A ARCHIVES=${ARCH:-$T/archives.sha256} BOX_CREATED=$(date -u +%FT%TZ) bash "$H/c2.sh" > "$T/$1.out" 2>&1; }
go a
[[ $(grep -c 'replay=PASS' "$T/a/table.txt") == 3 ]] && ok "3 replays PASS" || bad "a: $(cat "$T/a/table.txt")"
grep -q 'memory.max=4294967296 memory.swap.max=0' "$T/a/control-flag/status" && ok "scope read back" || bad "scope: $(head -1 "$T/a/control-flag/status")"
[[ $(ls "$T"/a/outbox/*.tgz | wc -l) == 3 ]] && (cd "$T/a/outbox" && sha256sum -c --quiet *.sha256) && ok "3 archives shipped, digests ok" || bad "ship a"
grep -q -- '--packages-from' "$T/a/control-flag/replay.command" && ok "replay gets --packages-from" || bad "cmd"
go b FAKE_RC_control_siblingok=1
grep -q '^control-siblingok DONE exit=1 replay=FAIL(exit 1; no reproduced line)' "$T/b/table.txt" && ok "a failing replay FAILs" || bad "b: $(cat "$T/b/table.txt")"
go c 'FAKE_AX_control_flag=["propext", "sorryAx"]'
grep -q '^control-flag DONE exit=0 replay=FAIL(axioms' "$T/c/table.txt" && ok "an extra axiom FAILs" || bad "c: $(cat "$T/c/table.txt")"
awk 'NR==1{c=substr($1,1,1); $1=(c=="0"?"1":"0") substr($1,2)} {print $1"  "$2}' "$T/archives.sha256" > "$T/bad.sha256"
ARCH=$T/bad.sha256 go d
grep -q 'REFUSED-ARCHIVES' "$T/d/run.log" && [[ ! -d "$T/d/control-flag" ]] && ok "archive digest mismatch refuses before any replay" || bad "d: $(tail -2 "$T/d/run.log")"
REPLAY_PASS_LINE="every pin reproduced by a fresh cold run" go e
[[ $(grep -c 'replay=FAIL(no reproduced line)' "$T/e/table.txt") == 3 ]] && ok "the old driver wording does not pass" || bad "e: $(cat "$T/e/table.txt")"
echo "fails=$fails"; exit $fails
