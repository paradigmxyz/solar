#!/usr/bin/env bash
# Regenerates the frozen VC benchmark (the examples' exports) into DIR and proves it with
# discharge.sh against the locks in frozen/: an export that differs from its lock fails before any
# proof. The locks hold the sha256 of each manifest and exported module; PINS names the exporter
# commit, toolchain and solvers they were taken with (the manifest records the solver versions, so
# another solver version changes the manifest's hash).
#
#   crates/verify/lean/freeze.sh SOLAR_VERIFY DIR [TIMEOUT_SECONDS]
set -euo pipefail
bin=$(realpath "$1")
dir=$2
limit=${3:-600}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
mkdir -p "$dir"
dir=$(cd "$dir" && pwd)
status=0
for c in Counter Token Vault ArrayMax; do
  rm -rf "$root/vc-exports/$c"
  (cd "$root" && "$bin" --emit-lean "vc-exports/$c" "crates/verify/examples/$c.sol" </dev/null >/dev/null) || true
  rm -rf "$dir/$c"
  mv "$root/vc-exports/$c" "$dir/$c"
  DISCHARGE_LOCK="$here/frozen/$c.sha256" "$here/discharge.sh" "$dir/$c" "$limit" || status=1
done
rmdir "$root/vc-exports" 2>/dev/null || true
exit $status
