#!/usr/bin/env bash
# CI compile-time benchmark script with JSON output for regression detection.
#
# Usage:
#   ./scripts/ci_compile_bench.sh [--baseline FILE] [--compare FILE] [--threshold PCT]
#
# Outputs JSON results to stdout or compares against baseline.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$REPO_ROOT"

BASELINE_FILE=""
COMPARE_FILE=""
THRESHOLD=5  # Default 5% regression threshold
OUTPUT_DIR="${REPO_ROOT}/target/compile-bench"

while [[ $# -gt 0 ]]; do
    case $1 in
        --baseline)
            BASELINE_FILE="$2"
            shift 2
            ;;
        --compare)
            COMPARE_FILE="$2"
            shift 2
            ;;
        --threshold)
            THRESHOLD="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

mkdir -p "$OUTPUT_DIR"

REPROS_DIR="testdata/repros"
SIZES=("small" "medium")  # Skip large in CI for speed

# Build release binary
echo "::group::Building release binary" >&2
cargo build --release --quiet 2>/dev/null
echo "::endgroup::" >&2

# Collect benchmark results
collect_benchmarks() {
    local results=()
    
    for size in "${SIZES[@]}"; do
        for repro in "$REPROS_DIR"/*_${size}.sol; do
            [[ -f "$repro" ]] || continue
            
            local name
            name=$(basename "$repro" .sol)
            local lines bytes
            lines=$(wc -l < "$repro" | tr -d ' ')
            bytes=$(wc -c < "$repro" | tr -d ' ')
            
            # Run multiple iterations for stability
            local parse_times=()
            local sema_times=()
            
            for _ in {1..3}; do
                # Time parse phase
                local start end
                start=$(python3 -c 'import time; print(time.time())')
                ./target/release/solar "$repro" --stop-after=parsing 2>&1 >/dev/null || true
                end=$(python3 -c 'import time; print(time.time())')
                parse_times+=("$(echo "scale=6; ($end - $start) * 1000" | bc)")
                
                # Time full compile
                start=$(python3 -c 'import time; print(time.time())')
                ./target/release/solar "$repro" 2>&1 >/dev/null || true
                end=$(python3 -c 'import time; print(time.time())')
                sema_times+=("$(echo "scale=6; ($end - $start) * 1000" | bc)")
            done
            
            # Calculate median
            local parse_median sema_median
            parse_median=$(printf '%s\n' "${parse_times[@]}" | sort -n | sed -n '2p')
            sema_median=$(printf '%s\n' "${sema_times[@]}" | sort -n | sed -n '2p')
            
            results+=("{\"name\":\"$name\",\"lines\":$lines,\"bytes\":$bytes,\"parse_ms\":$parse_median,\"sema_ms\":$sema_median}")
        done
    done
    
    # Output JSON
    echo "{"
    echo "  \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\","
    echo "  \"commit\": \"$(git rev-parse HEAD)\","
    echo "  \"commit_short\": \"$(git rev-parse --short HEAD)\","
    echo "  \"branch\": \"$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'detached')\","
    echo "  \"results\": ["
    local first=true
    for r in "${results[@]}"; do
        if $first; then
            first=false
        else
            echo ","
        fi
        echo -n "    $r"
    done
    echo ""
    echo "  ]"
    echo "}"
}

# Compare two benchmark files
compare_benchmarks() {
    local baseline="$1"
    local current="$2"
    local threshold="$3"
    
    python3 - "$baseline" "$current" "$threshold" << 'PYTHON'
import json
import sys

baseline_file = sys.argv[1]
current_file = sys.argv[2]
threshold = float(sys.argv[3])

with open(baseline_file) as f:
    baseline = json.load(f)
with open(current_file) as f:
    current = json.load(f)

baseline_map = {r["name"]: r for r in baseline["results"]}
current_map = {r["name"]: r for r in current["results"]}

regressions = []
improvements = []
all_results = []

for name, curr in current_map.items():
    if name not in baseline_map:
        continue
    base = baseline_map[name]
    
    parse_diff = ((curr["parse_ms"] - base["parse_ms"]) / base["parse_ms"]) * 100 if base["parse_ms"] > 0 else 0
    sema_diff = ((curr["sema_ms"] - base["sema_ms"]) / base["sema_ms"]) * 100 if base["sema_ms"] > 0 else 0
    
    result = {
        "name": name,
        "parse_base": base["parse_ms"],
        "parse_curr": curr["parse_ms"],
        "parse_diff_pct": parse_diff,
        "sema_base": base["sema_ms"],
        "sema_curr": curr["sema_ms"],
        "sema_diff_pct": sema_diff,
    }
    all_results.append(result)
    
    if parse_diff > threshold or sema_diff > threshold:
        regressions.append(result)
    elif parse_diff < -threshold or sema_diff < -threshold:
        improvements.append(result)

output = {
    "baseline_commit": baseline["commit_short"],
    "current_commit": current["commit_short"],
    "threshold_pct": threshold,
    "regressions": regressions,
    "improvements": improvements,
    "all_results": all_results,
    "has_regression": len(regressions) > 0,
}

print(json.dumps(output, indent=2))
sys.exit(1 if regressions else 0)
PYTHON
}

# Generate markdown report
generate_report() {
    local results_file="$1"
    local comparison_file="${2:-}"
    
    python3 - "$results_file" "$comparison_file" << 'PYTHON'
import json
import sys

results_file = sys.argv[1]
comparison_file = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] else None

with open(results_file) as f:
    results = json.load(f)

print("## ⏱️ Compile-Time Benchmark Results")
print()
print(f"**Commit:** `{results['commit_short']}`")
print(f"**Branch:** `{results['branch']}`")
print(f"**Timestamp:** {results['timestamp']}")
print()

if comparison_file:
    with open(comparison_file) as f:
        comparison = json.load(f)
    
    print(f"### Comparison vs `{comparison['baseline_commit']}`")
    print()
    
    if comparison["regressions"]:
        print("#### ⚠️ Regressions Detected")
        print()
        print("| Test | Parse (ms) | Δ | Sema (ms) | Δ |")
        print("|------|-----------|---|----------|---|")
        for r in comparison["regressions"]:
            parse_emoji = "🔴" if r["parse_diff_pct"] > comparison["threshold_pct"] else ""
            sema_emoji = "🔴" if r["sema_diff_pct"] > comparison["threshold_pct"] else ""
            print(f"| {r['name']} | {r['parse_curr']:.2f} | {parse_emoji} +{r['parse_diff_pct']:.1f}% | {r['sema_curr']:.2f} | {sema_emoji} +{r['sema_diff_pct']:.1f}% |")
        print()
    
    if comparison["improvements"]:
        print("#### ✅ Improvements")
        print()
        print("| Test | Parse (ms) | Δ | Sema (ms) | Δ |")
        print("|------|-----------|---|----------|---|")
        for r in comparison["improvements"]:
            print(f"| {r['name']} | {r['parse_curr']:.2f} | {r['parse_diff_pct']:+.1f}% | {r['sema_curr']:.2f} | {r['sema_diff_pct']:+.1f}% |")
        print()

print("### Full Results")
print()
print("| Test | Lines | Parse (ms) | Sema (ms) | KB/s |")
print("|------|-------|-----------|----------|------|")
for r in results["results"]:
    kbps = (r["bytes"] / 1024) / (r["sema_ms"] / 1000) if r["sema_ms"] > 0 else 0
    print(f"| {r['name']} | {r['lines']} | {r['parse_ms']:.2f} | {r['sema_ms']:.2f} | {kbps:.0f} |")

print()
print("<details>")
print("<summary>📊 Raw JSON</summary>")
print()
print("```json")
print(json.dumps(results, indent=2))
print("```")
print("</details>")
PYTHON
}

# Main execution
if [[ -n "$COMPARE_FILE" && -n "$BASELINE_FILE" ]]; then
    # Compare mode
    compare_benchmarks "$BASELINE_FILE" "$COMPARE_FILE" "$THRESHOLD"
elif [[ -n "$BASELINE_FILE" ]]; then
    # Generate report with comparison
    RESULTS_FILE="$OUTPUT_DIR/current.json"
    collect_benchmarks > "$RESULTS_FILE"
    
    COMPARISON_FILE="$OUTPUT_DIR/comparison.json"
    compare_benchmarks "$BASELINE_FILE" "$RESULTS_FILE" "$THRESHOLD" > "$COMPARISON_FILE" || true
    
    generate_report "$RESULTS_FILE" "$COMPARISON_FILE"
else
    # Just collect and output results
    RESULTS_FILE="$OUTPUT_DIR/results.json"
    collect_benchmarks > "$RESULTS_FILE"
    
    if [[ -t 1 ]]; then
        # Terminal output - generate report
        generate_report "$RESULTS_FILE"
    else
        # Pipe output - raw JSON
        cat "$RESULTS_FILE"
    fi
fi
