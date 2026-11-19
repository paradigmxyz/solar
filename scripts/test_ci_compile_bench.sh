#!/usr/bin/env bash
# Standalone regression test. Run from a checkout with Bash, Python 3 and bc.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export GIT_DIR
GIT_DIR=$(git -C "$script_dir" rev-parse --absolute-git-dir)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
mkdir -p scripts bin target/release testdata/repros
cp "$script_dir/ci_compile_bench.sh" scripts/
export PATH="$tmp/bin:$PATH"

cat > bin/cargo <<'STUB'
#!/usr/bin/env bash
echo build >> cargo.calls
exit "${CARGO_STATUS:-0}"
STUB
cat > target/release/solar <<'STUB'
#!/usr/bin/env bash
echo "compiler output"
echo "compiler warning" >&2
case "${FAIL_PHASE:-}:${2:-}" in
    parse:--stop-after=parsing|full:) exit 42 ;;
esac
STUB
chmod +x bin/cargo target/release/solar

# Capturing a script's nonzero exit must not also accept an unexpected success.
run() {
    local expected=$1 status=0
    shift
    bash scripts/ci_compile_bench.sh "$@" >out 2>err || status=$?
    if (( status != expected )); then
        cat err >&2
        echo "expected exit $expected, got $status" >&2
        exit 1
    fi
}

run 1
[[ ! -s out ]]
printf 'contract C {}\n' >testdata/repros/c_small.sol
run 0
python3 -c 'import json; assert [r["name"] for r in json.load(open("out"))["results"]] == ["c_small"]'
[[ $(grep -c 'compiler warning' err) == 6 ]]
! grep -q 'compiler output' err || exit 1

for phase in parse full; do
    FAIL_PHASE=$phase run 42
    [[ ! -s out && ! -s target/compile-bench/results.json ]]
done

printf '%s\n' '{"commit_short":"a","results":[{"name":"first","parse_ms":1,"sema_ms":1}]}' >base.json
printf '%s\n' '{"commit_short":"b","results":[{"name":"other","parse_ms":1,"sema_ms":1}]}' >curr.json
calls=$(wc -l <cargo.calls)
export CARGO_STATUS=77
run 1 --baseline base.json --compare curr.json --output-dir compare-only
[[ ! -s out ]]
grep -q 'No benchmark cases in common' err
python3 - <<'PY'
import json
with open("curr.json") as f:
    current = json.load(f)
with open("base.json") as f:
    current["results"] += json.load(f)["results"]
with open("curr.json", "w") as f:
    json.dump(current, f)
PY
run 0 --baseline base.json --compare curr.json --output-dir compare-only
python3 -c 'import json; assert [r["name"] for r in json.load(open("out"))["all_results"]] == ["first"]'
[[ $(wc -l <cargo.calls) == "$calls" && ! -e compare-only ]]
echo 'PASS: compile benchmark collection and comparison'
