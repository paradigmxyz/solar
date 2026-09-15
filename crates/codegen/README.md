# solar-codegen

Solidity MIR (Mid-level Intermediate Representation) and EVM code generation for Solar.

## Architecture

```text
HIR (from solar-sema) -> Lowering -> MIR -> Code Generation -> EVM Bytecode
```

### MIR Structure

- **Module**: Top-level container with functions, data segments, and storage layout
- **Function**: SSA-form functions with basic blocks, values, and instructions
- **BasicBlock**: Sequence of instructions ending with a terminator
- **Instruction**: Operations (arithmetic, memory, storage, control flow)
- **Value**: SSA values (instruction results, arguments, immediates, phi nodes)

### Key Types

- `ValueId`, `InstId`, `BlockId`, `FunctionId`: Index types for SSA values
- `MirType`: Types used in MIR (UInt, Address, MemPtr, StoragePtr)
- `InstKind`: Instruction variants (Add, Sub, SLoad, SStore, Call, etc.)
- `Terminator`: Block terminators (Jump, Branch, Return, Revert)

## Optional backends

`--codegen-backend` selects `evm` (the default), `yul`, `sonatina`, `sir`, or
`llvm`. The CLI enables the four optional Cargo features by default:

| Backend | Cargo feature | Toolchain |
| --- | --- | --- |
| Yul | `codegen-yul` | `solc`, or the executable named by `SOLAR_SOLC` |
| Sonatina | `codegen-sonatina` | Statically linked Sonatina Rust libraries |
| Sensei IR | `codegen-sir` | Statically linked Plank Rust libraries |
| LLVM IR | `codegen-llvm` | Statically linked solx EVM LLVM through its Inkwell bindings |

The library enables none of these features by default. To build the CLI without
LLVM, disable defaults and select the desired features explicitly, for example:

```sh
cargo build -p solar-compiler --no-default-features \
  --features cli,mimalloc,tracing,codegen-yul,codegen-sonatina,codegen-sir
```

LLVM builds require the EVM-enabled LLVM 21 fork, including LLD. Stock LLVM does
not provide this target. Set `LLVM_SYS_211_PREFIX` to its build or installation
directory before running Cargo. The native build used for this integration is
[NomicFoundation/solx-llvm at 9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a](https://github.com/NomicFoundation/solx-llvm/tree/9cf8cfdbfcdc3e74dd81f7cc0e7258ef81e8810a).
For an existing checkout of that revision:

```sh
cmake -S /path/to/solx-llvm/llvm -B target/llvm-evm \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
  -DLLVM_TARGETS_TO_BUILD= -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=EVM \
  -DLLVM_DEFAULT_TARGET_TRIPLE=evm -DLLVM_ENABLE_PROJECTS=lld \
  -DLLVM_ENABLE_RTTI=ON -DLLVM_INCLUDE_TESTS=OFF \
  -DLLVM_INCLUDE_BENCHMARKS=OFF -DLLVM_ENABLE_ZLIB=OFF \
  -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF
cmake --build target/llvm-evm --parallel
export LLVM_SYS_211_PREFIX="$PWD/target/llvm-evm"
cargo build -p solar-compiler --bin solar
```

Sonatina, SIR, and LLVM currently require Osaka. All four adapters lower data
sections, child-contract creation, constructor arguments, typed immutables,
internal frames, multi-value calls, memory copies, external calls, logs, and
ordinary environment operations. Opaque metadata stays at the runtime's end.
Yul and LLVM use native immutable relocations; Sonatina and SIR use trailing
immutable words patched during deployment and loaded with `CODECOPY`.

Use `-Zdump=backend-ir` to inspect the converted IR. Sonatina, SIR, and LLVM dumps
include separate runtime and deployment modules. SIR embeds the compiled runtime
as a deployment data segment. The built-in backend alone provides source maps
and EVM IR dumps. Unsupported operations produce diagnostics without switching
backends.

These adapters are still experimental. SIR's upstream compiler rejects recursive
calls and does not expose `MSIZE`. Yul can exceed solc's stack limit; arbitrary
Solidity memory access prevents us from adding a blanket `memoryguard` promise.
Internal activation frames use the free-memory pointer and remain allocated so
escaping references stay valid. This can cost more memory and gas than the
built-in backend's frame planning. Native stack scheduling can still reject
larger contracts.

Yul uses solc's process interface; it does not link C++ libsolc into the CLI.
Sonatina and SIR use their own native optimization and stack-scheduling pipelines.
SIR uses the same upstream O2 pipeline for gas and size because it has no distinct
size preset. Sonatina and SIR reject native stack spills until they can reserve
memory without clobbering Solidity memory. LLVM uses the EVM LLVM target and
linker in a child of the same statically linked CLI; fatal native failures become
diagnostics.
Library hosts using LLVM must call `backend::llvm::initialize_cli_worker` at
startup and return its optional exit code. LLVM stack spills also remain
unsupported. The musl release explicitly omits LLVM because its native libraries
need a matching musl C++ toolchain; the other release targets include it.

See [the benchmark guide](../../benches/runtime/README.md) for backend selection
and comparison commands.
