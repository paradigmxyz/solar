#!/usr/bin/env bash
set -eo pipefail

v=${1:-22}
bins=(clang llvm-config lld ld.lld FileCheck)
llvm_sh=$(mktemp)

wget https://apt.llvm.org/llvm.sh -O "$llvm_sh"
chmod +x "$llvm_sh"
"$llvm_sh" "$v" all
for bin in "${bins[@]}"; do
    if ! command -v "$bin-$v" &>/dev/null; then
        echo "Warning: $bin-$v not found" 1>&2
        continue
    fi
    ln -fs "$(which "$bin-$v")" "/usr/bin/$bin"
done

echo "LLVM $v installed:"
llvm-config --version
