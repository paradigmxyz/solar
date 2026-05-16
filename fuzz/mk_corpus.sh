#!/usr/bin/env bash
set -euo pipefail

# Check arguments
if [ $# -lt 2 ]; then
    echo "Usage: $0 <input_dir> [input_dirs...] <output_dir>" >&2
    exit 1
fi

# Get output directory (last argument)
output_dir="${@: -1}"

# Create output directory if it doesn't exist
mkdir -p "$output_dir"

# Get absolute path for output directory
output_dir="$(cd "$output_dir" && pwd)"

# Remove last argument to get input directories
set -- "${@:1:$(($#-1))}"

total_files=0

# Process each input directory
for input_dir in "$@"; do
    # Validate input directory exists
    if [ ! -d "$input_dir" ]; then
        echo "Warning: Input '$input_dir' is not a directory or does not exist, skipping" >&2
        continue
    fi

    # Get absolute path
    input_dir="$(cd "$input_dir" && pwd)"
    echo "checking $input_dir"

    # Extract directory name for prefix
    dir_name="$(basename "$input_dir")"

    # Find all .sol files and create symlinks
    while IFS= read -r sol_file; do
        # Get relative path from input_dir
        rel_path="${sol_file#$input_dir/}"

        # Convert path to flat filename: replace / with _
        flat_name="${rel_path//\//_}"

        # Add directory prefix to avoid conflicts
        prefixed_name="${dir_name}_${flat_name}"

        # Create symlink in output directory
        ln -sf "$sol_file" "$output_dir/$prefixed_name"

        echo "Linked: $input_dir/$rel_path -> $prefixed_name"
        total_files=$((total_files + 1))
    done < <(find "$input_dir" -type f -name "*.sol")
done

echo "Done. Created $total_files symlinks in $output_dir"
