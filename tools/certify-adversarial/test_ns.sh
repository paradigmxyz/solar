#!/usr/bin/env bash
# Runs the stub in a user and mount namespace whose /sys/fs/cgroup is a tmpfs with a finite
# memory.max and swap 0, standing in for a box scope (the hosted Hand has no systemd).
set -eu
unshare -r -m bash -c '
mount -t tmpfs none /sys/fs/cgroup
echo 4294967296 > /sys/fs/cgroup/memory.max
echo 0 > /sys/fs/cgroup/memory.swap.max
echo "oom 0" > /sys/fs/cgroup/memory.events
echo 1 > /sys/fs/cgroup/memory.peak
exec bash "$0" "$1"' "$@"
