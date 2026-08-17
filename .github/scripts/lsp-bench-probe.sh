#!/usr/bin/env bash
# Probe the Linux capabilities required by an authoritative LSP benchmark run.
set -Eeuo pipefail

readonly OUTPUT_DIR="${LSP_BENCH_PROBE_OUTPUT:-target/lsp-bench/probe}"
readonly STATUS_LOG="$OUTPUT_DIR/checks.tsv"
mkdir -p "$OUTPUT_DIR"
if [[ "${LSP_BENCH_PROBE_CAPTURED:-}" != 1 ]]; then
    export LSP_BENCH_PROBE_CAPTURED=1
    set +e
    bash "${BASH_SOURCE[0]}" "$@" 2>&1 | tee "$OUTPUT_DIR/probe.txt"
    exit "${PIPESTATUS[0]}"
fi
: > "$STATUS_LOG"
rm -f "$OUTPUT_DIR/cgroup-mkdir.err" "$OUTPUT_DIR/cgroup-values.env" "$OUTPUT_DIR/status.env"

failures=0
cgroup_path=""
helper_pid=""
fifo_path=""

mark_pass() {
    printf 'PASS\t%s\n' "$1" | tee -a "$STATUS_LOG"
}

mark_fail() {
    printf 'FAIL\t%s\n' "$1" | tee -a "$STATUS_LOG"
    failures=1
}

cleanup_probe_cgroup() {
    local attempts

    set +e
    if [[ -n "$helper_pid" ]] && kill -0 "$helper_pid" 2>/dev/null; then
        kill "$helper_pid" 2>/dev/null
        wait "$helper_pid" 2>/dev/null
    fi
    if [[ -n "$fifo_path" ]]; then
        rm -f "$fifo_path"
        fifo_path=""
    fi
    if [[ -n "$cgroup_path" && -d "$cgroup_path" ]]; then
        while read -r pid; do
            [[ "$pid" =~ ^[0-9]+$ ]] || continue
            kill "$pid" 2>/dev/null
        done < "$cgroup_path/cgroup.procs"
        for ((attempts = 0; attempts < 100; attempts++)); do
            rmdir "$cgroup_path" 2>/dev/null && break
            sleep 0.01
        done
    fi
    helper_pid=""
    cgroup_path=""
}

write_status() {
    local rc="$1"
    local status="pass"
    (( failures == 0 && rc == 0 )) || status="fail"
    {
        printf 'status=%s\n' "$status"
        printf 'exit_code=%s\n' "$rc"
        printf 'failures=%s\n' "$failures"
        printf 'runner_name=%s\n' "${RUNNER_NAME-}"
        printf 'runner_arch=%s\n' "${RUNNER_ARCH-}"
        printf 'runner_os=%s\n' "${RUNNER_OS-}"
        printf 'runner_label=%s\n' "${LSP_BENCH_RUNNER_LABEL-}"
        printf 'image_os=%s\n' "${ImageOS-}"
        printf 'image_version=%s\n' "${ImageVersion-}"
        printf 'kernel=%s\n' "$(uname -srmo 2>/dev/null || printf unavailable)"
    } > "$OUTPUT_DIR/status.env"
}

on_exit() {
    local rc="$?"
    cleanup_probe_cgroup
    if (( rc != 0 )); then
        failures=1
        printf 'EXIT\t%s\n' "$rc" | tee -a "$STATUS_LOG"
    fi
    write_status "$rc"
    return "$rc"
}
trap on_exit EXIT

printf 'probe_started_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf 'runner_name=%s\n' "${RUNNER_NAME-}"
printf 'runner_arch=%s\n' "${RUNNER_ARCH-}"
printf 'runner_os=%s\n' "${RUNNER_OS-}"
printf 'runner_label=%s\n' "${LSP_BENCH_RUNNER_LABEL-}"
printf 'image_os=%s\n' "${ImageOS-}"
printf 'image_version=%s\n' "${ImageVersion-}"
uname -a
uname -m
id
if [[ -r /etc/os-release ]]; then
    cat /etc/os-release
else
    mark_fail os-release
fi
printf '\ncgroup mount:\n'
if command -v findmnt >/dev/null 2>&1; then
    findmnt -rn -T /sys/fs/cgroup -o TARGET,FSTYPE,OPTIONS || true
else
    mark_fail findmnt-command
fi
printf '\nlscpu:\n'
if command -v lscpu >/dev/null 2>&1; then
    if ! lscpu; then
        mark_fail lscpu-metadata
    fi
else
    mark_fail lscpu-command
fi
printf '\nprobe tool versions:\n'
command -v unshare || true
command -v ip || true
command -v python3 || true
command -v unshare >/dev/null 2>&1 && unshare --version 2>&1 || true
command -v ip >/dev/null 2>&1 && ip -V 2>&1 || true
command -v python3 >/dev/null 2>&1 && python3 --version 2>&1 || true

if [[ "$(uname -s)" != Linux ]]; then
    mark_fail linux
fi
if [[ "$(uname -m)" != x86_64 ]]; then
    mark_fail x86_64
fi
if ! command -v unshare >/dev/null 2>&1; then
    mark_fail unshare-command
fi
if ! command -v ip >/dev/null 2>&1; then
    mark_fail ip-command
fi
if ! command -v python3 >/dev/null 2>&1; then
    mark_fail python3-command
fi

probe_cgroup_v2() {
    local relative parent before_usage after_usage before_user user_usec
    local before_system system_usec before_peak peak populated
    local member_count descendant_observed
    local i

    if ! command -v findmnt >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1;
    then
        mark_fail cgroup-v2-prerequisites
        return
    fi

    if ! relative="$(awk -F: '$1 == 0 { print $3; found = 1 } END { if (!found) exit 1 }' /proc/self/cgroup)"; then
        mark_fail cgroup-v2-membership
        return
    fi
    if [[ "$relative" != /* ]]; then
        mark_fail cgroup-v2-membership
        return
    fi
    parent="/sys/fs/cgroup$relative"
    if [[ ! -d "$parent" ]]; then
        mark_fail cgroup-v2-parent
        return
    fi
    if [[ "$(findmnt -rn -T "$parent" -o FSTYPE 2>/dev/null || true)" != cgroup2 ]]; then
        mark_fail cgroup-v2-mount
        return
    fi

    cgroup_path="$parent/solar-lsp-bench-probe-$BASHPID-$(date +%s%N)"
    if ! mkdir "$cgroup_path" 2>"$OUTPUT_DIR/cgroup-mkdir.err"; then
        mark_fail cgroup-v2-child-create
        return
    fi
    if [[ ! -r "$cgroup_path/cpu.stat" || ! -r "$cgroup_path/memory.peak" ||
        ! -r "$cgroup_path/cgroup.events" || ! -w "$cgroup_path/cgroup.procs" ]]; then
        mark_fail cgroup-v2-child-files
        return
    fi
    if [[ -e "$cgroup_path/cgroup.kill" && ! -w "$cgroup_path/cgroup.kill" ]]; then
        mark_fail cgroup-v2-child-kill
        return
    fi

    fifo_path="$OUTPUT_DIR/cgroup-go-$BASHPID"
    if ! mkfifo "$fifo_path"; then
        mark_fail cgroup-v2-helper-fifo
        return
    fi

    # The helper waits before forking so its descendant inherits the child cgroup.
    python3 - "$fifo_path" <<'PY' &
import os
import sys
import time

with open(sys.argv[1]) as gate:
    gate.read(1)
child = os.fork()
if child == 0:
    payload = bytearray(16 * 1024 * 1024)
    for offset in range(0, len(payload), 4096):
        payload[offset] = 1
    deadline = time.monotonic() + 0.8
    while time.monotonic() < deadline:
        pass
    os._exit(0)
os.waitpid(child, 0)
PY
    helper_pid="$!"

    # Give the helper a chance to open the FIFO, while keeping it in the parent cgroup.
    for ((i = 0; i < 100; i++)); do
        [[ -p "$fifo_path" ]] && break
        sleep 0.01
    done
    if ! kill -0 "$helper_pid" 2>/dev/null; then
        mark_fail cgroup-v2-helper-start
        return
    fi
    if ! printf '%s\n' "$helper_pid" > "$cgroup_path/cgroup.procs"; then
        mark_fail cgroup-v2-process-migration
        return
    fi
    before_usage="$(awk '$1 == "usage_usec" { print $2 }' "$cgroup_path/cpu.stat")"
    before_user="$(awk '$1 == "user_usec" { print $2 }' "$cgroup_path/cpu.stat")"
    before_system="$(awk '$1 == "system_usec" { print $2 }' "$cgroup_path/cpu.stat")"
    before_peak="$(cat "$cgroup_path/memory.peak")"
    populated="$(awk '$1 == "populated" { print $2 }' "$cgroup_path/cgroup.events")"
    if [[ "$populated" != 1 ]]; then
        mark_fail cgroup-v2-populated-event
        return
    fi
    if ! printf 'go\n' > "$fifo_path"; then
        mark_fail cgroup-v2-helper-release
        return
    fi

    descendant_observed=0
    for ((i = 0; i < 100; i++)); do
        member_count="$(wc -l < "$cgroup_path/cgroup.procs")"
        if [[ "$member_count" -ge 2 ]]; then
            descendant_observed=1
            break
        fi
        kill -0 "$helper_pid" 2>/dev/null || break
        sleep 0.01
    done
    if [[ "$descendant_observed" != 1 ]]; then
        mark_fail cgroup-v2-fork-descendant
    fi

    if ! wait "$helper_pid"; then
        mark_fail cgroup-v2-helper-exit
        return
    fi
    helper_pid=""
    populated=""
    for ((i = 0; i < 100; i++)); do
        populated="$(awk '$1 == "populated" { print $2 }' "$cgroup_path/cgroup.events")"
        [[ "$populated" == 0 ]] && break
        sleep 0.01
    done
    if [[ "$populated" != 0 ]]; then
        mark_fail cgroup-v2-empty-event
    fi

    after_usage="$(awk '$1 == "usage_usec" { print $2 }' "$cgroup_path/cpu.stat")"
    user_usec="$(awk '$1 == "user_usec" { print $2 }' "$cgroup_path/cpu.stat")"
    system_usec="$(awk '$1 == "system_usec" { print $2 }' "$cgroup_path/cpu.stat")"
    peak="$(cat "$cgroup_path/memory.peak")"
    printf 'cgroup_path=%s\nusage_before=%s\nusage_after=%s\nuser_before=%s\nuser_after=%s\nsystem_before=%s\nsystem_after=%s\nmemory_peak_before=%s\nmemory_peak_after=%s\npopulated_final=%s\ndescendant_observed=%s\n' \
        "$cgroup_path" "$before_usage" "$after_usage" "$before_user" "$user_usec" \
        "$before_system" "$system_usec" "$before_peak" "$peak" "$populated" \
        "$descendant_observed" \
        > "$OUTPUT_DIR/cgroup-values.env"

    if [[ ! "$before_usage" =~ ^[0-9]+$ || ! "$after_usage" =~ ^[0-9]+$ ||
        ! "$before_user" =~ ^[0-9]+$ || ! "$user_usec" =~ ^[0-9]+$ ||
        ! "$before_system" =~ ^[0-9]+$ || ! "$system_usec" =~ ^[0-9]+$ ||
        ! "$before_peak" =~ ^[0-9]+$ || ! "$peak" =~ ^[0-9]+$ ||
        "$after_usage" -lt $((before_usage + 250000)) ||
        "$user_usec" -lt $((before_user + 250000)) ||
        "$peak" -lt $((before_peak + 8 * 1024 * 1024)) ]]; then
        mark_fail cgroup-v2-descendant-accounting
        return
    fi
    if [[ "$populated" != 0 || "$descendant_observed" != 1 ]]; then
        return
    fi
    if ! rmdir "$cgroup_path"; then
        mark_fail cgroup-v2-child-cleanup
        return
    fi
    cgroup_path=""
    rm -f "$fifo_path"
    fifo_path=""
    mark_pass cgroup-v2-descendant-accounting
}
probe_cgroup_v2

probe_user_namespace() {
    if ! command -v unshare >/dev/null 2>&1; then
        mark_fail user-namespace
        return
    fi
    if unshare --user --map-root-user -- sh -c 'test "$(id -u)" = 0'; then
        mark_pass user-namespace
    else
        mark_fail user-namespace
    fi
}
probe_user_namespace

probe_user_network_namespace() {
    if ! command -v unshare >/dev/null 2>&1 || ! command -v ip >/dev/null 2>&1 ||
        ! command -v python3 >/dev/null 2>&1;
    then
        mark_fail network-namespace-loopback
        return
    fi
    if unshare --user --map-root-user --net -- bash -s <<'NS_PROBE'
set -Eeuo pipefail

namespace_cgroup=""
namespace_helper_pid=""
cleanup_namespace_cgroup() {
    local attempt

    set +e
    if [[ -n "$namespace_helper_pid" ]] && kill -0 "$namespace_helper_pid" 2>/dev/null; then
        kill "$namespace_helper_pid" 2>/dev/null
        wait "$namespace_helper_pid" 2>/dev/null
    fi
    if [[ -n "$namespace_cgroup" && -d "$namespace_cgroup" ]]; then
        if [[ -w "$namespace_cgroup/cgroup.kill" ]]; then
            printf '1\n' > "$namespace_cgroup/cgroup.kill" 2>/dev/null
        fi
        for ((attempt = 0; attempt < 100; attempt++)); do
            rmdir "$namespace_cgroup" 2>/dev/null && break
            sleep 0.01
        done
    fi
}
trap cleanup_namespace_cgroup EXIT

ip link set lo up
relative="$(awk -F: '$1 == 0 { print $3; found = 1 } END { if (!found) exit 1 }' /proc/self/cgroup)"
namespace_parent="/sys/fs/cgroup$relative"
namespace_cgroup="${namespace_parent%/}/solar-lsp-bench-namespace-probe-$BASHPID-$(date +%s%N)"
mkdir "$namespace_cgroup"
test -r "$namespace_cgroup/cpu.stat"
test -r "$namespace_cgroup/memory.peak"
test -r "$namespace_cgroup/cgroup.events"
test -w "$namespace_cgroup/cgroup.procs"
sleep 1 &
namespace_helper_pid="$!"
printf '%s\n' "$namespace_helper_pid" > "$namespace_cgroup/cgroup.procs"
grep -qx "$namespace_helper_pid" "$namespace_cgroup/cgroup.procs"
test "$(awk '$1 == "populated" { print $2 }' "$namespace_cgroup/cgroup.events")" = 1
wait "$namespace_helper_pid"
namespace_helper_pid=""
test -n "$(awk '$1 == "user_usec" { print $2 }' "$namespace_cgroup/cpu.stat")"
test -n "$(awk '$1 == "system_usec" { print $2 }' "$namespace_cgroup/cpu.stat")"
test "$(cat "$namespace_cgroup/memory.peak")" -gt 0
for _ in {1..100}; do
    [[ "$(awk '$1 == "populated" { print $2 }' "$namespace_cgroup/cgroup.events")" == 0 ]] && break
    sleep 0.01
done
test "$(awk '$1 == "populated" { print $2 }' "$namespace_cgroup/cgroup.events")" = 0
rmdir "$namespace_cgroup"
namespace_cgroup=""

python3 - <<'PY'
import os
import socket

assert sorted(os.listdir("/sys/class/net")) == ["lo"]

marker = b"solar-probe"
listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
listener.settimeout(2)
listener.bind(("127.0.0.1", 0))
listener.listen(1)
port = listener.getsockname()[1]
server_pid = os.fork()
if server_pid == 0:
    try:
        connection, _ = listener.accept()
        connection.sendall(marker)
        connection.close()
        os._exit(0)
    except Exception:
        os._exit(1)
try:
    client = socket.create_connection(("127.0.0.1", port), timeout=2)
    received = bytearray()
    while len(received) < len(marker):
        chunk = client.recv(len(marker) - len(received))
        if not chunk:
            break
        received.extend(chunk)
    assert bytes(received) == marker
    client.close()
finally:
    listener.close()
    _, status = os.waitpid(server_pid, 0)
    assert os.waitstatus_to_exitcode(status) == 0
PY
NS_PROBE
    then
        mark_pass namespace-cgroup-loopback-composition
    else
        mark_fail namespace-cgroup-loopback-composition
    fi
}
probe_user_network_namespace

exit "$failures"
