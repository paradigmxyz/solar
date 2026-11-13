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
budgets. Direct internal calls share memory and state while keeping separate,
width-typed SSA locals. Tail calls replace the current activation. Revert,
exception, STOP and raw RETURN propagate through callers; normal returns resume
the caller. One work budget covers the whole call tree, with a separate model
limit of 128 live activations. Only the selected function's reachable callees
are imported, including recursive cycles.

`alloc` reads the explicit FMP at 0x40, reserves exactly the requested size,
updates the FMP, then performs any requested zero initialization. Reservation
alone does not expand or clear memory. Checked overflow or an end above uint64
reverts with Panic(0x41); violating an infallible allocation's validity contract
is unsupported. No initial FMP, alignment, heap floor or physical frame is
invented. Deferred allocations and physical multi-return calls use explicit
placements, described below. Function-pointer
dispatchers are ordinary MIR bodies and use the
same internal-call semantics. The function-pointers Solidity example checks a
real inlining pair for both choices and replays an incorrect callee change.
Recursive ABI helpers and explicit placement windows are
supported below. Backend memory records remain unsupported.

A deferred allocation uses `mir_layout.allocations.<function>.<site>`, where
`site` is its zero-based position among the function's active MIR instructions
(phis included, terminators excluded). A `static` placement supplies `address`
and `size`; a `heap` placement supplies `size` and bumps the explicit FMP each
time the instruction executes. The extent must match the MIR operand. Static
reservation alone neither touches memory nor changes the FMP; requested zero
initialization still touches the exact range. Missing placements, mismatched
extents and invalid addresses are inconclusive. There are at most 4,096 bindings
across 128 functions, each reserving at most 64 KiB.

```json
{"mir_layout":{"allocations":{"f":{"0":{"kind":"static","address":256,"size":32}}}}}
```

These are explicit inputs to a placement window. They do not certify that the
backend selects those placements, and do not import its captured memory records.

MIR `mcopy` is a logical overlap-safe copy on every supported fork. Pre-Cancun
targets lower it to loops; scheduled EVM `MCOPY` still requires Cancun. Partial
copy tails retain exact memory expansion as well as bytes. The example compares
real `lower-mcopy` output and replays the old source-overread counterexample:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/mcopy/{before,after}.mir \
  --function=f --evm-version=shanghai --world solar-tv/examples/mir/mcopy/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/mcopy/{before,broken}.mir \
  --function=f --evm-version=shanghai --world solar-tv/examples/mir/mcopy/world.json
```

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/internal-calls/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/internal-calls/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/allocation/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/allocation/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/allocation/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/allocation/world.json
```

These are real `inline` and `lower-alloc` outputs. The allocation pass creates a
shared checked-allocation helper, which is interpreted as ordinary MIR. The
broken pair aliases two allocated objects; the checker reports the changed
return value and independently replays both original modules. Memory read/write
normalization uses kernel-proved range and byte-order laws. Closed initial reads
reduce by definitional equality; symbolic bytes stay abstract.

### Typed MIR products and slices

`insert_value` and `extract_value` operate on immutable nested products. An
uninitialized field stays undefined; reading another field does not initialize
it or invent a zero. Phis read their old incoming values simultaneously, selects
do not read unchosen scalar bits, and internal calls/tail calls can pass and return
logical products. An undefined scalar used in an effect, branch or returned
observable makes the check unsupported.

`make_memory_slice`, `make_calldata_slice` and `make_returndata_slice` construct
location-tagged pointer/length pairs. Construction neither allocates nor accesses
the range. Memory slices retain aliases; returndata slices refer to the current
volatile return buffer. Their types and values are defined in
[MIR/State/Values.lean](SolarTV/MIR/State/Values.lean), and product operations in
[MIR/State/Aggregates.lean](SolarTV/MIR/State/Aggregates.lean).

To compare type-lowering passes, the selected function boundary exposes scalar
leaves in declaration order. Both interpreters rebuild the original parameter
tree independently and flatten its returned leaves. A pointer may be represented
by its address word after lowering; two explicitly different pointer address
spaces are rejected. This boundary relation assigns no implicit physical ABI,
padding or frame addresses. A selected outer function exposes its return
components logically. Nested lowered calls with several scalar results require
an explicit `mir_layout.functions.<callee>.return_buffer` placement. For a single
logical aggregate or slice, supplying that placement selects the physical
memory-return convention; without it, the result remains a logical SSA value.
Publication flattens nested fields in declaration order, with each slice's
pointer before its length. Later leaves occupy `buffer + 32*k`, followed by the
buffer address at `0x20`, including when the call result is unused. Empty and
single-leaf products write no buffer. Typed callers retain their logical first
component until lowering reconstructs it from the physical words.

Tail calls keep the original callee's buffer. Nested calls may use overlapping
buffers: returned SSA values are captured before publication, and writes retain
their order and memory aliases. Only the published extras need defined bits; an
unused undefined first word is neither stored nor given an invented value.
This checks the supplied memory-return convention; it does not infer or certify
the backend's placement or stack-return decisions. Missing required placements
and undefined stored extras remain inconclusive.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/return-buffers/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/return-buffers/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/return-buffers/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/return-buffers/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/aggregate-returns/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/aggregate-returns/world.json
```

The aggregate example is a real `lower-structs`, `lower-frame-slots`,
`lower-slices`, `lower-memory-objects` pair. Its broken variant reads a return
field from the next buffer offset. [Returns.lean](SolarTV/MIR/State/Returns.lean)
also proves that placed pair and slice publication agree with their expanded
components for arbitrary values and memory.

Types are limited to depth 32 and 256 nodes, with a whole-model budget of 65,536
exported type nodes checked before expansion. A boundary admits at most 32 input
and 256 return leaves. Limits are reported as unsupported, never assumed away.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/aggregates/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/aggregates/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/slices/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/slices/world.json
```

These are actual `lower-structs` and `lower-slices` outputs. Replace `after` with
`broken` to replay a wrong-field or wrong-slice-pointer counterexample.

### MIR slots and storage bytes

Mapping and dynamic-array slot operations hash their real bytes and preserve
scratch writes and memory expansion. Byte keys use unreserved FMP scratch;
copying may overlap the source, and array offsets wrap at 256 bits. External
balance/code reads use live fixture accounts, with address truncation, access
warmth, padded copies and the original code of EIP-7702 designators. The MIR
contract's own runtime code remains unknown.

Storage byte helpers validate short/long headers, allocate memory objects,
clear old long tails and mask dirty padding. Invalid headers panic with 0x22;
allocation and arithmetic failures keep their own panic codes. Full-word memory
traffic remains observable, including when scratch aliases the input object.
Storage loops have a 2048-word work limit. Model errors never certify a pair.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/slots/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/slots/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/storage-bytes/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/storage-bytes/world.json
```

The examples are actual `lower-mapping-slots` and `lower-builtins` outputs;
`broken.mir` changes a parent slot or stored byte. Constant `datacopy` also
executes directly for ranges wholly inside known data. Library patches use the
explicit `mir_layout.libraries` addresses. Deferred contract code requires
`mir_layout.code` and the entry's `data` extent. The verifier still requires an
exact `datasize` copy; use `codecopy` for arbitrary image ranges, including
neighboring bytes and zero padding. Known entries must match a supplied image
after patching; inconsistent bytes or extents are inconclusive. Empty copies
read no source bytes or relocations; computing `datasize` still needs its extent.
Exported constant bytes are limited to 64 KiB across the selected call graph. Supplied
deferred images remain boundary assumptions, not proofs of another contract's
compilation or final assembler placement.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/linked-data/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/linked-data/world.json
```

This example specializes a copy under the supplied link boundary. Its broken
variant writes one byte later, changing both memory and the returned word.

Storage-array loads also execute directly: packed integers, signed values,
fixed bytes, checked enums and nested bytes objects. The array header is reserved
before data-slot hashing, and child allocations share the FMP and scratch memory.
Copies above 2048 elements are unsupported. External scratch frame slots alias
ordinary memory at base 128; slices retain pointer/length order. Internal frames
use explicit per-function activation bases. Slot offsets follow the two-word
frame header and the function's argument/return components. Both direct slots
and `internal_frame_addr` share ordinary byte memory, including aliases.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/storage-arrays/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/storage-arrays/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/frames/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/frames/world.json
```

These pairs come from `lower-builtins` and `lower-frame-slots`. Their broken
versions change an element encoding or the returned frame value.

### MIR ABI and placement windows

ABI encoders handle scalars, function pointers, bytes, arrays and nested tuples in
object, slice and scratch modes, with an optional selector. Sources can live in
memory, calldata or returndata; storage byte strings retain their header checks and
slot reads. Returndata copies retain bounds checks and opcode availability.
Decoders handle recursive layouts and multiple results from memory objects or
memory/calldata slices. They preserve offset and signed length checks, dirty-word
rejection, allocation/copy order, padding and function-word normalization.
Layouts are bounded to depth 32 and 256 nodes; each operation has a work limit of
2,048 elements. Exceeding a bound is unsupported, never a successful short result.
Memory slices of static aggregates read contiguous encoded heads; dynamic memory
aggregates retain their object representation.

Require builtins preserve exact revert bytes, including short and long strings,
dynamic messages and recursive custom-error arguments. `require(false, "")`
returns `Error(string)` with an empty string, not an empty revert. Passing checks
evaluate their arguments but do not read the referenced payload objects.
Set `debug_reverts: true` in the fixture to model compiler-generated check
messages from `--revert-strings=debug`. The default checks revert with empty
data; explicit user payloads retain the bytes encoded in MIR in either mode.

Checked `addmod` and `mulmod` panic on a zero modulus; their nonzero cases use
full-width modular arithmetic. ERC-7201 namespace hashing retains the input
reads and scratch write. Send and transfer execute real children with the
2,300-gas stipend, including out-of-gas and storage-sentry failures. Transfer and
`revert_returndata` preserve the failed call's complete revert bytes.

MIR `selfdestruct` applies fork-specific account lifetime and balance effects,
including Cancun's preservation of existing accounts, and rejects static writes.
Amsterdam's changed destruction semantics remain unsupported in this path.

SHA-256, RIPEMD-160 and address recovery also run as direct MIR builtins.
Provide an ordered `builtin_gas` list in the world fixture, with the child gas
budget at each executed precompile call. Like `creation_gas`, this is an explicit
boundary input because semantic MIR has no physical caller gas price. Budgets
are shared across internal calls; missing budgets make the check inconclusive.
Consumption of creation and builtin budgets remains observable at internal
returns, so a rewrite cannot silently shift a caller's later budget sequence.
Both successful and out-of-gas calls run the actual precompile in Lean and in
independent replay, preserving scratch writes and volatile returndata.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/precompile-builtins/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/precompile-builtins/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/precompile-builtins/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/precompile-builtins/world.json
```

Packed encodings accept literals, scalars and byte references, retaining each
unaligned word store and selecting scratch from the whole write footprint. They are bounded
to 2,048 parts and 64 KiB of data. Memory objects and slices, calldata slices and
storage bytes keep ordered header reads, overlapping copies and dirty word tails.
Hashes may borrow and restore a memory object's header. Packed arrays retain
padded and cleaned leaves, declared memory strides, nested memory rows and
wordwise calldata reads when ranges wrap. Array work is capped at 2,048 nodes.
Physical call-return buffers need the explicit convention above. `validate_abi` needs decoder
provenance or canonical raw-word evidence; absent evidence is inconclusive, not
a fabricated runtime guard.

The packed-array example uses a two-word memory stride. Its broken rewrite
copies the gap between elements instead of the second element:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/packed-arrays/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/packed-arrays/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/packed-arrays/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/packed-arrays/world.json
```

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/abi-scalar/before.mir \
  solar-tv/examples/mir/abi-scalar/after.mir --function f \
  --world solar-tv/examples/mir/abi-scalar/world.json
```

A real reference encoding pair, plus an incorrect relative-offset rewrite:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/abi-reference/{before,after}.mir \
  --function f --world solar-tv/examples/mir/abi-reference/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/abi-reference/{before,broken}.mir \
  --function f --world solar-tv/examples/mir/abi-reference/world.json
```

Layout-dependent operations take explicit window inputs in `mir_layout`:

```json
{
  "version": 1,
  "memory": {"size": 192},
  "mir_layout": {
    "code": "0x01020304",
    "functions": {"f": {"frame_base": 256}},
    "constructor": {"offset": 128, "size": 64},
    "data": {"0": {"offset": 1, "size": 3}},
    "immutable_slots": {"0": 256},
    "libraries": {"0": "17"}
  }
}
```

`heap_floor` reads its function's placement and must not exceed the supplied
initial FMP. `internal_frame_addr` and internal frame slots use `frame_base` for
activation zero. Recursive windows instead supply `frame_bases`, including the
first activation, for example `{"g":{"frame_bases":[256,512,768]}}`. The index
counts live activations of that function; tail calls replace an activation.
Missing entries are inconclusive, and `frame_base` cannot accompany a nonempty
`frame_bases` list. Each function admits at most 128 bases. No implicit frame
allocation or initialization is introduced. Constructor ranges reference existing
initial memory. `datasize` reads a deferred entry's linked extent, retaining
addends and alignment; the MIR verifier rejects this operation on literal data.
`codesize`/`codecopy` use the supplied image and zero-pad out-of-range reads.
Scalar immutables alias staging slots, or use `immutable_values` for read-only
runtime relocations. Staging words retain the declared SSA carrier: narrow
integers, booleans and addresses must have zero high bits. Signed runtime values
may also be sign-extended; typed reads project them to the instruction's carrier,
while raw `i256` reads retain the relocation word. Fixed-byte and function values
keep all 256 bits, including dirty fixed-byte padding visible through assembly.
Non-canonical context words are inconclusive. Library relocations retain 160-bit
addresses. Numeric keys are module-local IDs, and declaration tables must agree
across the pair.

Missing inputs stay unsupported. A proof is conditional on these placements and
code bytes; it does not verify backend layout, root reentry or runtime-image
correspondence. Frame/link inputs are recorded in the fixture hash and replayed
independently. Tables have at most 128 entries; code and memory retain the 64 KiB
limit.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/layout/before.mir \
  solar-tv/examples/mir/layout/after.mir --function f \
  --world solar-tv/examples/mir/layout/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/gas-sample/before.mir \
  solar-tv/examples/mir/gas-sample/after.mir --function f \
  --world solar-tv/examples/mir/gas-sample/world.json
```

The GAS example is deliberately a single-opcode window. `gas_before` is the budget
immediately before GAS, which pays its own opcode charge before sampling. Below
that charge it halts exceptionally. This does not price arbitrary MIR bodies or
assume physical call/return costs are zero.

### Direct MIR messages

`call`, `callcode`, `delegatecall`, `staticcall`, `create` and `create2` execute
children from the fixture's bytecode accounts. The MIR caller stays typed SSA;
we do not compile it into an EVM image. The Lean and native interpreters share
neither an imported instruction recipe nor child results. Both execute the real
child, including nested messages, precompiles, exact revert data and rollback.

Typed `address_call`, `address_staticcall` and `address_delegatecall` also accept
memory objects or slices with explicit gas. They reserve no output buffer;
`icall returndata_bytes<>` copies the live return buffer to an uninitialized
bytes object, retaining dirty padding and leaving the buffer available. Implicit
address-call gas is unsupported until caller pricing is modeled.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/address-calls/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/address-calls/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/address-calls/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/address-calls/world.json
```

MIR does not price its own instructions. CALL-family checks therefore assume
enough caller gas to forward the explicit requested amount without EIP-150
clipping. CREATE has no gas operand, so its forwarded child budgets are explicit
scenario inputs, in evaluation order:

```json
{"version":1,"creation_gas":[100000,200000]}
```

A missing budget is unsupported; an unused suffix is allowed. Each child pays
fork-specific EVM gas and can run out of gas. Requested CALL gas and each creation
budget are limited to 30 million. Independent native replay also limits combined
child gas spent to 30 million and child instructions to one million. These checks
prove semantic effects under the recorded boundary conditions; they do not prove
equality of physical caller gas or infer CREATE budgets from backend code.

Return data, access warmth and account creation/destruction flags remain observable
when an internal function returns. Calls replace the full returndata buffer while
copying only the requested output prefix; untouched output bytes survive. Internal
calls share that buffer. Transaction termination clears private frame observations,
while failure restores the parent journal. One model work budget covers both MIR
steps and nested child bytecode; it does not restart when a child returns.

The current MIR contract has no known runtime image. Child reentry into it and
child introspection of its code are unsupported. Fixture children can inspect and
execute their own known code. Amsterdam message gas remains unsupported.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/external-calls/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/external-calls/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/external-calls/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/external-calls/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/creation/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/creation/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/creation/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/creation/world.json
```

The positive pairs are actual `sccp` output. The broken examples call the wrong
account or change a CREATE2 salt; both produce independently replayed witnesses.

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
