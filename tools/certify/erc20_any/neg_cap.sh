#!/usr/bin/env bash
# Negative controls for build_cap.sh admission, no build: each case must exit 95 without running
# its command (the command would create a marker file). Usage: neg_cap.sh OUTDIR
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd) out=$1; mkdir -p "$out"; log=$out/neg_cap.txt; : > "$log"
run() { local case=$1; shift; local m=$out/ran-$case; rm -f "$m"
  "$@" bash "$here/build_cap.sh" 2 1048576 "$out/gate-$case.txt" "neg$case" -- touch "$m" > /dev/null 2>&1; local rc=$?
  local ran=no; [[ -e $m ]] && ran=yes
  local ok=FAIL; [[ $rc == 95 && $ran == no ]] && ok=OK
  echo "case=$case exit=$rc workload_ran=$ran $ok $(tail -1 "$out/gate-$case.txt" 2>/dev/null)" | tee -a "$log"; }
t=$(mktemp -d)
# (a1) cgroup root does not exist.
run a1-missing env CG_ROOT="$t/none" SUDO=
# (a2) cgroup root exists but is not writable.
mkdir "$t/ro"; chmod 555 "$t/ro"; run a2-readonly env CG_ROOT="$t/ro" SUDO=
# (a3) cgroup created but the memory controller is not enabled in it.
mkdir "$t/nomem"; run a3-no-memory env CG_ROOT="$t/nomem" SUDO=
# (b) memory.max accepts the write but reads back a different value: a fake cgroup directory
# whose memory.max is /dev/null (write succeeds, readback is empty). A mkdir wrapper builds it.
mkdir -p "$t/bin" "$t/mm"; cat > "$t/bin/mkfake" <<'EOF'
#!/usr/bin/env bash
[[ $1 == mkdir ]] || exec "$@"
mkdir "$2" && echo "cpu memory pids" > "$2/cgroup.controllers" && ln -s /dev/null "$2/memory.max" && echo 0 > "$2/memory.swap.max"
EOF
chmod +x "$t/bin/mkfake"; run b-readback env CG_ROOT="$t/mm" SUDO="$t/bin/mkfake"
chmod 755 "$t/ro"; rm -rf "$t"
