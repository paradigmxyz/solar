#!/bin/bash
# Benchmark compilation time: Solar vs solc

SOLAR_BIN="/Users/georgios/github/paradigmxyz/solar/.worktrees/bench-compile-time/target/release/solar"
TESTDATA_DIR="/Users/georgios/github/paradigmxyz/solar/.worktrees/bench-compile-time/crates/codegen/testdata"
RESULTS_FILE="/Users/georgios/github/paradigmxyz/solar/.worktrees/bench-compile-time/compile-time-results.md"

# Arrays to store results
declare -a names
declare -a solc_times
declare -a solar_times

measure_time() {
    local start=$(python3 -c 'import time; print(time.time())')
    eval "$1" >/dev/null 2>&1
    local end=$(python3 -c 'import time; print(time.time())')
    python3 -c "print(f'{$end - $start:.3f}')"
}

echo "Benchmarking compilation times..."
echo ""

for dir in "$TESTDATA_DIR"/*/; do
    if [ -f "$dir/foundry.toml" ]; then
        name=$(basename "$dir")
        echo "Testing: $name"
        
        cd "$dir"
        
        # Clean
        rm -rf out cache
        
        # Time solc (3 runs, take average)
        solc_total=0
        for i in 1 2 3; do
            rm -rf out cache
            t=$(measure_time "forge build --use solc --force")
            solc_total=$(python3 -c "print($solc_total + $t)")
        done
        solc_avg=$(python3 -c "print(f'{$solc_total / 3:.3f}')")
        
        # Time solar (3 runs, take average)
        solar_total=0
        for i in 1 2 3; do
            rm -rf out cache
            t=$(measure_time "forge build --use $SOLAR_BIN --force")
            solar_total=$(python3 -c "print($solar_total + $t)")
        done
        solar_avg=$(python3 -c "print(f'{$solar_total / 3:.3f}')")
        
        names+=("$name")
        solc_times+=("$solc_avg")
        solar_times+=("$solar_avg")
        
        # Clean up
        rm -rf out cache
        
        cd "$TESTDATA_DIR/.."
    fi
done

# Generate markdown table
echo "# Compilation Time Benchmark: Solar vs solc" > "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "Average of 3 runs per contract." >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "| Contract | solc (s) | Solar (s) | Speedup |" >> "$RESULTS_FILE"
echo "|----------|----------|-----------|---------|" >> "$RESULTS_FILE"

total_solc=0
total_solar=0

for i in "${!names[@]}"; do
    name="${names[$i]}"
    solc="${solc_times[$i]}"
    solar="${solar_times[$i]}"
    speedup=$(python3 -c "print(f'{$solc / $solar:.1f}x')")
    printf "| %-16s | %8s | %9s | %7s |\n" "$name" "$solc" "$solar" "$speedup" >> "$RESULTS_FILE"
    total_solc=$(python3 -c "print($total_solc + $solc)")
    total_solar=$(python3 -c "print($total_solar + $solar)")
done

total_speedup=$(python3 -c "print(f'{$total_solc / $total_solar:.1f}x')")
echo "|----------|----------|-----------|---------|" >> "$RESULTS_FILE"
printf "| %-16s | %8.3f | %9.3f | %7s |\n" "**TOTAL**" "$total_solc" "$total_solar" "$total_speedup" >> "$RESULTS_FILE"

echo ""
echo "Results saved to: $RESULTS_FILE"
cat "$RESULTS_FILE"
