# Runtime Coverage Inventory

This tracks runtime execution coverage for codegen. UI tests are still the right
tool for diagnostics and formatted compiler output, but runtime checks should
prove that deployed bytecode from this compiler behaves like deployed solc
bytecode.

Correctness mismatches should fail. Gas and size differences should be reported,
but not used as a hard solc comparison gate.

## Current Runtime Entry Points

| Entry point | Scope | Notes |
| --- | --- | --- |
| `cargo nextest run --workspace` | Default Rust and integration tests | Includes the `solar-compiler::foundry` integration target when Foundry is available. |
| `cargo xtask test foundry` | Foundry mode through `crates/solar/tests.rs` | Runs the default Foundry suite through `TESTER_MODE=foundry`. |
| `cargo test -p solar-compiler --test foundry -- --test-threads=1` | Dedicated Foundry target | Useful for isolating Foundry runtime behavior locally. |
| `SOLAR_FOUNDRY_REPORT_DIR=target/runtime-reports cargo test -p solar-compiler --test foundry -- --test-threads=1` | Structured Foundry report output | Writes one JSON report per project with filters, per-test pass/gas, bytecode sizes, and rerun metadata. |
| `.github/workflows/bench.yml` codegen runtime benchmark | Informational repo/micro gas and size comparisons | Compares this compiler to solc and to the latest main artifact when available. It should not fail only because this compiler uses more gas than solc. |

## Foundry Project Inventory

The repository contains more Foundry projects than the default harness currently
runs. Test counts below count `function test*` declarations in `.t.sol` files.

| Project | Tests | Default harness | Coverage notes |
| --- | ---: | --- | --- |
| `abi-encoding` | 8 | no | ABI encode/decode exists but is not wired into the default runtime suite. |
| `access-control` | 20 | no | Role mappings, modifiers, events, ownership. |
| `arithmetic` | 46 | yes | Local arithmetic and arithmetic edge behavior. |
| `calls` | 16 | yes | Internal/external call shapes. |
| `constructor-args` | 4 | yes | Constructor ABI and deployment arguments. |
| `control-flow` | 42 | yes | Branches, short-circuit boolean logic, require paths. |
| `edge-cases` | 12 | no | Small algebraic/value edge cases. |
| `enums` | 13 | no | Enum runtime conversions and comparisons. |
| `equivalence` | 28 | no | Mixed behavior suite; not currently in the default harness. |
| `erc20-minimal` | 11 | no | Token-shaped storage and allowance behavior. |
| `erc721-minimal` | 15 | no | Ownership mappings and events. |
| `events` | 2 | yes | Basic event emission. |
| `hashing` | 9 | no | Keccak and hash helpers. |
| `inheritance` | 6 | yes | Inheritance and inherited mapping behavior. |
| `interfaces` | 4 | yes | Interface calls. |
| `libraries` | 12 | yes | Libraries and overload behavior. |
| `low-level-calls` | 7 | no | `call`/low-level call behavior. |
| `modifiers` | 6 | no | Modifier control flow. |
| `multi-return` | 8 | yes | Multiple return values. |
| `multicall` | 11 | no | Dynamic call data, bytes arrays, low-level calls. |
| `receive-fallback` | 7 | no | Receive and fallback dispatch. |
| `stack-deep` | 7 | yes, compiler-only | Stack-too-deep stress cases; no solc comparison because solc cannot compile the legacy shape. |
| `storage` | 37 | yes | Dynamic arrays, nested mappings, payable storage, storage initialization. |
| `stress-arrays` | 34 | no | Larger array surface area. |
| `stress-control-flow` | 41 | no | Dense branch/loop/control-flow surface area. |
| `stress-events` | 42 | no | Broader event/log surface. |
| `stress-functions` | 24 | no | Many function and return-value shapes. |
| `stress-inheritance` | 31 | no | Larger inheritance surface. |
| `stress-mappings` | 30 | no | Larger mapping surface. |
| `stress-modifiers` | 35 | no | Larger modifier surface. |
| `structs` | 27 | ignored | Present but ignored; currently marked WIP because struct tests have stack-underflow issues. |
| `unifap-v2` | 34 | ignored | Present but ignored; requires `forge-std` setup not currently available in CI. |
| `unifap-v2-create` | 41 | ignored | Present but ignored; requires `forge-std` setup not currently available in CI. |
| `vault-minimal` | 14 | no | Token/vault-shaped storage and call behavior. |

## Feature Matrix

| Feature area | Runtime coverage now | UI/codegen coverage now | Main gaps |
| --- | --- | --- | --- |
| Checked arithmetic and Panic payloads | `arithmetic`, `control-flow`, `edge-cases` exists but `edge-cases` is not default | `tests/ui/codegen/checked_arithmetic_panic.sol`, checked pow/addmod/mulmod tests | Runtime byte-for-byte Panic data for storage/indexed operands, signed narrow ints, exponentiation, division/modulo by zero. |
| ABI encode/decode | `abi-encoding` exists but is not default; `multicall` exists but is not default | ABI/bin emission snapshots and ABI codegen tests | Default runtime coverage for nested dynamic ABI, `abi.encodePacked` with bytes/string, invalid decode/revert behavior. |
| Calldata, memory, and storage arrays | `storage`, `stress-arrays` exists but only `storage` is default | Array bounds, fixed memory allocation, bytes element tests | Runtime FMP integrity, nested fixed arrays, memory array zeroing, calldata slice behavior. |
| Mappings and dynamic keys | `storage`, `stress-mappings` exists but only `storage` is default | Mapping lowering and mapping bytes value snapshots | Raw storage-slot parity for string/bytes keys, nested dynamic keys, mapping-of-struct fields. |
| Storage packing/layout | `storage`, token-shaped suites exist but several are not default | Packed bool and ERC-7201 UI coverage | Runtime packed field overwrite tests, signed packed fields, mixed bool/int/address structs. |
| Constructors and immutables | `constructor-args` is default | Constructor ABI validation and immutable codegen snapshots | Runtime immutable patching across several immutables and constructor paths. |
| Calls and returns | `calls`, `multi-return`, `interfaces`, `libraries` are default; `multicall` is not default | Internal call frame and recursive return snapshots | Runtime memory-returning internal calls, recursion depth, low-level call return/revert data parity. |
| Reverts and custom errors | Some `control-flow`/`calls` coverage | Revert payload and custom error payload snapshots | Runtime byte-for-byte revert data, require string payloads, custom errors through nested calls. |
| Events/logs | `events` is default; `stress-events` exists but is not default | Event-related lowering appears in codegen tests | Full log arity and indexed/non-indexed payload runtime parity. |
| Inline assembly and Yul | Mostly compile/UI coverage | Many parser/lowering/codegen UI tests | Runtime execution checks for memory-safe assembly, calldata access, low-level builtins. |
| Stack depth and backend stack behavior | `stack-deep` is compiler-only in the default suite | Stack phi and EVM IR UI tests | Runtime differential cases where both compilers compile, plus targeted backend stack canaries. |

## Immediate Follow-Up Targets

These are the highest-value next tickets from this inventory:

1. Wire `abi-encoding`, `hashing`, `edge-cases`, `low-level-calls`, `receive-fallback`, and `vault-minimal` into a deterministic runtime path after checking they are stable on CI.
2. Add byte-for-byte return/revert data comparison outside Foundry assertions, so runtime failures do not depend on hand-written test logic.
3. Add raw storage-slot parity checks for dynamic mapping keys and packed storage.
4. Promote ignored `structs` cases once the stack-underflow blockers are fixed.
5. Keep `unifap-*` as a separate follow-up because their dependency setup is larger than ordinary runtime coverage.
