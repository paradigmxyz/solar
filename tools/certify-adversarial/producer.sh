#!/usr/bin/env bash
# producer.sh OUT_JSON: builds the solar binary the driver will use and records it.
#
# The driver (tools/certify/generic.py) builds solar itself with "cargo build --locked --bin solar"
# in its own checkout and records the binary's sha256 in record.json (solar.binary_sha256). This
# script runs the same build first, in the same checkout, so that every case can be checked
# against one recorded producer: commit, binary path, sha256, build command and the codegen flags
# the driver passes. Nothing falls back to a cached stage or binary: the checkout must be clean at
# SOLAR_SHA, and no SOLAR, CERTIFY_SKIP_BUILD or SOLAR_VERIFY override may be set.
set -euo pipefail
out=$1
: "${SOLAR_DIR:?}" "${SOLAR_SHA:?}"
for v in SOLAR CERTIFY_SKIP_BUILD SOLAR_VERIFY CERTIFY_REPLAY_TRUST_UPSTREAM CERTIFY_ALLOW_FOREIGN_SOLAR; do
    if [[ -n "${!v:-}" ]]; then echo "producer: $v is set; refusing" >&2; exit 2; fi
done
cd "$SOLAR_DIR"
head=$(git rev-parse HEAD)
[[ "$head" == "$SOLAR_SHA" ]] || { echo "producer: checkout is at $head, not $SOLAR_SHA" >&2; exit 2; }
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || { echo "producer: checkout has edits" >&2; exit 2; }
cmd=(cargo build --locked --bin solar)
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" "${cmd[@]}" >&2
bin="$SOLAR_DIR/${CARGO_TARGET_DIR:-target}/debug/solar"
[[ -x "$bin" ]] || { echo "producer: no binary at $bin" >&2; exit 2; }
python3 - "$out" "$bin" "$head" "${cmd[*]}" <<'EOF'
import hashlib, json, re, subprocess, sys, datetime
out, binp, head, cmd = sys.argv[1:5]
ver = subprocess.run([binp, "--version"], capture_output=True, text=True).stdout
m = re.search(r"Commit SHA: ([0-9a-f]{40})", ver)
gen = open("tools/certify/generic.py").read()
def const(name):
    m2 = re.search(rf"^{name} = (.+)$", gen, re.M)
    return m2.group(1).strip() if m2 else None
rec = {
    "solar_commit": head,
    "binary_path": binp,
    "binary_sha256": hashlib.sha256(open(binp, "rb").read()).hexdigest(),
    "binary_commit": m.group(1) if m else None,
    "version": ver.strip().splitlines()[0] if ver.strip() else "",
    "build_command": cmd,
    "profile": "debug",
    "driver_options": const("OPTIONS"),
    "driver_environment": const("ENVIRONMENT"),
    "driver_lean_memory": const("LEAN_MEMORY"),
    "stage_cache": None,
    "rustc": subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip(),
    "recorded": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
}
if rec["binary_commit"] != head:
    sys.exit(f"producer: the binary embeds commit {rec['binary_commit']}, not {head}")
json.dump(rec, open(out, "w"), indent=2)
print(json.dumps(rec, indent=2))
EOF
