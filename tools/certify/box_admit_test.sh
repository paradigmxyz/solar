#!/usr/bin/env bash
# Tests of tools/certify/box_admit.sh on fixture cgroup and /proc trees: a valid scope, process
# tree and run directory pass, and every admission or receipt failure aborts with a message.
#   bash tools/certify/box_admit_test.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tool="$here/box_admit.sh"
tmp="$(mktemp -d)"
trap 'chmod -R u+rwX "$tmp"; rm -rf "$tmp"' EXIT
fails=0
ok() { echo "ok: $*"; }
bad() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }

cg=/system.slice/run-1.scope
mem=47244640256
export BOX_ADMIT_CGROUP_ROOT="$tmp/cgroup" BOX_ADMIT_PROC_ROOT="$tmp/proc"

# A fake process: pid, parent, cgroup.
proc_add() {
    mkdir -p "$tmp/proc/$1"
    printf '%s (some cmd) S %s 1 1 0\n' "$1" "$2" > "$tmp/proc/$1/stat"
    printf '0::%s\n' "$3" > "$tmp/proc/$1/cgroup"
}

# The valid fixture: the scope at 44G with no swap, the inner pid 100 with two levels of
# descendants in it, and an unrelated process outside it.
fixture() {
    rm -rf "$tmp/cgroup" "$tmp/proc" "$tmp/run"
    mkdir -p "$tmp/cgroup$cg"
    echo "$mem" > "$tmp/cgroup$cg/memory.max"
    echo 0 > "$tmp/cgroup$cg/memory.swap.max"
    printf '100\n101\n102\n' > "$tmp/cgroup$cg/cgroup.procs"
    proc_add 1 0 /init.scope
    proc_add 50 1 /user.slice
    proc_add 100 1 "$cg"
    proc_add 101 100 "$cg"
    proc_add 102 101 "$cg"
    mkdir -p "$tmp/run/record"
    echo '{"statuses": {}}' > "$tmp/run/record/record.json"
    echo '{"bind": 1}' > "$tmp/run/bind.json"
    echo '{"closure": 1}' > "$tmp/run/replay-receipt.json"
    echo "'T' depends on axioms: [propext, Classical.choice, Quot.sound]" > "$tmp/run/axioms.out"
    echo 'c every-caller negative tests: 19 cases, 0 failed, 0 skipped' > "$tmp/run/negative_any.out"
    (cd "$tmp/run" && sha256sum record/record.json bind.json replay-receipt.json axioms.out negative_any.out > receipts.sha256)
}

# expect_pass NAME CMD...; expect_abort NAME PATTERN CMD...
expect_pass() {
    local name="$1"
    shift
    if "$tool" "$@" > "$tmp/out" 2>&1; then ok "$name: $(cat "$tmp/out")"; else cat "$tmp/out" >&2; bad "$name is refused"; fi
}
expect_abort() {
    local name="$1" pat="$2"
    shift 2
    if "$tool" "$@" > "$tmp/out" 2>&1; then
        bad "$name is accepted"
    elif grep -q "^box_admit: ABORT: .*$pat" "$tmp/out"; then
        ok "$name aborts: $(cat "$tmp/out")"
    else
        cat "$tmp/out" >&2
        bad "$name aborts without the message '$pat'"
    fi
}

fixture
expect_pass "the valid scope" admit "$cg" 100 "$mem"
expect_pass "the valid process tree" pidtree "$cg" 100
expect_pass "the valid run directory" receipts "$tmp/run"

fixture; chmod 000 "$tmp/cgroup$cg/memory.max"
if [[ -r "$tmp/cgroup$cg/memory.max" ]]; then rm "$tmp/cgroup$cg/memory.max"; fi
expect_abort "an unreadable memory.max" "cannot read .*memory.max" admit "$cg" 100 "$mem"
fixture; echo max > "$tmp/cgroup$cg/memory.max"
expect_abort "memory.max max" "memory.max is 'max' (unlimited)" admit "$cg" 100 "$mem"
fixture; echo 8589934592 > "$tmp/cgroup$cg/memory.max"
expect_abort "another memory.max" "memory.max is '8589934592', expected $mem" admit "$cg" 100 "$mem"
fixture; echo max > "$tmp/cgroup$cg/memory.swap.max"
expect_abort "an unlimited swap.max" "memory.swap.max is 'max', expected 0" admit "$cg" 100 "$mem"
fixture; echo 1048576 > "$tmp/cgroup$cg/memory.swap.max"
expect_abort "a nonzero swap.max" "memory.swap.max is '1048576', expected 0" admit "$cg" 100 "$mem"
fixture; rm "$tmp/cgroup$cg/memory.swap.max"
expect_abort "an unreadable swap.max" "cannot read .*memory.swap.max" admit "$cg" 100 "$mem"
fixture; rm -rf "$tmp/cgroup$cg"
expect_abort "a missing scope" "scope cgroup .* does not exist" admit "$cg" 100 "$mem"
expect_abort "a missing scope in a sample" "scope cgroup .* does not exist" pidtree "$cg" 100
fixture
expect_abort "an empty scope name" "is not an absolute cgroup path" admit "" 100 "$mem"
fixture; printf '101\n102\n' > "$tmp/cgroup$cg/cgroup.procs"
expect_abort "an inner pid not listed in the scope" "inner pid 100 is not in .*cgroup.procs" admit "$cg" 100 "$mem"
fixture; printf '0::/user.slice\n' > "$tmp/proc/100/cgroup"
expect_abort "an inner pid in another cgroup" "inner pid 100 is in cgroup /user.slice, not the scope" admit "$cg" 100 "$mem"
fixture; rm "$tmp/proc/100/cgroup"
expect_abort "an inner pid whose cgroup is unreadable" "cannot read the cgroup of inner pid 100" admit "$cg" 100 "$mem"
fixture
expect_abort "an inner pid that is not a pid" "is not a pid" admit "$cg" "" "$mem"
fixture; proc_add 103 102 /user.slice/escaped.scope
expect_abort "a grandchild outside the scope" "escaped the scope $cg: 103:/user.slice/escaped.scope" pidtree "$cg" 100
fixture; proc_add 103 102 "$cg"; rm "$tmp/proc/103/cgroup"
expect_abort "a descendant whose cgroup is unreadable" "escaped the scope $cg: 103:unreadable" pidtree "$cg" 100
fixture; rm -rf "$tmp/proc/100"
expect_abort "a sample of an inner pid that is gone" "inner pid 100 is not running" pidtree "$cg" 100

for r in record/record.json bind.json replay-receipt.json axioms.out negative_any.out receipts.sha256; do
    fixture; rm "$tmp/run/$r"
    expect_abort "a missing $r" "required receipt $r is missing" receipts "$tmp/run"
done
fixture; echo '{"bind": 2}' > "$tmp/run/bind.json"
expect_abort "a receipt that differs from receipts.sha256" "receipt bind.json has sha256 .*, receipts.sha256 says" receipts "$tmp/run"
fixture; grep -v ' axioms.out$' "$tmp/run/receipts.sha256" > "$tmp/s" && mv "$tmp/s" "$tmp/run/receipts.sha256"
expect_abort "a receipt not listed in receipts.sha256" "required receipt axioms.out is not listed" receipts "$tmp/run"
fixture; echo 'not a digest line' >> "$tmp/run/receipts.sha256"
expect_abort "a malformed receipts.sha256" "malformed receipts.sha256 line" receipts "$tmp/run"
fixture; echo 'c every-caller negative tests: 19 cases, 2 failed, 0 skipped' > "$tmp/run/negative_any.out"
(cd "$tmp/run" && sha256sum record/record.json bind.json replay-receipt.json axioms.out negative_any.out > receipts.sha256)
expect_abort "negative tests with failures" "negative_any.out reports failures" receipts "$tmp/run"
fixture; echo 'no summary' > "$tmp/run/negative_any.out"
(cd "$tmp/run" && sha256sum record/record.json bind.json replay-receipt.json axioms.out negative_any.out > receipts.sha256)
expect_abort "negative tests without a summary" "negative_any.out has no summary line" receipts "$tmp/run"

if [[ $fails -ne 0 ]]; then
    echo "box_admit tests: $fails failed" >&2
    exit 1
fi
echo "box_admit tests: all passed"
