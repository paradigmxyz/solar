#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/run_erwin.sh [--mutate <file>]

# https://github.com/haoyang9804/Erwin/

solar=${solar:-solar}
npx=${npx:-bunx}

# Parse arguments
mutate_file=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --mutate)
            mutate_file="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

mkdir -p erwin

if [[ -n "$mutate_file" ]]; then
    "$npx" @__haoyang__/erwin mutate -f "$mutate_file" -o erwin/generated_programs
fi

while true; do
    "$npx" @__haoyang__/erwin generate \
        --target solar \
        --compiler_path "$solar" \
        --refresh_folder \
        --generation_rounds 1000 \
        --enable_test \
        -o erwin \
        -max 1
done
