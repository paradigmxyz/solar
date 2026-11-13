#!/usr/bin/env bash
# Certifies one contract's backend: compiles it, exports its stages, writes their kernel proofs
# and checks them, with nothing written by hand for the contract.
#
#   solar-tv/milestone/certify.sh FILE.sol CONTRACT [--name NAME] [--loops N] [--exclude SIG]...
#       [-- SOLAR_OPTIONS...]
#
# NAME (default: FILE's base name, lowercase letters, digits and underscores) is the directory
# solar-tv/milestone/NAME, the Lean library NAME and the namespace SolarTV.Milestone.<Name>.
# SOLAR_OPTIONS default to -Ogas --evm-version osaka and are kept in NAME/options. --loops N
# (kept in NAME/loops) claims that a run's loops make at most N iterations in all, for a contract
# whose loops run a fixed number of times; the first stage's end proof checks it. --exclude SIG
# (kept in NAME/exclude, one signature a line) leaves the external function SIG, such as
# bump(uint256), out of every theorem: each assumes the call's selector word is not SIG's, and
# the stages and their composition hold on that scope only. It is for a function outside the
# fragment the proofs cover, such as one whose loop runs a number of times the calldata picks,
# which otherwise fails its proof; the theorems state the exclusion.
#
# 1. Copies FILE to NAME/NAME.sol (the source the manifest records), builds solar (unless
#    CERTIFY_SKIP_BUILD=1; SOLAR picks the binary), checks that its version names the
#    checkout's committed compiler sources, compiles with -Zemit-lean-stages=NAME/gen, and
#    checks that the flag leaves the runtime bytecode as it is, that it is NAME/bytecode.hex
#    when that pin exists (else writes it), and the manifest's hashes.
# 2. certify.py writes NAME/Proofs: per pair of consecutive stages a sym_exec proof that both
#    runs end in the same result on every call, the Stages composition from the first lowered
#    MIR to the final EVM IR, and the assembly walk of the bytecode against the final EVM IR.
# 3. Adds the library to lakefile.toml when missing, its Lean processes capped at LEAN_MEMORY
#    MB (default 4500), builds it one module at a time, and audits the axioms of every
#    theorem: only propext, Classical.choice and Quot.sound.
# 4. negative.sh NAME CONTRACT: a bytecode mutation and edited stage artifacts fail with a
#    mismatch.
# 5. When NAME is committed, the regenerated files must be the committed ones, except for the
#    compiler's commit in the manifest.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
tv="$root/solar-tv"
export LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-2}" CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
memory="${LEAN_MEMORY:-4500}"

die() { echo "error: $*" >&2; exit 1; }
[[ $# -ge 2 ]] || die "usage: certify.sh FILE.sol CONTRACT [--name NAME] [--loops N] [--exclude SIG]... [-- SOLAR_OPTIONS...]"
file="$(realpath "$1")"
contract=$2
shift 2
name="$(basename "$file" .sol | tr 'A-Z-' 'a-z_')"
options=()
loops=
excludes=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --name) name=$2; shift 2 ;;
        --loops) loops=$2; shift 2 ;;
        --exclude) excludes+=("$2"); shift 2 ;;
        --) shift; options=("$@"); break ;;
        *) die "unknown argument $1" ;;
    esac
done
[[ "$name" =~ ^[a-z][a-z0-9_]*$ ]] || die "the name $name is not lowercase letters, digits and _"
[[ "$name" != w1 && "$name" != certify ]] || die "the name $name is taken"
[[ "$contract" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || die "the contract $contract is not an identifier"
dir="solar-tv/milestone/$name"
cap="$(python3 -c 'import sys; n = sys.argv[1]; print("".join(p[:1].upper() + p[1:] for p in n.split("_")))' "$name")"
cd "$root"
mkdir -p "$dir"
source="$dir/$name.sol"
[[ "$file" == "$(realpath -m "$source")" ]] || cp "$file" "$source"
if [[ ${#options[@]} -eq 0 ]]; then
    if [[ -f "$dir/options" ]]; then mapfile -t options < "$dir/options"; else options=(-Ogas --evm-version osaka); fi
fi
printf '%s\n' "${options[@]}" > "$dir/options"
if [[ -n "$loops" ]]; then
    [[ "$loops" =~ ^[0-9]+$ ]] || die "--loops $loops is not a number"
    if [[ "$loops" -eq 0 ]]; then rm -f "$dir/loops"; else echo "$loops" > "$dir/loops"; fi
fi
loops="$(cat "$dir/loops" 2>/dev/null || echo 0)"
if [[ ${#excludes[@]} -gt 0 ]]; then printf '%s\n' "${excludes[@]}" > "$dir/exclude"; fi
excludes=()
[[ -f "$dir/exclude" ]] && mapfile -t excludes < "$dir/exclude"

echo "== 1. compile $source:$contract (${options[*]}) and export its stages"
target="${CARGO_TARGET_DIR:-$root/target}"
if [[ -z "${CERTIFY_SKIP_BUILD:-}" ]]; then
    cargo build -j "$CARGO_BUILD_JOBS" --bin solar 2>&1 | tail -n 1
fi
solar="${SOLAR:-$target/debug/solar}"
compile() { "$solar" "$source" "${options[@]}" --emit=bin-runtime "$@" 2>/dev/null; }
bin_runtime() {
    python3 -c 'import json, sys; c = json.load(sys.stdin)["contracts"]; print(c[sys.argv[1]]["bin-runtime"])' \
        "$source:$contract"
}
version="$(compile | python3 -c 'import json, sys; print(json.load(sys.stdin)["version"])')" ||
    die "solar does not compile $source"
commit="$(sed -n 's/.*+commit\.\([0-9a-f]\{8\}\).*/\1/p' <<<"$version")"
[[ -n "$commit" ]] || die "cannot read the compiler commit from its version $version"
git diff --quiet HEAD -- crates Cargo.toml Cargo.lock || die "the compiler sources have uncommitted changes"
git diff --quiet "$commit" HEAD -- crates Cargo.toml Cargo.lock ||
    die "the compiler reports commit $commit, whose sources differ from HEAD; touch crates/config/build.rs and rebuild"
plain="$(compile | bin_runtime)"
rm -rf "$dir/gen"
if ! exported="$("$solar" "$source" "${options[@]}" --emit=bin-runtime "-Zemit-lean-stages=$dir/gen" 2>"$dir/.export.log" | bin_runtime 2>/dev/null)"; then
    grep -E '^error' -A3 "$dir/.export.log" >&2 || cat "$dir/.export.log" >&2
    rm -f "$dir/.export.log"
    die "the exporter rejects $contract (above): outside the fragment the stages have Lean semantics for"
fi
rm -f "$dir/.export.log"
[[ "$plain" == "$exported" ]] || die "-Zemit-lean-stages changed the bytecode"
if [[ -f "$dir/bytecode.hex" ]]; then
    [[ "$plain" == "$(cat "$dir/bytecode.hex")" ]] || die "the bytecode is not the pinned $dir/bytecode.hex: $plain"
else
    echo "$plain" > "$dir/bytecode.hex"
fi
gen="$dir/gen/$contract"
python3 - "$gen" "$source" "$plain" <<'EOF'
import hashlib, json, pathlib, sys

directory, source, bytecode = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
manifest = json.loads((directory / "manifest.json").read_text())
sha = lambda path: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
assert [s["path"] for s in manifest["sources"]] == [source], manifest["sources"]
assert manifest["sources"][0]["sha256"] == sha(source), "source hash"
assert manifest["bytecode"] == bytecode, "manifest bytecode"
assert manifest["bytecode_sha256"] == hashlib.sha256(bytes.fromhex(bytecode)).hexdigest()
for stage in manifest["stages"]:
    assert stage["sha256"] == sha(directory / stage["file"]), stage["file"]
print("stages:", ", ".join(f"{s['definition']} ({s['pass']})" for s in manifest["stages"]))
print("bytecode:", bytecode, "sha256", manifest["bytecode_sha256"])
EOF

exclude_args=()
for sig in "${excludes[@]}"; do
    selector="$("$solar" "$source" "${options[@]}" --emit=hashes 2>/dev/null | python3 -c '
import json, sys
c = json.load(sys.stdin)["contracts"][sys.argv[1]]["hashes"]
print(c.get(sys.argv[2], ""))' "$source:$contract" "$sig")"
    [[ -n "$selector" ]] || die "--exclude $sig: $contract has no external function $sig"
    echo "excluded: $sig (selector $selector)"
    exclude_args+=(--exclude "$selector=$sig")
done

echo "== 2. write the proofs"
rm -rf "$dir/Proofs" "$dir/Proofs.lean"
info="$(python3 solar-tv/milestone/certify.py "$gen" --gen-module "$name.gen.$contract" \
    --proof-dir "$dir/Proofs" --proof-module "$name.Proofs" --namespace "SolarTV.Milestone.$cap" \
    --label "$name" --loops "$loops" "${exclude_args[@]}")"
final="$(python3 -c 'import json, sys; print(json.loads(sys.argv[1])["final"])' "$info")"
echo "$info"

echo "== 3. build and audit the proofs"
if ! grep -q "^name = \"$name\"$" solar-tv/lakefile.toml; then
    cat >> solar-tv/lakefile.toml <<EOF

# $name: the stages of milestone/$name/$name.sol's $contract and their kernel proofs, which
# milestone/certify.sh writes.
[[lean_lib]]
name = "$name"
srcDir = "milestone"
roots = ["$name.Proofs"]
globs = ["$name.+"]
moreLeanArgs = ["-M", "$memory"]
EOF
fi
cd "$tv"
# One module at a time, so that at most one Lean process runs: the exported stages first, which
# the proofs import, then the proofs.
mapfile -t modules < <(cd milestone && ls "$name"/Proofs/Runs/*.lean | sed 's|\.lean$||; s|/|.|g')
mapfile -t stages < <(cd milestone && ls "$name/gen/$contract"/*.lean | sed 's|\.lean$||; s|/|.|g')
for module in "${stages[@]}" "${modules[@]}" "$name.Proofs.Stages" "$name.Proofs.Assembly" "$name.Proofs"; do
    printf '%s: ' "$module"
    if ! lake build "$module" > "$tv/.lake/certify-build.log" 2>&1; then
        grep -E '^error' -A12 "$tv/.lake/certify-build.log" | head -60 >&2
        die "$module does not check"
    fi
    tail -n 1 "$tv/.lake/certify-build.log"
done
audit="$(mktemp --suffix=.lean)"
trap 'rm -f "$audit"' EXIT
{
    echo "import $name.Proofs"
    echo "#check @$final"
    echo "#print axioms $final"
    for module in "${modules[@]}"; do
        thm="$(sed -n 's/^theorem \([a-z0-9_]*\) .*/\1/p' "milestone/${module//.//}.lean" | head -1)"
        echo "#print axioms SolarTV.Milestone.$cap.$thm"
    done
    echo "#print axioms SolarTV.Milestone.$cap.walk_eq"
    echo "#print axioms SolarTV.Milestone.$cap.decodes"
} > "$audit"
report="$(lake env lean -M "$memory" "$audit" 2>&1)" || { echo "$report" >&2; die "the audit does not check"; }
echo "$report"
bad="$(grep "depends on axioms" <<<"$report" | sed -E 's/.*depends on axioms: \[(.*)\]/\1/' |
    tr ',' '\n' | tr -d ' ' | grep -v -x -E 'propext|Classical\.choice|Quot\.sound' || true)"
[[ -z "$bad" ]] || die "axioms beyond propext, Classical.choice and Quot.sound: $bad"
[[ "$(grep -c "depends on axioms" <<<"$report")" -eq $((${#modules[@]} + 3)) ]] || die "a theorem is missing from the audit"
cd "$root"

echo "== 4. negative tests"
solar-tv/milestone/negative.sh "$name" "$contract"

echo "== 5. the regenerated files are the committed ones"
if git ls-files --error-unmatch "$dir/bytecode.hex" > /dev/null 2>&1; then
    others="$(git status --porcelain -- "$dir" solar-tv/lakefile.toml | grep -v -E " $gen/(Manifest\.lean|manifest\.json)$" || true)"
    [[ -z "$others" ]] || { echo "$others" >&2; die "regenerated files differ from the committed ones"; }
    changed="$(git diff -U0 HEAD -- "$gen/Manifest.lean" "$gen/manifest.json" | grep '^[-+] ' | grep -v compiler || true)"
    [[ -z "$changed" ]] || { echo "$changed" >&2; die "the manifest differs in more than the compiler's commit"; }
    git checkout -q -- "$gen/Manifest.lean" "$gen/manifest.json"
    echo "every file of $dir regenerates to the committed one"
else
    echo "$dir is not committed yet"
fi
echo "$name: $final proved; bytecode $plain"
