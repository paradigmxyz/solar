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
for v in SOLAR CERTIFY_SKIP_BUILD SOLAR_VERIFY CERTIFY_REPLAY_TRUST_UPSTREAM CERTIFY_ALLOW_FOREIGN_SOLAR \
    CERTIFY_ALLOW_UNPINNED CERTIFY_PIN_BASE; do
    if [[ -n "${!v:-}" ]]; then echo "producer: $v is set; refusing" >&2; exit 2; fi
done
# The build's configuration must be the one the driver supports (generic.py cargo_env_problem and
# cargo_config_problem): no rustc override, wrapper or flags, no other target or profile, and no
# Cargo config outside the checkout's own. Refused here before anything builds.
for v in RUSTC RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTC_BOOTSTRAP \
    RUSTC_FORCE_INCREMENTAL CARGO_BUILD_TARGET; do
    if [[ -n "${!v+x}" ]]; then echo "producer: $v is set; refusing" >&2; exit 2; fi
done
while IFS='=' read -r k _; do
    case "$k" in
        CARGO_BUILD_JOBS|CARGO_INCREMENTAL|CARGO_HOME|CARGO_NET_OFFLINE|CARGO_TERM_*) ;;
        CARGO_BUILD_*|CARGO_TARGET_*|CARGO_PROFILE_*|CARGO_UNSTABLE_*|CARGO_ENCODED_*|__CARGO_*)
            echo "producer: $k is set; refusing" >&2; exit 2 ;;
    esac
done < <(env)
d=$(cd "$SOLAR_DIR/.." && pwd)
while :; do
    for f in "$d/.cargo/config" "$d/.cargo/config.toml"; do
        [[ -e "$f" ]] && { echo "producer: external Cargo config $f; refusing" >&2; exit 2; }
    done
    [[ "$d" == / ]] && break
    d=$(dirname "$d")
done
for f in "${CARGO_HOME:-$HOME/.cargo}/config" "${CARGO_HOME:-$HOME/.cargo}/config.toml"; do
    [[ -e "$f" ]] && { echo "producer: external Cargo config $f; refusing" >&2; exit 2; }
done
cd "$SOLAR_DIR"
# A clean build: no target directory before it.
[[ ! -e target ]] || { echo "producer: $SOLAR_DIR/target exists; the build must start empty" >&2; exit 2; }
grep -qx 'PROVENANCE_VERSION = 2' tools/certify/generic.py \
    || { echo "producer: the driver is not at build provenance version 2" >&2; exit 2; }
head=$(git rev-parse HEAD)
[[ "$head" == "$SOLAR_SHA" ]] || { echo "producer: checkout is at $head, not $SOLAR_SHA" >&2; exit 2; }
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || { echo "producer: checkout has edits" >&2; exit 2; }
cmd=(cargo build --locked --bin solar)
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" "${cmd[@]}" >&2
bin="$SOLAR_DIR/${CARGO_TARGET_DIR:-target}/debug/solar"
[[ -x "$bin" ]] || { echo "producer: no binary at $bin" >&2; exit 2; }
harness=$(git -C "$(dirname "$0")" rev-parse HEAD 2>/dev/null || echo unknown)
python3 - "$out" "$bin" "$head" "${cmd[*]}" "$harness" <<'EOF'
import hashlib, json, os, re, subprocess, sys, datetime
out, binp, head, cmd, harness = sys.argv[1:6]
sysroot = subprocess.run(["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True).stdout.strip()


def tree_sha256(root):
    """sha256 over "<sha256>  <relative path>" lines of every regular file under ROOT, sorted by path
    (symlinks as "link -> target")."""
    lines = []
    for dp, dns, fns in os.walk(root):
        dns.sort()
        for f in sorted(fns):
            p = os.path.join(dp, f); rel = os.path.relpath(p, root)
            if os.path.islink(p):
                lines.append(f"link {os.readlink(p)}  {rel}")
            elif os.path.isfile(p):
                h = hashlib.sha256()
                with open(p, "rb") as fh:
                    for chunk in iter(lambda: fh.read(1 << 20), b""):
                        h.update(chunk)
                lines.append(f"{h.hexdigest()}  {rel}")
    nl = chr(10)
    return hashlib.sha256((nl.join(sorted(lines, key=lambda l: l.split("  ", 1)[1])) + nl).encode()).hexdigest()
ver = subprocess.run([binp, "--version"], capture_output=True, text=True).stdout
m = re.search(r"Commit SHA: ([0-9a-f]{40})", ver)
gen = open("tools/certify/generic.py").read()
def const(name):
    m2 = re.search(rf"^{name} = (.+)$", gen, re.M)
    return m2.group(1).strip() if m2 else None
rec = {
    "solar_commit": head,
    "driver_commit": head,
    "harness_commit": harness,
    "escape_hatches": {v: os.environ.get(v, "unset") for v in ("CERTIFY_ALLOW_UNPINNED", "CERTIFY_ALLOW_FOREIGN_SOLAR")},
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
    "cargo_lock_sha256": hashlib.sha256(open(os.path.join(os.environ["SOLAR_DIR"], "Cargo.lock"), "rb").read()).hexdigest(),
    "toolchain_dir": sysroot,
    "toolchain_dir_sha256": tree_sha256(sysroot),
    "recorded": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
}
if rec["binary_commit"] != head:
    sys.exit(f"producer: the binary embeds commit {rec['binary_commit']}, not {head}")
json.dump(rec, open(out, "w"), indent=2)
print(json.dumps(rec, indent=2))
EOF
