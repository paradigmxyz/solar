#!/usr/bin/env bash
# Runs a command that may execute code of an untrusted Lean module (a build, or a Lean frontend
# that imports one, whose initializers run) in its own PID namespace, killed with it, and a private
# mount namespace where all of HOME is read-only but one directory, with private /tmp, /dev/shm, /var/tmp and /run/user, from the
# current directory.
#
#   tools/certify/sandbox.sh RW_DIR CMD...
set -euo pipefail
rw=$1
shift
exec unshare -r -m -p -f --kill-child --mount-proc bash -c 'set -e; home=$1 rw=$2 dir=$3; shift 3
    mount --bind "$home" "$home"; mount -o remount,bind,ro "$home" "$home"
    for t in /tmp /dev/shm /var/tmp /run/user; do if [[ -d "$t" ]]; then mount -t tmpfs tmpfs "$t"; fi; done
    mount --bind "$rw" "$rw"; mount -o remount,bind,rw "$rw" "$rw"
    cd "$dir"; exec "$@"' _ "$HOME" "$rw" "$PWD" "$@"
