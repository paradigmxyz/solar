#!/usr/bin/env bash
# Prints the bin directory of a copy of the toolchain's executables whose lean is
# tools/certify/lean_iso.sh, made under OUT: the directory sandboxed steps put first on PATH. In
# threads mode its lean is instead the toolchain's lean with LEAN_NUM_THREADS set to
# CERTIFY_LEAN_THREADS (default 1), for the unsandboxed builds whose lake runs with more threads.
#
#   tools/certify/sysroot.sh TOOLCHAIN OUT [threads]
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tc=$1 out=$2 mode=${3:-iso}
[[ "$mode" == iso || "$mode" == threads ]] || { echo "sysroot: unknown mode $mode" >&2; exit 2; }
rm -rf "$out"
mkdir -p "$out/bin"
for d in "$tc"/*; do
    [[ "$(basename "$d")" == bin ]] || ln -s "$d" "$out/$(basename "$d")"
done
# lake finds lean beside its own executable, so lake is copied, not linked.
for b in "$tc"/bin/*; do
    case "$(basename "$b")" in
        lean) ;;
        lake) cp "$b" "$out/bin/lake" ;;
        *) ln -s "$b" "$out/bin/$(basename "$b")" ;;
    esac
done
if [[ "$mode" == threads ]]; then
    # The generated lean expands CERTIFY_LEAN_THREADS when it runs, not here.
    # shellcheck disable=SC2016
    printf '#!/usr/bin/env bash\nexec env LEAN_NUM_THREADS="${CERTIFY_LEAN_THREADS:-1}" "%s/bin/lean" "$@"\n' "$tc" > "$out/bin/lean"
else
    cp "$here/lean_iso.sh" "$out/bin/lean_iso.sh"
    printf '#!/usr/bin/env bash\nexec bash "%s/bin/lean_iso.sh" "$@"\n' "$out" > "$out/bin/lean"
fi
chmod +x "$out/bin/lean" "$out/bin/lake"
echo "$out/bin"
