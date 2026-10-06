#!/usr/bin/env bash
# Install the Lean toolchain pinned in scripts/evm-rules/lean/lean-toolchain for this
# runner's architecture, from a cached archive when there is one.
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(sed 's/leanprover\/lean4:v//' scripts/evm-rules/lean/lean-toolchain)
case "$(uname -m)" in
  x86_64)
    platform=linux
    checksum=47bf4bbd78f70c2e9670598ab7124d92b6efb7330ff33e5fbb4030f6fd72e4e4
    ;;
  aarch64)
    platform=linux_aarch64
    checksum=fdb974c2cdb4627e090d5d4007b913e09d13c4868720fb5594e22808b3de9e37
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
