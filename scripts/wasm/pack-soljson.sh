#!/usr/bin/env bash
set -euo pipefail

(( $# == 3 )) || { >&2 echo "Usage: $0 solar.wasm soljson-wrapper.js packed-soljson.js"; exit 1; }

wasm="$1"
wrapper="$2"
output="$3"

if base64 --help 2>&1 | grep -q -- "-w"; then
    wasm_base64="$(base64 -w0 "$wasm")"
else
    wasm_base64="$(base64 < "$wasm" | tr -d '\n')"
fi

{
    cat <<'JS'
var Module = typeof Module === "object" ? Module : {};
Module["wasmBinary"] = (function (source) {
  if (typeof Buffer === "function") {
    return Uint8Array.from(Buffer.from(source, "base64"));
  }
  var binary = atob(source);
  var bytes = new Uint8Array(binary.length);
  for (var i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
})(
JS
    printf '"%s");\n' "$wasm_base64"
    cat "$wrapper"
} > "$output"
