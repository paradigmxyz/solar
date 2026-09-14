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

Sonatina, SIR, and LLVM currently require Osaka. These initial adapters support
word operations and CFGs, including lazy ABI arguments, phis, and internal calls.
They reject unsupported operations and relocations instead of switching backends.
Live compiler frames, embedded MIR data, immutable references, and library
relocations still need adapters. Multi-value calls are also incomplete. Use `-Zdump=backend-ir` to inspect the generated target IR. Source
maps and EVM IR dumps are available only from the built-in backend.

Yul uses solc's process interface; it does not link C++ libsolc into the CLI.
Sonatina and SIR use their own native optimization and stack-scheduling pipelines.
SIR uses the same upstream O2 pipeline for gas and size because it has no distinct
size preset. SIR rejects native stack spills until it can reserve memory without
clobbering Solidity memory. LLVM uses the EVM LLVM target and linker in a child
of the same statically linked CLI; fatal native failures become diagnostics.
Library hosts using LLVM must call `backend::llvm::initialize_cli_worker` at
startup and return its optional exit code. LLVM stack spills also remain
unsupported. The musl release explicitly omits LLVM because its native libraries
need a matching musl C++ toolchain; the other release targets include it.

See [the benchmark guide](../../benches/runtime/README.md) for backend selection
and comparison commands.
