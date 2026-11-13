#!/usr/bin/env bash
# Install the Lean toolchain pinned in scripts/evm-rules/lean/lean-toolchain for this
# runner's architecture, from a cached archive when there is one.
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(sed 's/leanprover\/lean4:v//' scripts/evm-rules/lean/lean-toolchain)
case "$(uname -m)" in
  x86_64)
    platform=linux
    checksum=7289597a44543246c9227b8cae501767e095ce17f6d448b95fc4a8dc97268f6d
    ;;
  aarch64)
    platform=linux_aarch64
    checksum=c8f0f8a3920e839718b1343e6781fd80a3e9b1024fbcd7078eb30f54e806b028
    ;;
  *) echo 'Unsupported Lean installation architecture' >&2; exit 1 ;;
esac

directory="${RUNNER_TEMP:?}/solar-lean"
archive="$directory/lean-$version-$platform.tar.zst"
mkdir -p "$directory"
if [[ ! -f "$archive" ]]; then
  curl --fail --location --retry 3 --silent --show-error \
    "https://github.com/leanprover/lean4/releases/download/v$version/lean-$version-$platform.tar.zst" \
    --output "$archive"
fi
printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check
tar --zstd -xf "$archive" -C "$directory"
echo "$directory/lean-$version-$platform/bin" >> "${GITHUB_PATH:?}"
"$directory/lean-$version-$platform/bin/lean" --version
