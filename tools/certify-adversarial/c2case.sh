#!/usr/bin/env bash
# c2case.sh NAME: one control's cold replay, run by c2.sh in the control's own scope. The replay
# passes only when the driver's last line contains REPLAY_PASS_LINE (the driver's own success
# wording, which c2.sh records).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
name=$1; R=$OUT/$name
cg=$(sed -n 's/^0:://p' /proc/self/cgroup | head -1)
mmax=$(cat "/sys/fs/cgroup$cg/memory.max" 2>/dev/null || echo unreadable)
smax=$(cat "/sys/fs/cgroup$cg/memory.swap.max" 2>/dev/null || echo unreadable)
ship() {
    { echo "memory.events $(tr '\n' ' ' < "/sys/fs/cgroup$cg/memory.events" 2>/dev/null)"
      echo "memory.peak $(cat "/sys/fs/cgroup$cg/memory.peak" 2>/dev/null || echo unreadable)"; } > "$R/scope.txt"
    tar -C "$OUT" --exclude="$name/rec-replay-work-*" -czf "$OUT/outbox/$name.tgz.part" "$name"
    mv "$OUT/outbox/$name.tgz.part" "$OUT/outbox/$name.tgz"
    (cd "$OUT/outbox" && sha256sum "$name.tgz" > "$name.tgz.sha256")
}
if [[ ! "$mmax" =~ ^[0-9]+$ || "$smax" != 0 ]]; then
    echo "REFUSED-SCOPE cgroup=$cg memory.max=$mmax memory.swap.max=$smax" > "$R/status"; ship; exit 0
fi
left=$((DEADLINE - $(date +%s)))
echo "START $(date -u +%FT%TZ) cgroup=$cg memory.max=$mmax memory.swap.max=$smax budget_s=$left" > "$R/status"
cmd=(tools/certify/certify --replay "$R/rec" --solidity-lean "$LEAN_DIR" --packages-from "$PACKAGES_FROM")
printf '%q ' "${cmd[@]}" > "$R/replay.command"; echo >> "$R/replay.command"
(cd "$SOLAR_DIR" && exec setsid -w timeout -k 30 "$left" "${cmd[@]}") > "$R/replay.log" 2>&1
rc=$?
line=$(grep -v '^\s*$' "$R/replay.log" | tail -1)
fresh=$(ls -d "$R"/rec.replay-*/record.json 2>/dev/null | head -1)
check=$(python3 - "$rc" "$line" "${fresh:-}" "$REPLAY_PASS_LINE" <<'EOF'
import json, sys
rc, line, fresh, want = sys.argv[1:5]
why = []
if rc != "0": why.append(f"exit {rc}")
if not want or want not in line: why.append("no reproduced line")
r = {}
if fresh:
    r = (json.load(open(fresh)).get("replay") or {})
else:
    why.append("no fresh record")
ax = sorted(r.get("axioms") or [])
if fresh and ax != sorted(["propext", "Classical.choice", "Quot.sound"]): why.append(f"axioms {ax}")
if fresh and r.get("trusted_count") != 0: why.append(f"trusted {r.get('trusted_count')}")
print(("replay=PASS" if not why else "replay=FAIL(" + "; ".join(why) + ")")
      + f" closure={r.get('closure')} replayed={r.get('replayed_count')} trusted={r.get('trusted_count')} axioms={','.join(ax)}")
EOF
)
kind=DONE; [[ $rc == 124 || $rc == 137 ]] && kind=KILLED-DEADLINE
echo "$kind exit=$rc $check log_sha256=$(sha256sum < "$R/replay.log" | cut -c1-64) $(date -u +%FT%TZ)" >> "$R/status"
ship
