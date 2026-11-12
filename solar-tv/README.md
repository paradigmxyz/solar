# solar-tv

Check a MIR function or EVM IR block window before and after a compiler pass.
The CLI uses the compiler to parse and validate both files, translates its
canonical output to Lean, and asks Lean to prove refinement for MIR or equal
observations for the EVM IR fragment, for every input. A counterexample is replayed
in Lean and independently through the Rust MIR or EVM IR interpreter. Disagreement
is inconclusive, not a reported compiler bug.

From the repository root, with Python 3.10+ and the pinned Lean toolchain:

```sh
cargo build -p solar-compiler -p solar-tv-replay
solar-tv/solar-tv mir solar-tv/examples/mir/dce/{before,after}.mir
solar-tv/solar-tv evmir solar-tv/examples/evmir/peephole/{before,after}.evmir
```

The DCE example deletes an unused multiplication and subtraction. Replacing
the live return with the input breaks it:

```console
$ solar-tv/solar-tv mir solar-tv/examples/mir/dce/{before,broken}.mir
counterexample
  MIR function @with_dead_code
  arg0 = 0
  before = 1
  after  = 0

Difference
  Returned value changed

Changed IR
...

Replay
  /path/to/solar-tv mir /path/to/before.mir /path/to/broken.mir --solar /path/to/solar --timeout 60
```

Failed checks identify the function or block window, concrete inputs and first
differing observable: the return, revert, exception, outgoing edge or live stack.
A short diff shows the changed IR, followed by a command to replay the pair.
These are concrete observations, not a claim about which instruction caused the bug.
Use `--verbose` for the full diff and compiler or prover diagnostics. Unsupported
IR and checker limits remain inconclusive; they are not counterexamples.

Exit codes are `0` for verified, `1` for a replayed counterexample and `2` for
inconclusive, unsupported, timeout or tool failure. Structured reports distinguish
these statuses. Use `--solar PATH` to
select a compiler binary. [Operation coverage](OPERATIONS.md) tracks every schema
entry and distinguishes window proofs from explicit-world entry proofs. Full operation
coverage is still in progress.

## Audit records

Every result shows the Lean semantics fingerprint, pinned toolchain, target,
gas policy and assumptions on stderr. Keep a JSON record with `--report`:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/dce/{before,after}.mir \
  --report target/tv/audits/manual.json
solar-tv/solar-tv summarize target/tv
```

Records bind the canonical input hashes and identify the evidence: a kernel
proof for verification, or a kernel-checked witness with matching Rust observations
for a counterexample. The summary counts `verified`, `counterexample`, `unsupported`,
`timeout`, `inconclusive` and `error` separately. It reads only `audits/*.json`,
so retained failure pairs do not get counted twice. Use a fresh directory when
comparing runs. Corrupt records cause an error instead of silently disappearing.

The Rust helper reparses the original IR and runs the existing interpreters with
the witness's arguments, stack, context sizes, memory and gas. It does not execute
the Python importer's operation recipe. `--replay PATH` or `SOLAR_TV_REPLAY` selects
the helper; by default it is beside the selected compiler. Replay has a 16 MiB
memory limit, ten million units of work and a MIR call-depth limit of 1,024.
A missing helper, unsupported replay state or model disagreement cannot produce
a counterexample result. Positive proofs still trust the importer; concrete replay
does not establish a correspondence theorem for all inputs.

| Layer | Supported fragment | Observation |
| --- | --- | --- |
| MIR | One integer function using i1 and integer widths through i256, word and checked arithmetic, casts, select, phis and branches in an acyclic CFG; constant-address memory and Keccak operations. | Returned bits and live memory, exact modeled revert payload, or exceptional halt. |
| EVM IR | A block window with symbolic live-ins, word/stack operations, constant-address memory, static transfers and terminal halts. | Outgoing edge, live stack and memory; successful/reverted byte payloads; or execution exception, given sufficient gas. |

The importer and the mapping from compiler operations to Lean are trusted.
EVM windows compare live stack values at outgoing edges before executing either
successor. Terminal halts discard the remaining stack. The default check assumes
sufficient gas; the opt-in gas policies below add finite budgets. Physical PCs,
dynamic memory addresses and general EVM control flow remain outside this fragment.

| Layer | Operation semantics | Transform proofs |
| --- | --- | --- |
| MIR | [MIR/Ops.lean](SolarTV/MIR/Ops.lean) | [Dead-code elimination](SolarTV/MIR/Transforms/DCE.lean), [trim trailing default switch cases](SolarTV/MIR/Transforms/TrimSwitchDefaults.lean), [rename phi predecessors](SolarTV/MIR/Transforms/RenamePhiPredecessors.lean), [cast cleanup](SolarTV/MIR/Transforms/Casts.lean), [overflow-check elimination](SolarTV/MIR/Transforms/CheckElim.lean) |
| EVM IR | [EVMIR/Ops.lean](SolarTV/EVMIR/Ops.lean) | [EQ/ISZERO branch peephole](SolarTV/EVMIR/Transforms/EqIszeroJumpi.lean), [commutative swap](SolarTV/EVMIR/Transforms/StackPeephole.lean), [stack safety](SolarTV/EVMIR/StackSafety.lean) |

Both layers share [Memory.lean](SolarTV/Memory.lean). EVM memory instructions and
output are defined in [EVMIR/Memory.lean](SolarTV/EVMIR/Memory.lean); the MIR
dead-store proof lives in [MIR/Transforms/DeadStore.lean](SolarTV/MIR/Transforms/DeadStore.lean).
Gas charging is defined in [EVMIR/Gas.lean](SolarTV/EVMIR/Gas.lean), with the
resource proof in [EVMIR/Transforms/GasSwapAdd.lean](SolarTV/EVMIR/Transforms/GasSwapAdd.lean).

```sh
(cd solar-tv && lake build)
python3 -m unittest discover -s solar-tv/tests
```

The tests regenerate the example outputs with real compiler passes. The `solar-tv`
CI job builds the Lean proofs, runs these tests for both IRs, and checks that an
injected wrong-code pass stops compilation and produces a replayable pair.

The phi example removes an empty forwarder with `cfg-simplify` and updates the
join's incoming predecessor. Its broken version gives the zero edge the other
predecessor's phi value. Run both checks with the following commands.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/phi/{before,after}.mir
solar-tv/solar-tv mir solar-tv/examples/mir/phi/{before,broken}.mir
```

The correct pair verifies. The broken pair returns `21` for input `0`, where the
original returns `11`. Phi inputs are read from the incoming SSA environment
and selected by predecessor, independently of their order in the instruction.
Cycles are inconclusive. The importer limits path expansion to 512 block visits
and path depth to 128 blocks. For supported acyclic pairs, the proof covers every
input.

The typed examples check a guarded boolean cleanup and a conditional branch
with i160 phi values. Inputs and intermediate values use native Lean bit vectors
at their declared width. Arithmetic wraps at that width, and signed comparisons
use its sign bit. `trunc` keeps low bits, `zext` fills with zero, and `sext`
replicates the source sign bit. A truncation to i1 can differ from a nonzero test.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/casts/{before,after}.mir
solar-tv/solar-tv mir solar-tv/examples/mir/typed-branch/{before,after}.mir
solar-tv/solar-tv mir solar-tv/examples/mir/casts/{before,broken}.mir
solar-tv/solar-tv mir solar-tv/examples/mir/typed-branch/{before,broken}.mir
```

The broken cast sign-extends a boolean where the original zero-extends it.
For input `1`, it returns the maximum word instead of `1`. The broken branch
selects the opposite phi value. Counterexamples retain each input's declared
width and are checked again at concrete values. Undefined values and widths above 256 remain inconclusive.

## Outcomes and refinement

[Outcome.lean](SolarTV/Outcome.lean) defines returns, reverts and exceptional
halts, together with refinement as inclusion of possible observable outcomes.
For the deterministic fragment, each input has exactly one outcome, so a
candidate must preserve the returned bits, the revert payload or the exceptional
halt. A failing overflow check is a defined `Panic(0x11)` revert. Replacing it
with a successful return fails refinement.

Checked arithmetic supports signed and unsigned integer layouts, including
division, wrapping division, remainder and exponentiation. Execution stops at the first failure, including when a
checked result is unused. Semantic panic checks retain the complete panic code.
Empty reverts and `invalid` have distinct outcomes. Unsupported operations,
MIR memory-backed revert data, undefined values and checker limits produce
an inconclusive result. Division and remainder by zero produce `Panic(0x12)`;
signed minimum divided by -1 produces `Panic(0x11)` except for wrapping division.

The `check-elim` example removes the addition check dominated by `arg0 < 100`
and retains the unguarded check. Its broken version removes that check too.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/checked/{before,after}.mir
solar-tv/solar-tv mir solar-tv/examples/mir/checked/{before,broken}.mir
```

The broken pair has a counterexample at `2^256 - 1`. The original reports
`revert Panic(17)` and the rewrite returns `0`. Witnesses are checked again
against the refinement relation at their concrete input values.

## EVM stack windows

`bb0 [stack=N]` supplies the entry height, with a default of zero. Every entry
word is an independent symbolic 256-bit input, named `stack0`, `stack1`, and so
on, with `stack0` at the top. Proofs cover all word values at that declared height.
The model checks underflow and the 1,024-word limit after every instruction,
including the label PUSH of a static transfer.

The supported operations include all 26 pure word opcodes, literal PUSH, POP,
DUP/SWAP through depth 235 where the target permits them, EXCHANGE,
CALLDATASIZE and RETURNDATASIZE. Static
PUSH/JUMP and PUSH/JUMPI expose an edge and the remaining stack. Successor
blocks must be stop sentinels. STOP and an empty RETURN both produce successful
empty output. REVERT with empty output and execution exceptions are separate
outcomes. RETURN/REVERT support constant offsets and lengths in the memory
fragment below. The compiler also rejects structurally invalid stack
snapshots before they reach the checker.

The stack order, limit checks and halt behavior follow EVMYulLean at `047f630`.
The Lean stack-safety proofs cover full-stack PUSH and DUP, underflow, and the
failure caused by dropping PUSH/POP at the stack limit. [Reference semantics](https://github.com/NethermindEth/EVMYulLean/blob/047f63070309f436b66c61e276ab3b6d1169265a/EvmYul/EVM/Semantics.lean#L450)

The real peephole example removes SWAP1 before ADD with two symbolic live-ins.
Its broken version uses SUB and corrupts the outgoing stack. The second broken
pair changes INVALID into STOP.

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/symbolic-stack/{before,after}.evmir
solar-tv/solar-tv evmir solar-tv/examples/evmir/symbolic-stack/{before,broken}.evmir
solar-tv/solar-tv evmir solar-tv/examples/evmir/failure/{before,broken}.evmir
```

## Memory

`MLOAD`, `MSTORE`, `MSTORE8`, `MSIZE`, overlapping `MCOPY` and `KECCAK256` are
modeled in both layers. Keccak uses an executable permutation with the EVM
padding, not an uninterpreted function. Bytes use
big-endian word order; `MSTORE8` keeps the low byte. Reads and writes expand
memory to the next 32-byte boundary. A zero-length output does not read or
expand memory, even at the maximum word offset. These rules follow
[EVMYulLean's memory operations](https://github.com/NethermindEth/EVMYulLean/blob/047f63070309f436b66c61e276ab3b6d1169265a/EvmYul/MachineStateOps.lean#L24).

The current projection retains the first **256 bytes** and the active memory
size. `memory0` through `memory7` are arbitrary incoming words; `memoryWords`
is the incoming count of active words. Bytes beyond that count start at zero.
The count is a 251-bit value, covering word-aligned sizes representable by
`MSIZE`. Memory outside the prefix is unchanged by every admitted operation.
All nonempty accesses must have constant offsets and lengths inside the prefix.
Dynamic addresses and larger ranges are inconclusive, not assumed in bounds.

A MIR function's successful return retains its live memory for its caller.
An EVM outgoing edge retains the live stack, memory bytes and active size.
EVM terminal halts discard private memory and compare the returned or reverted
bytes instead. MIR raw memory-backed revert payloads remain unsupported.
The optional EVM gas model also charges for memory expansion.

The MIR example runs `memory-dse`: a later word store overwrites the earlier
store and subsumes its expansion. The broken version moves the surviving store,
changing `MSIZE` from 32 to 64 at the zero input. The EVM peephole example removes
an addition of zero before a store. Its broken version shifts the store by one
byte and corrupts the returned payload.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/memory/{before,after}.mir
solar-tv/solar-tv mir solar-tv/examples/mir/memory/{before,broken}.mir
solar-tv/solar-tv evmir solar-tv/examples/evmir/memory/{before,after}.evmir
solar-tv/solar-tv evmir solar-tv/examples/evmir/memory/{before,broken}.evmir
```

Verification still requires a kernel-checked universal proof. The solver and
a few concrete memory patterns only propose counterexamples; each mismatch is
replayed in the kernel before it is reported. Failure to prove or find a
counterexample remains inconclusive.

## Gas and out-of-gas

Gas checks apply to **scheduled EVM IR**, where the instructions to charge are
known. The default remains gas-independent. Choose a policy explicitly:

- `--gas=exact`: preserve the modeled outcome and exact remaining gas at every
  input budget, including out-of-gas. A cheaper rewrite can fail this check.
- `--gas=refine`: whenever the original avoids out-of-gas, preserve its modeled
  outcome and leave at least as much gas. Original out-of-gas is unconstrained.
  This permits savings, but is weaker than full EVM equivalence: callers or
  continuations that observe extra gas can distinguish them. Use `exact` when
  that distinction matters.

The budget `gas` is a symbolic 256-bit input. `GAS` observes it after paying its
own two-gas charge. Instructions debit their base price and memory expansion
before execution. Expansion costs `3 * words + words² / 512`; repeated accesses
pay only for additional growth. Out-of-gas and other exceptions consume the
remaining gas. Successful returns and reverts retain what is left.

The prices and debit order follow [EVMYulLean's gas schedule](https://github.com/NethermindEth/EVMYulLean/blob/047f63070309f436b66c61e276ab3b6d1169265a/EvmYul/EVM/Gas.lean#L18)
and [execution semantics](https://github.com/NethermindEth/EVMYulLean/blob/047f63070309f436b66c61e276ab3b6d1169265a/EvmYul/EVM/Semantics.lean#L442).
The admitted window operations support every compiler target from Homestead
through Amsterdam, with Cancun as the CLI default. `--evm-version` selects
availability and pricing, including EXP, PUSH0 and EXCHANGE differences.
Window checks reject calls, storage and refunds. The explicit-world mode below
executes them with a fixed entry budget. Neither mode includes transaction
validation or intrinsic gas.

These are explicit opcode windows: execution starts after any entry JUMPDEST,
and ends before the outgoing successor executes. A static transfer includes its
label PUSH and JUMP/JUMPI. Layout-dependent structured jumps remain unsupported.
The checker does not estimate EVM gas from MIR; `mir --gas=...` is inconclusive.

## Storage, calls and creation

Use `--world` for stateful validation against an explicit account fixture.
MIR executes directly as typed SSA, including CFG edges and simultaneous phis.
Select an internal function with `--function`, or omit it for an `[entry]`
dispatcher. EVM IR checks a complete runtime entry.

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/storage/{before,after}.evmir \
  --world solar-tv/examples/evmir/storage/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/storage/{before,broken}.mir \
  --world solar-tv/examples/mir/storage/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/storage-passes/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/storage-passes/world.json
solar-tv/solar-tv evmir solar-tv/examples/evmir/creation/{before,broken}.evmir \
  --world solar-tv/examples/evmir/creation/world.json --report target/tv/create.json
```

The first pair removes an overwritten storage write. The second changes a stored
value. The creation counterexample changes the deployed code. Reports name the
changed account, storage slot, balance, nonce, code or output, and replay it in
Lean and independently in the Rust MIR interpreter or `evm2`, respectively.
The `storage-passes` pair is actual `storage-cse,storage-dse` output and proves
for every typed argument and declared symbolic storage cell. Its `broken.mir`
changes a surviving write and produces a replayed counterexample.

A fixture declares `version: 1`, optional `address`, `caller`, `gas`, `static`,
hex `calldata`, `warm_slots: [[address, slot], ...]`, and `accounts`. Each account
has an `address` and optional `nonce`, `balance`, hex `code`, and `storage` object.
Numbers accept decimal or `0x` strings. Defaults are root address 65536, caller
131072, one million gas, zero value/gas price, empty input and fresh memory.
The root account defaults to nonce one and gets its code from the IR. Omitted
accounts are absent; omitted slots are zero. Storage keys or values such as
`"$seed"` are universally quantified words, as are declared EVM entry stack words.
Everything else in the fixture is concrete.

For direct MIR, `memory: {"data": "0x...", "size": 96}` supplies initial byte
memory; `size` must be word-aligned, at most 64 KiB, and contain `data`. Omitted
bytes are zero. There is no implicit FMP or frame setup. An account may also
supply `transient: {"0": "$seed"}` at an internal function boundary; transaction
entries start with empty transient storage. A normal function return exposes
its values, all active memory, persistent/transient cells and logs. STOP and raw
RETURN end the transaction; private memory and transient cells then disappear.
Revert and exceptional halt roll back the journal and retain their exact outcome.

MIR state validation compares semantic effects with sufficient gas. It assigns
no EVM gas costs to semantic instructions and rejects `--gas` and fixture `gas`
budgets. Calls, allocation, aggregates, ABI/layout helpers and backend memory
records remain explicitly unsupported. They never fall back to compilation.

Environment inputs are optional `origin`, `gas_price`, `chain_id`, a `blob_hashes`
array and a `block` object. The block accepts `number`, `coinbase`, `timestamp`,
`gas_limit`, `difficulty`, `prevrandao`, `base_fee`, `blob_base_fee`, `slot` (uint64)
and `hashes` (block number to hash). Omitted history hashes are explicitly zero;
BLOCKHASH still enforces the previous-256-block window. Defaults are chain ID one,
timestamp one, gas limit `2^64-1`, blob base fee one and zero for other block fields.
`warm_addresses` adds up to 64 explicitly warm accounts. From Shanghai, the
selected coinbase is warm automatically.

[EVMIR/State/](SolarTV/EVMIR/State/) contains executable account, memory, call and
constructor semantics. It implements persistent/transient storage, cold/warm
accesses, refunds, logs, CALL/CALLCODE/DELEGATECALL/STATICCALL, CREATE/CREATE2,
value transfers, constructor code deposit and journal rollback. Child frames
execute their actual bytecode with private memory and return data. All 18
precompiles active across the supported forks have executable Lean models,
including signature recovery, hashes, ModExp, BN254, BLS12-381 and KZG point
evaluation. [Precompiles/](SolarTV/EVMIR/Precompiles/) separates encodings, field
and curve arithmetic, gas schedules and execution. Tests compare their results
and gas directly with `evm2`; entry proofs still require the Lean kernel.

SELFDESTRUCT transfers funds immediately and defers deletion until transaction
end. From Cancun, existing accounts keep code and storage; accounts created
within the transaction retain deletion. Refunds, access warmth and parent
rollback follow the selected fork.

From Prague, EIP-7702 designators resolve exactly one hop, with target access gas
charged before forwarding. All four call kinds retain their address/caller/value
rules; `EXTCODE*` reads the indicator, while `CODESIZE`/`CODECOPY` reads executing
code. Delegating to a precompile does not invoke it. The fixture represents state
after authorization processing: authorization signatures, intrinsic gas and
authorization refunds are outside this entry checker.

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/sha256/{before,after}.evmir \
  --world solar-tv/examples/evmir/sha256/world.json
solar-tv/solar-tv evmir solar-tv/examples/evmir/selfdestruct/{before,broken}.evmir \
  --world solar-tv/examples/evmir/selfdestruct/world.json --evm-version=shanghai
solar-tv/solar-tv evmir solar-tv/examples/evmir/environment/{before,after}.evmir \
  --world solar-tv/examples/evmir/environment/world.json
solar-tv/solar-tv evmir solar-tv/examples/evmir/delegation/{before,after}.evmir \
  --world solar-tv/examples/evmir/delegation/world.json --evm-version=prague
```

This is a **scenario proof**, not arbitrary-world equivalence. MIR's structured
import uses the compiler parser, verifier and operation schema, without codegen.
Its independent replay reparses and interprets the original MIR, never an imported
recipe. The audit names this boundary and contains no backend-image hashes.

EVM IR inputs are compiled without optional optimization to runtime bytecode;
backend lowering remains an additional trusted boundary there. Native replay
rebuilds the original IR independently. EVM audits bind the fixture, opcode
configuration and both runtime images. Both modes record a native sample even
for a positive kernel proof. This does not prove backend lowering correct. The
root's code bytes are not themselves an output; CODECOPY, self-calls and code introspection still execute
against the actual before/after image. Child deployed code remains observable.

EVM world mode supports Istanbul through Osaka. Amsterdam additionally admits the
environment, scalar and memory fragment, including SLOTNUM; account, state, log,
call and creation operations remain unsupported until two-resource gas is modeled.
History is bounded to 256 entries and blob hashes to 64. Limits are 32 initial
accounts, 64 storage cells per account,
64 KiB private memory per frame and 512 evaluator steps/depth by default
(`fuel: 1..2048`). BLAKE2F has a separate 4,096-round model work limit;
ModExp permits at most 64 KiB per operand (Osaka's protocol limit is 1,024 bytes).
A model limit or unsupported path prevents verification; it is never
equated with an EVM exception. The default compares effects at the fixture's
finite gas budget. `--gas=exact` also compares remaining gas and refunds;
`--gas=refine` allows only original out-of-gas to be unconstrained, otherwise
requiring the same effects/refund and at least as much remaining gas. Fixture
checks are standalone; the compiler's pass hooks continue to check local windows.

The real SWAP1/ADD peephole costs 17 gas before and 14 after, through its outgoing
transfer. It verifies under `refine`, and fails `exact` with different remaining
gas. The broken version adds a PUSH/POP: the original finishes at budget 17,
while the candidate runs out of gas.

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/gas/{before,after}.evmir --gas=refine
solar-tv/solar-tv evmir solar-tv/examples/evmir/gas/{before,after}.evmir --gas=exact
solar-tv/solar-tv evmir solar-tv/examples/evmir/gas/{before,broken}.evmir --gas=refine
solar-tv/solar-tv evmir solar-tv/examples/evmir/gas-observer/{before,broken}.evmir --gas=refine
solar-tv/solar-tv evmir solar-tv/examples/evmir/gas-memory/{before,broken}.evmir --gas=refine
```

The observer example moves `GAS` across a PUSH/POP, changing the observed value
from 16 to 11 at budget 18. The memory example reads farther from the start:
both versions halt successfully with sufficient gas, but only the original
finishes at budget 10. Reports and replay commands retain the policy, budget,
target version and remaining gas.

## Check passes during compilation

From the repository root:

```sh
export SOLAR_TV="$PWD/solar-tv/solar-tv"
target/debug/solar solar-tv/examples/mir/dce/before.mir \
  -Zmir-pipeline=dce -Zsolar-tv=target/tv
target/debug/solar solar-tv/examples/evmir/peephole/before.evmir \
  -Zevm-ir-pipeline=peephole -Zsolar-tv=target/tv
SOLAR_TV_GAS=refine target/debug/solar solar-tv/examples/evmir/gas/before.evmir \
  --evm-version=cancun -Zevm-ir-pipeline=peephole -Zsolar-tv=target/tv
```

`SOLAR_TV_GAS=exact|refine` enables gas auditing at EVM IR pass boundaries. The
compiler forwards its actual EVM target version. MIR boundaries keep their
gas-independent checks; the environment never assigns gas prices to MIR.

The host flag also applies to compilation from Solidity:

```sh
target/debug/solar solar-tv/examples/solidity/functions.sol -Ogas --emit=bin \
  -Zsolar-tv=target/tv
solar-tv/solar-tv summarize target/tv
```

Each pass manager compares its module before and after a pass, including edits a
pass forgot to report. MIR checks each changed function separately. EVM IR checks
complete changed blocks with proven fixed incoming stack heights. Unrelated
dispatchers, helpers and successor bodies do not have to fit the modeled fragment.
The compiler still parses and validates the full module, and Rust counterexample
replay executes the selected region in that original context.

Reports identify the function or stable block label and incoming height. Both
module and region hashes are retained. A verified region is a local proof, not a
claim that the entire pass or contract was verified (`whole_pass: false`). ABI and
function interface changes, new/deleted functions, shared declaration changes,
unknown stack heights and changed window boundaries remain unsupported. In
particular, replacing a revert block with a jump to shared revert code needs a
larger region; it is not reported as a wrong-code counterexample.
If a neighboring block also changes, stack-position correspondence is unknown,
so that window is unsupported too. This avoids blaming a valid stack reshuffle
that spans several blocks.

You can select a region when replaying full snapshots:

```sh
solar-tv/solar-tv mir before.mir after.mir --function helper
solar-tv/solar-tv evmir before.evmir after.evmir --window bb7 --stack-height 3
```

The standalone window proof assumes the specified incoming height. The pass hook
derives it from the original and edited CFGs; a maximum bound is insufficient.
Successor labels denote the same boundaries on both sides. No instructions are
cut out of a block. Layout changes and shared-tail transformations remain
unsupported; structured transfers are also unsupported under gas policies.
Full context is limited to 1 MiB per file, and each selected region keeps the
128 KiB and 512-line limits. Unrelated EVM bodies are not copied into the proof.
For a complete physical `JUMPI; STOP` block, the untaken path executes `STOP`;
it is not an invented edge to the next block.

A verified pair produces a message on stderr. A counterexample stops the
pipeline. Unsupported, timed-out and inconclusive checks warn and compilation continues.
A checker launch or protocol failure is a compiler error. Each unsuccessful
check gets a separate directory containing `before`, `after` (with the IR
extension), `result.txt` with the pass, module and checker response, and `result.json`.
Each changed check also writes one atomic record under `audits/`, including successful
checks whose IR snapshots are discarded. Concurrent checks use separate record files.
The diagnostic includes the witness or inconclusive reason and replay command.
Replay a saved MIR pair from the directory named by the diagnostic:

```sh
pair=target/tv/mir-XXXXXX  # use the directory from the diagnostic
solar-tv/solar-tv mir "$pair/before.mir" "$pair/after.mir"
```

Set `SOLAR_TV` to the validator executable or put `solar-tv` on `PATH`.
The checker reparses with the running compiler. Embedders can set `SOLAR` to
the compiler executable. Only host options enable auditing for Standard JSON.

The wrong-code integration test injects a pass that replaces a live return
value and rechecks the saved pair with Lean:

```sh
SOLAR="$PWD/target/debug/solar" SOLAR_TV="$PWD/solar-tv/solar-tv" \
  cargo nextest run -p solar-compiler --test it --run-ignored only -E 'test(solar_tv::)'
```
