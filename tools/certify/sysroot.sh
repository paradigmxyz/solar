#!/usr/bin/env bash
# Prints the bin directory of a copy of the toolchain's executables whose lean is
# tools/certify/lean_iso.sh, made under OUT: the directory sandboxed steps put first on PATH.
#
#   tools/certify/sysroot.sh TOOLCHAIN OUT
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tc=$1 out=$2
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
cp "$here/lean_iso.sh" "$out/bin/lean_iso.sh"
printf '#!/usr/bin/env bash\nexec bash "%s/bin/lean_iso.sh" "$@"\n' "$out" > "$out/bin/lean"
chmod +x "$out/bin/lean" "$out/bin/lake"
echo "$out/bin"
