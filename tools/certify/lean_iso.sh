#!/usr/bin/env bash
# The lean the driver's sandboxed steps run (tools/certify/sysroot.sh installs it as bin/lean of a
# copy of the toolchain's bin directory, so that lake runs it for every module it compiles): the
# toolchain's lean in a further sandbox that may write only a scratch
# directory, its outputs (-o, -i, -c, -b) redirected there and moved to where they were asked for
# once it succeeds. A module's code then never writes any file but its own outputs, whatever else
# the build directory holds.
#
#   lean_iso.sh ARGS...    (the toolchain's lean ARGS...)
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
real="${CERTIFY_REAL_LEAN:?}"
iso="${CERTIFY_ISO_DIR:?}"
mkdir -p "$iso"
scratch="$(mktemp -d "$iso/lean.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
args=() moves=()
while [[ $# -gt 0 ]]; do
    case $1 in
        -o|-i|-c|-b)
            [[ $# -ge 2 ]] || { echo "lean_iso: $1 needs a path" >&2; exit 2; }
            n=${#moves[@]}
            args+=("$1" "$scratch/out$n")
            moves+=("$2")
            shift 2 ;;
        -o?*|-i?*|-c?*|-b?*|--o=*|--i=*|--c=*|--bc=*|--olean=*|--ilean=*)
            echo "lean_iso: unsupported output argument $1" >&2; exit 2 ;;
        *) args+=("$1"); shift ;;
    esac
done
# In the step's sandbox only the build directory is writable: here it is read-only too, but for
# the scratch directory, with fresh temporary directories.
unshare -r -m -p -f --kill-child --mount-proc bash -c 'set -e; ro=$1 rw=$2; shift 2
    mount --bind "$ro" "$ro"; mount -o remount,bind,ro,nosuid,nodev "$ro"
    mount --bind "$rw" "$rw"; mount -o remount,bind,rw,nosuid,nodev "$rw"
    for t in /tmp /dev/shm /var/tmp /run/user; do if [[ -d "$t" ]]; then mount -t tmpfs tmpfs "$t"; fi; done
    exec "$@"' _ "${CERTIFY_ISO_RO:?}" "$scratch" "$real" ${args[@]+"${args[@]}"}
for n in ${moves[@]+"${!moves[@]}"}; do
    out="$scratch/out$n"
    [[ -e "$out" || -L "$out" ]] || continue
    # Only a regular file is moved: a link could point at a file a later step may change.
    [[ -f "$out" && ! -L "$out" ]] || { echo "lean_iso: output $n is not a regular file" >&2; exit 1; }
    mkdir -p "$(dirname "${moves[$n]}")"
    mv -f "$scratch/out$n" "${moves[$n]}"
done
