# solar-difffuzz

A differential fuzzer for the whole chain from Solidity source to deployed bytecode, and for
solar-verify's proofs about that bytecode.

Each case is a contract and a sequence of calls. Every call runs on these legs, from the state
the previous call left:

| Leg | What runs |
|---|---|
| `solar-evm` | the compiler's bytecode on revm, the reference for the next three |
| `mir` | the final lowered MIR (`-Zdump=mir-final`) on `solar-mir-interpreter` |
| `evmir-scheduled` | the runtime EVM IR as stack scheduling emitted it, on `solar-evmir-interpreter` |
| `evmir-final` | the runtime EVM IR after the EVM IR pipeline, as assembled |
| `solc-evm` | solc's bytecode (`--via-ir` by default) on revm |
| `solidcore` | solidity-lean's SolidCore source semantics, with `--lean` |

revm is not the EVM the compiler's own tests run on (evm2), so the EVM legs do not share a
bug with them. Each leg is compared byte for byte: success or revert, return and revert data,
logs (topics and data), and storage. The compilers' EVM runs and SolidCore compare the whole
nonzero storage after each call; an interpreter compares the slots it wrote with what the EVM
changed. A leg that cannot run a call (an interpreter reaching `gas`, `create` or another
contract) reports it as unsupported, which is counted and never compared; a dump that does not
parse back is a finding of its own (`invalid-dump`). The dumps never change the bytecode: each
dump compiles again and must emit the same runtime.

```sh
cargo build -p solar-compiler --bin solar -p solar-difffuzz -p solar-verify
T=target/debug
# Generated contracts.
$T/solar-difffuzz run --seed 0 --iters 1000 --out /tmp/fz --solar $T/solar --solc solc
# Mutations of existing contracts.
$T/solar-difffuzz corpus --iters 1000 --out /tmp/fzc --solar $T/solar --solc solc tests/ui/codegen/run-call/**/*.sol
# Spec soundness (needs z3).
$T/solar-difffuzz specs --iters 100 --out /tmp/fzs --solar $T/solar --solc solc --verify $T/solar-verify
# A saved case: exits 1 while it still diverges.
$T/solar-difffuzz replay tools/difffuzz/cases/open/alloc-pointer-add.json --solar $T/solar --solc solc
```

`--solar-arg` and `--solc-arg` replace the compilers' options (`--evm-version=osaka` and
`--evm-version osaka --via-ir`), `--calls` sets the length of a sequence and `--timeout`
bounds each compiler process. Every run writes `log.jsonl` (one line per case) and
`summary.json`: cases, calls, how each leg ran, why legs could not run, how solc's calls
ended, solc's rejections of generated code (a generator bug) and accepted divergences.

## Generated contracts

`grammar.rs` generates contracts in the subset every leg runs: integer types of several
widths and signedness, `bool`, `address`, scalars, nested mappings, fixed and dynamic storage
arrays and packed structs in storage, events with indexed fields, custom errors with
arguments, `require` with and without messages, `assert`, checked and `unchecked`
arithmetic, shifts, exponentiation, explicit conversions, ternaries, bounded `for` loops,
`push`, `pop`, `delete`, `break` and `continue`, `keccak256(abi.encode(..))`, internal pure
helpers, and internal procedures that read and write storage. Literals in code are mostly small, call
arguments mostly boundary values. Programs are kept as a typed tree, so reduction rewrites
the tree and every candidate still type-checks: it drops calls, functions, state variables,
statements, and replaces expressions with literals of their type or with one of their
operands.

`corpus` deploys every contract of the given files whose constructor takes no arguments,
swaps operators or changes literals in one to three places, and calls the functions whose
parameters are value types. A corpus case is reduced by calls and then by lines.

## SolidCore

With `--lean DIR` (a solidity-lean checkout on `ck5/certified-translation`, built with
`lake build SolidCore.Solidity.Checked`) and `--lean-solc` (the solc 0.8.35 its importer
pins), every case also runs on SolidCore. solidity-lean's importer turns solc's AST into a
SolidCore `SourceUnit`; a generated Lean file type-checks it, runs the constructor, and runs
each call through `CheckedContract.callCalldataAtFromWithContext?` from the storage the last
successful call left, as `SolidCore.Refinement.Source.result?` does for one call (caller
`0x2000`, contract `0x1000`, no value), and `lake env lean` evaluates it with `#eval`. This
is a test of the semantics, not a proof: nothing the fuzzer runs is kernel-checked. An import
or checker rejection, a missing result and running out of fuel are failures of the leg, never
agreement.

## Spec soundness

`specs` mines candidate specs from a run of each generated contract, after Daikon: contract
invariants over its integer scalars (orderings and bounds) and postconditions relating each
function's scalars, `old` values, integer parameters and single integer result. It keeps
the candidates every successful call satisfied and asks solar-verify to prove them, dropping
whatever fails until the rest verify together (Houdini). A case counts only when every
function, the constructor included, verifies. Then eight fresh sequences of 30 calls run on
the annotated contract's Solar and solc bytecode, and every proved spec is evaluated after
every successful call over math integers, as the verifier reads it. A violation is a
soundness bug of the verifier or of the compiler and is saved under `violations/`. Contracts
for this mode have no `assert` (always an obligation) and public scalars (read through their
getters). The verifier's internal errors are saved under `verify-internal/`.
Half the functions with an integer parameter also get a `@custom:check` bound on it, which
the verifier assumes: their cases run on the `--instrument` output, which enforces it.

## Foundry

Comparisons that Foundry can express run on Foundry nightly 1.9.1-nightly, commit
`5717c1b8fea92d59da287e56e8b6c798a3a36e74` (`foundryup --install nightly` on 2026-11-12); the
custom legs above remain only for SolidCore and the MIR and EVM IR interpreters.

- `--forge PATH` runs `forge equivalence` on Solar's and solc's deployed code for every generated
  case, from arbitrary storage and every calldata, beside the random calls. A distinguishing input
  is replayed from the state the calls left; one that agrees there is counted as unreached and
  kept under `forge-unreached/`. So far those have all been dirty padding (CODEGEN-025).
- `forge/` is a Foundry project over Seaport, pinned at the external suite's revision.
  `bash forge/setup.sh [solar]` builds its optimized (assembly) and reference (high-level)
  implementations with solc and with the compiler, and `forge test` runs fuzz tests (views,
  order hashing, raw calldata, counters, signed ETH-for-ERC721 orders seeded with
  `basic.spec.ts`'s amounts as fixtures, getters over arbitrary stored words) and an invariant
  test that applies random fills, cancellations and counter bumps to all four builds in lockstep.
  Same-source builds must agree byte for byte; the two implementations must agree on success and
  return data. `bash forge/equivalence.sh` checks each implementation's two builds with `forge
  equivalence` entry point by entry point and writes `code/equivalence.jsonl`.

On the four builds, every fuzz test passes 1,000 runs and the invariant test 64 runs of 32 calls.
`forge equivalence` proves the two builds of each implementation equivalent on `name()`,
`information()`, `getOrderStatus`, `getOrderHash` and `cancel`. It is incomplete on
`incrementCounter` (symbolic `BLOCKHASH`), on `getCounter` and `getContractOffererNonce` (a
counterexample that does not replay concretely; `testFuzz_storedWords` finds no difference there),
and on the optimized `fulfillBasicOrder`. On the reference `fulfillBasicOrder` it found CODEGEN-029:
an enum read from forged storage is checked at the load, where solc checks it at the use.

## Divergences

Every divergence is saved under `divergences/` with its legs, kind and call, reduced while it
keeps the same legs and kind, and written as JSON and as Solidity. Some are expected and are
counted in `summary.json` instead:

- **Gas.** A call that reads gas, or runs out of it on either EVM, may differ in anything:
  the compilers do not promise gas parity, and the interpreters do not model gas. Nor do
  they model the EVM's 1024-word stack, so a call whose bytecode overflows it is compared
  only between the two compilers.
- **Documented compiler differences.** Between Solar's and solc's bytecode only, a contract
  with inline assembly (raw bits and memory layout: CODEGEN-001, -009, -010, -012, -025 in
  docs/SOLC_DIVERGENCE.md), one that reads compiled code (`type(C).runtimeCode`) or one with
  internal function types (pointer values are code offsets). Solar's own legs and SolidCore
  are still compared.

These need a look before they are classified, and `cases/` records the ones found so far:

- **Raw memory in assembly.** A contract that returns or hashes memory it did not write sees
  each compiler's memory layout (docs/SOLC_DIVERGENCE.md, CODEGEN-009 and CODEGEN-010).
- **Evaluation order.** solc's legacy pipeline and its IR pipeline order some operands
  differently; the compiler and `--via-ir` agree on every generated case so far.

### Open cases

`cases/open/` holds reduced cases that still diverge, for `replay`:

- `alloc-pointer-add.json`: the final lowered MIR of a function that builds a memory struct
  for a custom error adds a constant to an `alloc` pointer with `add`, not `ptradd`, and the
  MIR validator rejects the dump when it is parsed back, so the MIR interpreter cannot check
  any such function. The bytecode itself agrees with solc. Several UI tests show it, such as
  `tests/ui/codegen/run-call/dirty-values/abi-events-errors/errors_ints.sol`.
- `assignment-evaluation-order.json`: `s0[1] = (uint16(5) + uint16(65535)) | s0[0];` on an
  empty array. The compiler and solc `--via-ir` revert with `Panic(0x11)`: they evaluate the
  overflowing addition first. SolidCore reverts with `Panic(0x32)`: it follows the legacy
  pipeline's order (solidity-lean's EVAL_ORDER_DESIGN.md) and checks the index first. Under
  this order no refinement from SolidCore to the compiler's bytecode can hold for such a
  function; the source semantics and the compiler must pick the same order.
- `binary-operand-order.json`: `require((int128(7) / int128(0)) < (int128(-2) + type(int128).min));`.
  The compiler and solc `--via-ir` revert with `Panic(0x12)`: they evaluate the left operand
  first. SolidCore and solc's legacy pipeline revert with `Panic(0x11)`: they evaluate the right
  operand first. Same cause as the case above, for binary operators.
  An out-of-range index on the right (`int256(1) / int256(0) - s3[2]` on a `int256[2]`) shows
  the same order: `Panic(0x12)` against SolidCore's `Panic(0x32)`.
