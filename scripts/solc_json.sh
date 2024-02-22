#!/usr/bin/env bash
# Use standard JSON to compile a Solidity file with Solc.

set -eo pipefail

dir="$(dirname "$0")"
template_path="$dir/standard_json_template.json"
prelude="// SPDX-License-Identifier: UNLICENSED
pragma solidity 0;

"

file="$1"
if [ -z "$file" ] || [ "$file" == '-' ]; then
    file="stdin.sol"
    source="$(cat)"
else
    source="$(cat "$file")"
fi

[[ "$source" != *SPDX-License-Identifier* ]] && source="$prelude$source"
# echo "$source" >&2

FILE_NAME=$file SOURCE_CODE=$source envsubst < "$template_path" \
    | solc --standard-json
