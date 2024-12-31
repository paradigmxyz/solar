#!/usr/bin/env bash
# Build a combined JSON file for a Solidity file using Solc.

set -eo pipefail

file="$1"
[ -z "$file" ] && echo "Usage: $0 <file>" && exit 1
shift
file_basename="$(basename "$file" .sol)"
contract="${2:-C}"

solc --combined-json generated-sources-runtime "$file" --optimize \
| jq . \
| tee "$file_basename.json" \
| jq -r ".contracts[\"$file_basename:$contract\"][\"generated-sources-runtime\"][0].contents" \
| tee "$file_basename.rt.yul"
