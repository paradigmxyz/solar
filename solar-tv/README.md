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
a counterexample result. Concrete replay alone does not establish correspondence
for all inputs. The checked import fragments below also require a Lean certificate
before either a proof or a counterexample can be accepted.

| Layer | Supported fragment | Observation |
| --- | --- | --- |
| MIR scalar projection | One integer function using i1 and integer widths through i256, word and checked arithmetic, casts, select, phis and branches in an acyclic CFG; constant-address memory and Keccak operations. | Returned bits and live memory, exact modeled revert payload, or exceptional halt. |
| MIR direct source | Void/multiple/aggregate interfaces and acyclic calls; memory/storage effects with a proved defined execution over arbitrary caller state. | All returned leaves, live memory, storage, returndata, access warmth and exact modeled outcomes; abstract MIR is unpriced. |
| MIR fresh heap | A constant initial allocation lowered to a named heap-floor reservation; the complete single-block SSA body is checked. | Direct-state effects under an explicit placement/FMP producer condition; neither caller initialization nor backend placement is inferred. |
| MIR compositional effects | Acyclic integer/pointer functions, supplied internal callees, explicit opcode/allocation effects and constructor-range requests. | Equal ordered requests and terminal boundaries for every possible effect result; arbitrary stateful handlers, including failure; scalar steps are unpriced. |
| EVM IR | A block window with symbolic live-ins, word/stack operations, constant-address memory, static transfers and terminal halts. | Outgoing edge, live stack and memory; successful/reverted byte payloads; or execution exception, given sufficient gas. |
| EVM effectful prefix | A scalar/stack rewrite across memory, hash or log operations and static conditional exits, with an identical remaining suffix. | Every original success preserves the exit, complete stack/world and no less gas, for arbitrary gas-independent effect handlers and shared transfer charges. |
| EVM post-precompile suffix | A changed tail after an exactly shared prefix, with canonical recovery-precompile calls and GAS reads retained. | All shared prefix halts are identical; successful suffixes preserve their exit and complete stack/world with no less gas. The shared prefix-executor binding remains trusted. |
| EVM scalar island | An exact-cost stack rewrite between unchanged contexts, including storage sentries, logs and static guards. | All prefix exits, checked stacks and remaining gas are preserved; context-executor bindings remain trusted. |
| EVM source graph identity | Identical independently exported reachable graphs, including stateful operations and transfer stack checks. | Every initial state and execution bound under shared node handlers; structural evidence, without an opcode-implementation or physical gas/layout proof. |
| EVM stack layout | Reordered source nodes and changed zero/one-slot transfer checks, with kernel-checked height bounds. | Every initial height, payload and execution bound under shared schema-shaped handlers; unpriced transfers and explicit dynamic-exit boundaries. |

The compiler parser, source normalizer and operation-to-Lean mapping remain
trusted. Acyclic EVM regions additionally check their imported execution tree
against a separately exported compiler CFG, for every projected live-in state.
Pure acyclic scalar MIR checks its typed arguments, phis, control flow, casts
and checked arithmetic against the direct SSA interpreter. Memory, actual calls,
ABI entries and symbolic checked powers remain outside that certificate.
Audit records identify these certificates
under `import_correspondence`; other fragments retain the importer assumption.
See [proof boundaries and remaining work](CORRECTNESS.md).

### Automatic typed MIR and caller state

When the scalar importer cannot represent a function's interface or effects,
the checker can execute the independently exported typed SSA directly. This
covers void and multiple returns, nested product/slice values, memory/storage
effects and acyclic internal calls. It preserves logical pointer address spaces
and every returned component. No backend lowering or implicit FMP setup is used.

```sh
solar-tv/solar-tv mir \
  solar-tv/examples/mir/universal-state/storage-{before,after}.mir
solar-tv/solar-tv mir \
  solar-tv/examples/mir/universal-state/multiple-{before,after}.mir
solar-tv/solar-tv mir \
  solar-tv/examples/mir/universal-state/aggregate-{before,after}.mir
solar-tv/solar-tv mir \
  solar-tv/examples/mir/universal-state/calls-{before,after}.mir --function f
# A changed storage effect, confirmed by the kernel and native interpreter:
solar-tv/solar-tv mir \
  solar-tv/examples/mir/universal-state/storage-{before,broken}.mir
```

These examples are actual `egraph` before/after pairs. The proof quantifies the
initial journal, memory contents, returndata, typed arguments, caller environment
and block context. Initial memory must be aligned and at most 64 KiB; every
reached operation must remain defined in the model. Acyclic call/CFG analysis
supplies a work bound, capped at 8,192 units. Recursion requires an invariant.

Both executions must complete: identical model errors, undefined results and
work exhaustion cannot establish equivalence. Memory and transient storage stay
observable at normal returns. The proof and its rejection boundary are in
[MIR/Universal.lean](SolarTV/MIR/Universal.lean). Audit records identify direct
source execution as `independent-typed-source-interpreter`; they retain the
trusted Rust schema exporter and constructor-renderer boundary.

Counterexamples use concrete fixtures only after the universal proof fails,
and require matching native replay. A concrete scenario that verifies never
replaces the arbitrary-state proof. Erased pointer/layout preconditions and
unproved pointer witnesses remain unsupported.

### Compositional MIR effects

Large scalar functions can instead prove equality of their effect trees. Every
memory, storage, environment and log operation remains an ordered request with
its opcode and operands. Results are universally quantified. The contextual
theorem in [MIR/Effects.lean](SolarTV/MIR/Effects.lean) preserves every stateful
interpretation, including aliasing and early failure; it assumes neither empty
memory nor successful storage writes.

```sh
solar-tv/solar-tv mir \
  solar-tv/examples/mir/weth-width-guard/{before,after}.mir \
  --function transferFrom --report target/tv/weth.json
# A broken address-width guard, with independent concrete replay:
solar-tv/solar-tv mir \
  solar-tv/examples/mir/weth-width-guard/{before,broken}.mir \
  --function transferFrom --world solar-tv/examples/mir/weth-width-guard/world.json
```

This real `spell-width-tests` pair retains the complete WETH `transferFrom`
body. Its audit says `compositional-effect-tree`, distinct from
`universal-caller-state`. The Rust parser/schema export and effect-tree renderer
remain trusted; this route has no full-state interpreter correspondence proof.
Supplied acyclic internal callees also expand into the tree, with private
arguments/locals and shared effect state. A normal return resumes its caller;
STOP, raw RETURN, revert and exception end the frame. Allocations retain their
size, zeroing and checked-failure policy as explicit requests. Results are
arbitrary pointer carriers: no allocation success, fresh address, implicit FMP
setup or backend return-buffer placement is assumed. Pointer offsets wrap; they
establish no validity or provenance proof. Constructor base/end queries remain
distinct ordered requests, with arbitrary results and no selected placement.
External calls, tail calls, other layout queries, GAS sampling and code observers
are rejected by this route.

The `lower-alloc` example is reduced from Morpho's constructor. Its FMP read
becomes `mload(0x40)` followed by a pointer-carrier cast. The request constructors
in [MIR/Transforms/LowerAlloc.lean](SolarTV/MIR/Transforms/LowerAlloc.lean) have
kernel-checked primitive correspondence, including memory expansion and failure.
The constructor range remains unknown; no default FMP or heap floor is supplied.
This is still a compositional interaction certificate with a trusted SSA
projection, rather than a whole-contract or physical-gas proof.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/fmp-lowering/{before,after}.mir \
  --function constructor --report target/tv/fmp-lowering.json
# A wrong FMP slot, with matching Lean and original-MIR Rust replay:
solar-tv/solar-tv mir solar-tv/examples/mir/fmp-lowering/{before,broken}.mir \
  --function constructor --world solar-tv/examples/mir/fmp-lowering/world.json
```
SSTORE stays a request that can fail, but changes in physical scalar gas costs
and their effect on its gas sentry are outside this unpriced MIR certificate.
Undefined operands and unmodeled operations cannot be discarded as dead lets.

The reduced WETH `DOMAIN_SEPARATOR` SCCP pair keeps its allocating `name`
helper. Its audit records `independent-call-effect-tree`, two supplied functions
and one allocation site:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/domain-separator/{before,after}.mir \
  --function DOMAIN_SEPARATOR --report target/tv/domain-separator.json
solar-tv/solar-tv mir solar-tv/examples/mir/domain-separator/{before,broken}.mir \
  --function DOMAIN_SEPARATOR --world solar-tv/examples/mir/domain-separator/world.json
```

The concrete FMP fixture confirms the broken helper in both Lean and Rust; it is
never a precondition of the positive interaction proof. Call expansion is
limited to depth 32, renderer nesting 128, 4,096 work units and 256 KiB of text.
Unsupported instructions in supplied bodies are checked even when unused.

### EVM stack rewrites across effects

When a shared prologue uses symbolic memory addresses, a projected heap cannot
represent it. A separate interaction proof can retain those accesses instead:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/log-stack/{before,after}.evmir \
  --prefix bb0 --report target/tv/log-stack.json
```

This reduced Morpho `peephole` pair removes two swaps around `LOG4`. The theorem
quantifies every incoming stack, including underflow and the 1024-word limit,
and every memory/log handler, including aliasing, faults and dynamic charges.
Each request retains its opcode and ordered operands. Scalar charges may only
decrease between successive effects. A kernel-checked expansion theorem binds
the interaction tree to instruction-by-instruction charging.

The audit records `universal-effectful-stack-prefix` and gas policy `refine`.
Every original success must keep the full stack and world with no less gas.
The suffix, terminal and physical successor must match, and the continuation
must be gas-insensitive. Exact-gas mode is rejected rather than weakened.
The schema-to-operation renderer, concrete handler correspondence and logical
relocation context remain trusted. This is not a full-state bytecode proof or a
physical placement certificate. Calls, unpaired or dynamic interior transfers,
code/GAS observers and storage writes are excluded. A broken log-topic
test is independently replayed in Lean and Rust using a concrete entry.
Amsterdam's two-resource gas is outside this route and is rejected explicitly.

Adjacent static `push bbN; jumpi` pairs also retain early exits. The proof checks
both outcomes, the live stack and the temporary label slot, including stack
overflow before `JUMPI`. The Morpho decoder's real `block-cse` rewrite reuses its
base pointer after four guards:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/decoder-branches/{before,after}.evmir \
  --window bb0 --stack-height 2 --report target/tv/decoder-branches.json
```

The fallback certificate quantifies every stack height; `2` is recorded only as
the requested window height. The audit names it
`universal-branching-effect-prefix`. Logical labels carry no byte offsets: a
shared, universally quantified per-label context supplies the physical PUSH and
JUMPI charges, including PUSH0. Binding that context to relocated bytecode
remains trusted. A changed exit is rejected, and a broken decoder store offset
has a counterexample independently checked by Lean and the Rust interpreter.

### EVM rewrites after precompiles

WETH's predicate peephole replaces `EQ; ISZERO` with `SUB` after a recovery
precompile call. The proof retains the full common prefix, including GAS, call
failure, memory and returndata, rather than restarting with entry defaults:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/post-precompile/{before,after}.evmir \
  --prefix bb0 --report target/tv/post-precompile.json
solar-tv/solar-tv evmir solar-tv/examples/evmir/post-precompile/{before,broken}.evmir \
  --world solar-tv/examples/evmir/post-precompile/world.json
```

`universal-post-precompile-suffix` composes an arbitrary shared state/gas
executor with the universal suffix proof. Both sides pass it the same complete
instruction sequence. Every early prefix halt is preserved exactly; after a
successful prefix, the proof quantifies the updated context, stack, remaining
gas and world. The audit binds the prefix hash and instruction count. Its
executor binding remains trusted; this is not full-state interpreter or physical
code-placement correspondence.

Admission is deliberately narrow: prefix calls must use the canonical
`push 1; gas; staticcall` recovery target. Arbitrary external calls, relocations
and code-image observers are rejected. The changed suffix and its continuation
must be gas-insensitive. Exact-gas mode and Amsterdam's two-resource gas are
rejected. The broken predicate has matching Lean/native counterexamples, and
the tests also check exact preservation of an early out-of-gas outcome.

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
uv run --locked --all-packages pytest -n 2 --dist worksteal solar-tv/tests
```

The tests regenerate the example outputs with real compiler passes. Two proof
workers bound concurrent memory use. The `solar-tv` CI job builds the Lean
proofs, runs these tests for both IRs, and checks that an
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

MIR state validation compares semantic effects without pricing abstract
instructions. Explicit `message_gas` budgets price message boundaries as described
below; root `--gas` and fixture `gas` budgets remain unsupported. Direct internal
calls share memory and state while keeping separate, width-typed SSA locals. Tail calls replace the current activation. Revert,
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

SHA-256, RIPEMD-160 and address recovery use `extcall static word`, with explicit
input staging, failure checks and scratch reads. Both successful and out-of-gas
calls run the actual precompile in Lean and independent replay. The requested
child gas is an explicit call operand, or uses the ordered caller budgets in
`message_gas` for implicit forwarding. Caller budgets are shared across internal
calls and remain observable, including at terminal boundaries. The removed
builtins' `builtin_gas` option is rejected rather than silently ignored.

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
Physical call-return buffers need the explicit convention above.

### ABI canonicality obligations

`validate_abi uint8, value` records the required representation explicitly. The
checker proves it at the reached program point, including through phis and calls.
Rules cover unsigned widths, sign extension, fixed-byte padding, booleans and enum
ranges. A false obligation is inconclusive; it is never an assumed fact or a
fabricated runtime revert. Raw external-function padding uses `bytes24`; its
normalized value uses `uint192`. Checking the normalized value cannot justify
removing the raw padding check.

The kernel-checked discharge rule is in
[MIR/Transforms/AbiValidation.lean](SolarTV/MIR/Transforms/AbiValidation.lean).
This example runs actual ABI conversion passes. Its `i256` input is an
unrestricted raw function word: the kernel-checked proof covers every value,
including malformed padding. The proof keeps typed boolean intermediates and
branches intact and discharges the normalized `uint192` obligation.
Its broken version skips the function-word padding rejection:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/abi-validation/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/abi-validation/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/abi-validation/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/abi-validation/world.json
```

Both interpreters replay a counterexample: malformed input reverts before the
rewrite but returns successfully afterward.

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

Typed `extcall`, `extcall static` and `extcall delegate` accept optional memory
objects or slices. An absent input is empty and reads no object header. The
default output range is empty; `word` selects bytes `0..32` of scratch memory,
clearing them before the call except for constant SHA-256 and RIPEMD-160 callees. Explicit-world CLI checks remain limited
to Istanbul through Osaka and Amsterdam's supported environment fragment.
`icall returndata_bytes<>` copies the live return buffer to an uninitialized
bytes object, retaining dirty padding and leaving the buffer available. With
`message_gas`, an omitted gas option forwards the EIP-150 cap after the boundary's
charges, including the value stipend. An explicit zero still requests zero gas.
Without caller budgets, omitted gas is inconclusive. Pre-EIP-150 implicit gas
needs instruction pricing and remains unsupported.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/address-calls/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/address-calls/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/address-calls/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/address-calls/world.json
```

MIR does not price abstract instructions. To check physical gas **at each message
boundary**, supply ordered caller budgets just before the opcode's charges:

```json
{"version":1,"message_gas":[6400,40000]}
```

These budgets pay memory expansion, warm/cold account access, value transfer and
new-account charges. CALL requests are capped by EIP-150, with the value stipend
added to the child budget. CREATE/CREATE2 derive their child budget after base,
initcode and hashing charges. Unused child gas returns to the caller; child
reverts and exceptional failures retain their distinct gas behavior. Insufficient
upfront gas terminates the caller and rolls back its journal. EIP-7702 resolution
is charged before forwarding. Amsterdam's message gas remains unsupported.

Remaining gas at completed boundaries is observable, including through internal
calls. A proof in this mode requires equal boundary gas observations; it does not
infer instruction prices between those explicit boundaries or claim a whole
MIR body's physical gas equivalence. Missing budgets are unsupported, an unused
suffix is allowed, and an explicit empty list never enables a fallback.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/message-gas/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/message-gas/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/message-gas/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/message-gas/world.json
```

The implicit-gas example uses a real `sccp` pair and a child that returns its
`GAS` reading. Its broken rewrite adds a 100-gas option; both Lean execution and
independent native replay observe 98 instead of 3,739:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/implicit-gas/{before,after}.mir \
  --function=f --world solar-tv/examples/mir/implicit-gas/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/implicit-gas/{before,broken}.mir \
  --function=f --world solar-tv/examples/mir/implicit-gas/world.json
```

Without `message_gas`, CALL assumes sufficient caller gas to avoid clipping.
CREATE instead needs explicit ordered child budgets, for example
`{"version":1,"creation_gas":[100000,200000]}`. The two modes cannot be combined.
Precompiles use the same `extcall` gas operands and caller budgets as other calls.
Budgets are limited to 128 entries and 30 million gas per entry. Independent
native replay also limits combined child gas spent to 30 million and child
instructions to one million. Child bytecode always pays fork-specific gas.

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
complete changed blocks with proven fixed incoming stack heights, or the
universal prefix and terminal-region proofs described below. Unrelated
dispatchers, helpers and successor bodies do not have to fit the modeled fragment.
The compiler still parses and validates the full module, and Rust counterexample
replay executes the selected region in that original context.

Reports identify the function or stable block label and incoming height. Both
module and region hashes are retained. A verified region is a local proof, not a
claim that the entire pass or contract was verified (`whole_pass: false`). ABI and
function interface changes, new/deleted functions and shared declaration changes
remain unsupported. Unknown entry heights require a universal proof. When
successors or adjacent blocks change, the hook selects the larger region
described below instead of assuming that stack positions correspond at the old
window boundary. This allows shared revert code and stack reshuffles spanning
several blocks.

You can select a region when replaying full snapshots:

```sh
solar-tv/solar-tv mir before.mir after.mir --function helper
solar-tv/solar-tv evmir before.evmir after.evmir --window bb7 --stack-height 3
```

The standalone window proof assumes the specified incoming height. The pass hook
derives it from the original and edited CFGs; a maximum bound is insufficient.
Successor labels denote the same boundaries on both sides. No instructions are
cut out of a block. Single-block windows do not admit layout changes or shared
tails; the multi-block mode below follows their successors instead. Structured
transfers remain unsupported under gas policies.
Full context is limited to 1 MiB per file. Selected windows and MIR functions keep
the 128 KiB and 512-line limits; multi-block expansion has the budgets below.
Unrelated EVM bodies are not copied into the proof.
For a complete physical `JUMPI; STOP` block, the untaken path executes `STOP`;
it is not an invented edge to the next block.

### Multi-block EVM IR regions

Use `--region` to execute an entire reachable acyclic CFG through its terminal
outcomes. Internal labels may disappear or be added, shared tails execute with
their incoming stack and memory, and stack reshuffles may cross block boundaries.
Only the entry height must agree. Terminal success/revert bytes and exceptions
are observable; intermediate stack positions are not artificial outputs.

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/cfg-region/{before,after}.evmir \
  --region bb0 --stack-height 0
solar-tv/solar-tv evmir solar-tv/examples/evmir/tail-region/{before,after}.evmir \
  --region bb0 --stack-height 0
solar-tv/solar-tv evmir solar-tv/examples/evmir/tail-region/{before,broken}.evmir \
  --region bb0 --stack-height 0
```

These are real `cfg-simplify` and `tail-merge` outputs. The broken shared tail
returns 23 where the original returns 22; the Lean kernel checks that witness
and the native interpreter independently follows the original modules' CFGs.
Reports retain the entry, height and before/after block sets, alongside replay
commands and `whole_pass: false`.

The pass hook selects regions when labels, successors or adjacent blocks change;
isolated edits can still use smaller windows. Unrelated unreachable blocks need
not fit the model. Manual entry heights are explicit assumptions; the hook proves
a fixed height in both original and edited CFGs. Internal joins may have different
heights on different paths. Physical conditional continuations and structured
jump fallthroughs retain the temporary stack growth of their lowering.

[Region.lean](SolarTV/EVMIR/Region.lean) composes the existing instruction semantics;
it does not add an instruction interpreter or obtain observations from native
replay. The importer expands at most 256 block visits, 2,048 instructions and
128 blocks per path. A separate path admits pure cyclic regions whose independently
exported source graphs are definitionally equal. It proves agreement for every
execution bound, without assuming termination or cutting a loop into a fake exit:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/cyclic-region/{before,after}.evmir \
  --region bb0 --stack-height 1
```

This example is a real `cfg-simplify` removal of unreachable code. Nonisomorphic
loops still require an invariant and remain unsupported. This direct-interpreter
route excludes dynamic or escaping labels, layout-observing instructions,
stateful operations and priced gas regions. Memory uses the existing constant-access
256-byte projection. No whole-pass or arbitrary-world claim follows from a
verified region.

### Identical reachable source graphs

An instruction unsupported by the region interpreter need not block a pair
whose independently exported reachable graphs are exactly identical:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/graph-identity/{before,after}.evmir \
  --region bb0 --stack-height 0
solar-tv/solar-tv evmir solar-tv/examples/evmir/graph-identity/{before,broken}.evmir \
  --world solar-tv/examples/evmir/graph-identity/world.json --gas exact
```

The example is a real `block-layout` pair. The kernel checks the entry, node
labels, every operation and operand, terminal, edge and transfer stack-growth
check. `EVMIR/SourceGraph.lean` proves equality for every initial state and every
execution bound under the same node interpreter, including early failures and
stateful cycles. An unchanged context read is retained, never replaced by zero.
Changed operations, branches or transfer checks require a different proof.

Reports distinguish `universal-source-graph-identity` and `unpriced-structural`
from the direct interpreter certificates. The Rust parser/CFG normalizer and
source-data constructor binding remain trusted. This proves structural source
identity, not a new opcode implementation or physical transfer-pricing theorem.
Code-image observers and calls remain excluded. Only an unsupported instruction
selects this fallback; failed import certificates and actual counterexamples
are never replaced. The broken store has a separate kernel/native scenario
counterexample. Live pass auditing still leaves emitted bytecode unchanged.

### Stack-safe block layout

Block layout can reorder nodes and add or remove a temporary jump-label push.
The separate `EVMIR/StackGraph.lean` simulation retains those different stack
checks and proves them equivalent using a checked upper height at every node:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/stack-layout/{before,after}.evmir \
  --region bb0
solar-tv/solar-tv evmir solar-tv/examples/evmir/stack-layout/{after,broken}.evmir \
  --world solar-tv/examples/evmir/stack-layout/world.json --gas exact
```

The kernel checks the node mapping, opcode signatures, control edges and local
height bounds. It quantifies every initial height, including overflow, arbitrary
live payloads and every execution bound. A caller-supplied `--stack-height`
cannot establish this result. The payload retains values, memory and account
state; shared opcode handlers may change it or fail. Binding those handlers'
height effects to the concrete opcode implementation remains trusted.

Reports name `kernel-checked-schema-stack-layout`, retain the mapping and
height certificate, and use `unpriced-structural`. Physical jump charges can
change; this route does not prove gas equality. A raw dynamic `JUMP` is a
full-payload exit under a shared logical continuation, not a resolved target or
a proved physical return address. Code-image observers and calls remain
excluded. Changed operations or unsafe transfer growth remain unsupported.

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

### Block prefixes with unknown incoming stacks

`--prefix` checks an effect-free prefix of a complete EVM IR block for **every**
incoming stack length and value, including underflow and the 1024-word limit:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/prefix/{before,after}.evmir \
  --solar target/debug/solar --prefix bb0 --report target/tv-prefix.json
```

The example is a real `block-cse` rewrite from WETH: the third constant push
becomes `DUP1` before `LOG3`. The Rust source exporter supplies the complete
block. Every instruction after the scalar prefix, the terminal and the physical
successor must match. Memory, logs and storage stay in the arbitrary live world;
this proof assumes no empty state or fixed memory prefix.

[Prefix.lean](SolarTV/EVMIR/Prefix.lean) proves that instruction-by-instruction
charging agrees with the prefix observation. Equal-cost prefixes preserve every
gas budget and a gas-sensitive continuation. Cheaper prefixes carry an explicit
resource-refinement policy and require sufficient gas and a gas-insensitive
continuation. `--gas=exact` rejects such a cheaper rewrite. Code observers and
calls require further correspondence and remain unsupported here.

The pass hook uses this mode for unknown-height block windows. Capture retains
its original rejection and region identity; evaluation chooses the proof mode
**after** sampling. A failed universal proof is unsupported, since a hypothetical
stack need not be reachable. It never fabricates a compiler counterexample.

### Exact scalar rewrites inside stateful blocks

`--prefix` also checks a scalar island after an unchanged gas-sensitive prefix:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/interior-cse/{before,after}.evmir \
  --prefix bb0 --report target/tv-interior.json
solar-tv/solar-tv evmir solar-tv/examples/evmir/interior-cse/counterexample-{before,broken}.evmir \
  --world solar-tv/examples/evmir/interior-cse/world.json --gas exact
```

The reduced WETH example runs storage updates and a static guard before
`block-cse` replaces a repeated `push 32` with `dup 1`. The checker retains the
complete shared prefix and suffix, terminal and physical successor. Its
`universal-exact-scalar-island` certificate proves the middle for every stack
and gas budget. Prefix exits remain exits; successful prefixes supply their
actual context, stack, gas and live world. The suffix may observe gas or fail a
storage sentry, so cheaper rewrites are rejected rather than given a weaker
gas-insensitivity assumption.

`EVMIR/Suffix.lean` lifts the middle's checked-stack and exact-price proof through
both shared contexts. Their parser/schema and executor bindings remain trusted;
this is a local compositional certificate, not full-state importer or physical
code-placement correspondence. Code-image observers, external calls, broken
static guards and Amsterdam's two-resource gas remain unsupported. The small
broken-store example has a kernel-confirmed counterexample matched by Rust
replay. The full storage/log fixture is also replayed independently, including
static violations and out-of-gas before the island.

### Terminal regions with unknown incoming stacks

Complete acyclic terminal regions can also quantify every incoming stack:

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/tail-region/before.evmir \
  solar-tv/examples/evmir/tail-region/after.evmir --region bb0
```

Omitting `--stack-height` proves all lengths and values, including underflow,
the 1024-word boundary and the temporary label push of a concrete transfer.
Memory contents and active size remain arbitrary in the constant-access memory
projection. Each side retains its independent source-CFG certificate and an
entry theorem for the actual universally quantified execution. Gas-priced
regions, unknown addresses, dynamic transfers and cycles remain unsupported.
The pass hook and sampler use this route for unknown-height terminal regions;
capture identities stay unchanged. Failed universal checks stay unsupported,
because their hypothetical entry states need not be reachable.

These proofs also admit `CALLER` and `CALLDATALOAD` over arbitrary caller
addresses and calldata bytes. Calldata reads preserve big-endian byte order and
zero extension, and do not expand memory. The caller is widened from 160 bits.
Size words are independently quantified, so the proof covers a conservative
superset of real frame contexts. Regions that consume incoming words split out
short stacks explicitly; underflow and transfer overflow remain observable.
Fixed scalar-window inputs do not include these context values and still
reject the operations rather than substituting zero.

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/caller-tail/{before,after}.evmir \
  --region bb1
solar-tv/solar-tv evmir solar-tv/examples/evmir/calldata-prefix/{before,after}.evmir \
  --prefix bb0
solar-tv/solar-tv evmir solar-tv/examples/evmir/calldata-prefix/{before,broken}.evmir \
  --world solar-tv/examples/evmir/calldata-prefix/world.json
```

### Rewrites after a shared memory prologue

A changed scalar prefix can follow identical memory operations. The checker
executes this shared prologue with its memory effects, dynamic/expansion charges
and early faults, then proves the scalar rewrite for arbitrary resulting stacks.
Append and scalar correspondence lemmas connect the two stages to sequential
execution. Initial memory is never fixed to zero.

Equal-price rewrites preserve every gas budget. Cheaper rewrites require enough
gas for the scalar segment after a successful prologue and a gas-insensitive
continuation; failure within the shared prologue remains observable. Reports use
`universal-memory-prefix` and record the shared operation count. Windows missing
caller/calldata inputs can use this stronger route without guessing those inputs
or the entry height. Other unsupported cases remain explicit.

```sh
solar-tv/solar-tv evmir solar-tv/examples/evmir/memory-prologue/{before,after}.evmir \
  --window bb1 --stack-height 1
solar-tv/solar-tv evmir solar-tv/examples/evmir/memory-prologue/{before,broken}.evmir \
  --world solar-tv/examples/evmir/memory-prologue/world.json
```

### Removing unreachable MIR functions

`--prune` checks deletion-only function-table changes. It preserves every
external entry, requires every surviving declaration to be identical, and checks
that all its named call targets survive. The kernel theorem covers calls,
tail calls, saved return continuations and terminal observations for every step
bound, including recursive calls and CFG cycles. It does not unroll loops to a
fixed depth or claim that a removed function remains callable.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/function-dce/{before,after}.mir --prune
solar-tv/solar-tv mir solar-tv/examples/mir/function-dce/{before,broken}.mir \
  --function f
```

The first pair comes from the real `function-dce` pass. The second changes a
surviving result and produces a counterexample checked by Lean and Rust replay.
The live pass adapter automatically tries the table certificate for removed or
renumbered functions; capture-only records keep their original selection.

Reports call this `rooted-function-table-correspondence`, with module scope and
an `unpriced-structural` gas policy. Primitive instruction semantics and state
are shared abstract parameters. The compiler parser/display, named-reference
extraction and binding to this call machine remain trusted. This is not an
instruction-semantics, physical gas or backend-layout certificate. Numeric
function-pointer dispatchers, backend-memory records, changed surviving bodies
or globals, and modules without explicit entry points are unsupported. Each
module is limited to 1 MiB and 128 functions.

### Removing dead SSA definitions in MIR loops

Function checks can prove deletion-only DCE across cyclic CFGs:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/cyclic-dce/{before,after}.mir --function f
solar-tv/solar-tv mir solar-tv/examples/mir/cyclic-dce/{before,broken}.mir \
  --function f --world solar-tv/examples/mir/cyclic-dce/world.json
```

The first pair is real `dce` output. The checker admits total scalar operations,
slice constructors and well-typed slice projections. The kernel checks exact
erasure and confirms that retained instructions, phi groups and transfers do
not read any erased result. A simulation preserves
live registers and shared state after every block, for every block-transfer
bound. Loops need no fixed unrolling or termination assumption. Effect handlers
are arbitrary and may fail immediately. The broken store has matching Lean and
independent Rust counterexamples.

Reports identify `cyclic-dead-ssa-simulation`, function scope and
`unpriced-structural`. This is a contextual action-machine certificate, distinct
from direct typed-interpreter correspondence. The compiler parser/schema export,
injective local-ID normalization, total-pure classification and action renderer
remain trusted. Retained callees must be unchanged and cannot re-enter the
changed function. GAS, code observers, external calls, placement queries and
backend memory records remain unsupported. Physical scalar gas and its effect
on SSTORE's gas sentry are outside the claim.

All six previously cycle-rejected DCE pairs in the broader capture set now
verify. They cover OpenZeppelin's storage-byte helper, `name` and `symbol`, and
three Morpho `extSloads` captures.

### Checking builtin lowering

For the exact storage-byte write and address-call shapes, `--function` checks
the typed SSA expansion and its required callee bodies before applying the
Lean state proof. The storage check covers header and extent panics, ordered
clearing, complete words and dirty partial words within the storage/memory
model's work limits. The call check preserves the observed CALL and returndata
effects with a shared GAS sample and caller budget at the call boundary.
Reports state those conditions and identify the certificate. See
[the proof boundaries](CORRECTNESS.md#remaining-obligations).

### Validating Morpho's CALL and caller exits

```sh
solar-tv/solar-tv call-budget --output target/morpho-call-budget
```

The command captures the `safeTransferFrom` MIR pair and scheduled EVM IR in
the compilation that emits the runtime. The proof derives the prepared frame
and exact CALL budget from helper entry, runs the child with that budget, then
matches the final continuation to both MIR versions. It covers empty and true
returns, false returns, short or noncanonical ABI data, and child reverts.

The proof follows returns into the emitted continuations for `supply`,
`repay`, `supplyCollateral`, `liquidate` and `flashLoan`. It compares
their ABI output and remaining gas, and checks that a revert rolls back the
owning invocation with the exact payload. It starts with the live caller
values at helper entry.

The helper replay runs the emitted CALL and continuation with only local jump
destinations relocated. It checks copied memory, revert bytes and the journal.
In the success/revert pair the token writes a slot before responding. The
successful call keeps the write and the revert rolls it back. Other pairs
check empty, malformed, short and false results. The existing gas and dirty
memory replays are also saved. Under `callers/`, the Rust replayer starts
from all five real ABI selectors and executes the unchanged runtime against
success, token revert and false-return fixtures. It checks return words,
storage and log rollback.

Add `--check-caller-entry` to certify five complete ABI invocations in Cancun.
The Lean kernel checks execution from PC 0 in the original runtime and from
ABI decoding in both typed MIR snapshots. It checks the helper live-ins and
FMP, then resumes each reached frame through the actual helper and caller
instructions. The final proof compares terminal status, ABI bytes, accounts
and logs. MIR uses the gas samples and message budgets from the checked
bytecode execution. The exact physical gas and refund are separately checked
against the Rust replay.

Use `--caller-worlds DIR` to supply concrete calldata, accounts, gas and
child code. Copy the `*.world.json` files saved under `entry/` and edit the
five inputs. Each file must keep its caller's selector and reach the checked
helper. The proof covers the supplied invocations. Children that depend on
the MIR root's runtime code cause an unsupported result.

Short execution segments and reusable bytecode decoding and Keccak proofs
keep the checks within the per-process memory limit. The saved files under
`entry/` include the checked segments and independent replays. From this
directory, recheck all five proofs with the compiled modules on the search path:

```sh
kernel=/absolute/path/to/call-budget/entry/kernel
LEAN_PATH="$kernel${LEAN_PATH:+:$LEAN_PATH}" \
  lake env lean -j1 --memory=2048 "$kernel/MorphoAbiSuite.lean"
```

Use `--mir-pair BEFORE AFTER` to verify an existing capture. The output
includes the runtime, the Lean certificates, `mir.json` and the independent
replays under `continuation/` and `callers/`. [Proof details](CORRECTNESS.md#exact-emitted-call-gas)

### Removing unused MIR type declarations

`--prune` also checks complete named-type table erasure when every function,
global, phase and retained source line stays unchanged. Both source modules are
independently parsed and validated, with named types resolved into structural
parameter, result, operand and operation types. The kernel compares the complete
resolved function tables and retained text:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/type-pruning/{before,after}.mir --prune
solar-tv/solar-tv mir solar-tv/examples/mir/type-pruning/{after,broken}.mir \
  --function f
```

The first example is real `lower-structs` output. The second has a wrong store,
with a counterexample checked by Lean and independent Rust replay. Changing
retained code, globals, phase, declaration order or resolved type identity is
rejected. Partial type-table rewrites and simultaneous function deletion are
outside this certificate.

Reports use `resolved-type-table-erasure`, module scope and
`unpriced-structural`. The theorem quantifies the interpreter, complete state
and every execution bound. Cycles, calls and GAS nodes remain data; no operation
is executed or assigned a default value to establish identity. A separate
`--mir-module-model` source export retains every function, including unreachable
bodies. It cannot select a function or relax normal replay's GAS restriction.
The parser/schema exporter and typed constructor renderer remain trusted;
primitive instruction implementations, physical gas and backend layout are
not certified. Limits are 128 functions, 512 blocks, 4,096 instructions, 65,536
type nodes and 64 KiB of exported constant data.

Live audits automatically try this module certificate after a shared-declaration
change. Unsupported edits still fail the checker; the adapter never drops an
arbitrary header or phase change. Capture-only records retain their original
rejection and selection, so the frozen corpus denominator stays unchanged.

### Allocation prefixes before unchanged cyclic callees

An allocation-coalescing constructor can finish all changed instructions before
entering an unchanged storage helper. The checker validates that cut rather
than unrolling the helper's loops. Its byte-memory theorem preserves every
initialized byte, final allocation cursor and active memory size. Ordered calls
and their arguments must agree. Every reachable callee must be identical, and
the continuation cannot call back into the changed constructor. A continuation
may fail, loop or mutate state; successful termination is never assumed.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/coalesce-allocs/{before,after}.mir \
  --function constructor
solar-tv/solar-tv mir solar-tv/examples/mir/coalesce-allocs/{before,broken}.mir \
  --function constructor --world solar-tv/examples/mir/coalesce-allocs/world.json
solar-tv/solar-tv mir solar-tv/examples/mir/allocation-lowering/{before,after}.mir \
  --function constructor
solar-tv/solar-tv mir solar-tv/examples/mir/allocation-lowering/{before,broken}.mir \
  --function constructor --world solar-tv/examples/mir/allocation-lowering/world.json
```

This is explicitly a **conditional prefix certificate**. The allocation
producer must establish an aligned FMP of at least 128, represented by the word
at `0x40`, and a nonwrapping total extent below `2^64`. Active memory must be
aligned and include that word. The cursor and byte contents remain arbitrary;
the checker never supplies an initial cursor. Proving every caller establishes
the contract remains separate work. Only infallible, uninitialized reservations
and aligned constant word stores inside the current reservation are admitted.
`lower-alloc` may replace the first reservation with an uninterrupted
`mload 64`, pointer cast, constant addition and `mstore 64` sequence. A separate
kernel lemma in `MIR/Transforms/LowerAllocPrefix.lean` connects that sequence to
the reservation under the same producer contract, including word nonwrapping
and active memory. Reports record `lowered_reservations` on each side. Only one
initial lowered reservation is admitted; intermediate FMP reads, later
reservations on that side, checked/zeroed allocations, changed helpers and
changed call arguments are rejected. `heap_floor` is never substituted for FMP.

Reports distinguish `producer-valid-allocation-prefix` from direct interpreter
proofs and use `unpriced-structural`. The compiler parser/schema export,
SSA-to-byte-memory projection and continuation binding remain trusted. Byte
writes agree with the shared byte-list memory operation in the active range;
the kernel also checks FMP read-over-write and disjointness laws. This does not
certify physical gas, backend placement or full SSA import correspondence. The
broken initializer and wrong lowered FMP store have counterexamples checked by
Lean and Rust replay.

This is a function-local claim: bodies outside the constructor's transitive
call graph can change, and reports list them as `unrelated_changed_bodies`.
They are not certified by this check. Shared declarations, function interfaces
and table order must still agree; numeric function-pointer dispatchers remain
unsupported.

### Fresh heap-floor reservations

`lower-alloc` may replace an initial FMP read with `heap_floor`. Those are not
equivalent over arbitrary caller memory: the named placement must agree with
the live FMP. The checker exposes that producer obligation and proves the
complete imported SSA body, including every subsequent write and return:

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/fresh-heap/{before,after}.mir \
  --function name.body
solar-tv/solar-tv mir solar-tv/examples/mir/fresh-heap/{before,broken}.mir \
  --function name.body --world solar-tv/examples/mir/fresh-heap/world.json
```

Reports name `producer-valid-fresh-heap-ssa` with `semantic-effects`. The theorem
quantifies arbitrary live memory, journal, returndata and context satisfying
the explicit condition: an aligned named floor of at least 128 equals the word
at `0x40`, that word is active, and the source reservation fits the interpreter's
64 KiB memory-work domain. No zeroed heap or particular cursor is supplied.
Every accepted source extent has valid frames; an impossible work-range
condition cannot establish a vacuous success.

`MIR/Transforms/FreshHeap.lean` proves the reservation's byte-memory update and
that an explicit entry write establishes the cursor condition. The pair proof
runs the actual imported typed SSA functions. It does not summarize their
suffixes, accept equal model errors or assume that later operations succeed.
The parser/schema export and typed constructor renderer remain trusted.
Establishing the condition at every caller and connecting placements to the
backend remain separate obligations; physical gas is not certified. Tests
include dirty heaps at several cursors, changed suffixes and returns, missing
and disagreeing bindings, independent counterexample replay and unchanged
compiler bytecode.

### Reproducible real-contract evaluation

Capture changed pass pairs without running the prover, then check a deterministic
sample from pinned Solady, OpenZeppelin and Morpho source archives:

```sh
target/debug/solar contract.sol -Ogas --emit=bin \
  -Zsolar-tv=target/tv-capture -Zsolar-tv-capture
solar-tv/evaluate --limit 8 --timeout 30 --seed 0 \
  --output target/tv-evaluation
```

Capture is explicitly unchecked: `captures/*/regions.jsonl` records `captured`,
never `verified`, and the normal audit summary does not count it as a proof.
Module pairs are stored once per pass and shared by their region records.

The evaluation compiles each contract normally and with capture, requires
identical ABI and bytecode outputs, and records compiler, replay, source and
semantics fingerprints. Selection alternates IR layers and passes, with seeded
hash ordering inside each group. Rejected and unsupported regions stay in the
denominator. `results.json` separates selected and unselected checks, every
status and each case's output identity. Use a fresh output directory for each run.
CI retains the reports and full before/after pairs as an artifact.

The same eight-check-per-contract sample now verifies **17 of 24** with
`--timeout 30`; 7 remain unsupported, with no counterexamples, timeouts or
errors. Selected check IDs and all three output fingerprints are unchanged.
Four additions are complete terminal paths formerly rejected for unknown stack
heights: WETH block layout, ERC20Mock tail merge and loop layout, and Morpho tail
merge. The ERC20Mock `spell-width-tests` pair also verifies over arbitrary caller
state, including its unchanged storage/logging helper.
ERC20Mock's layout of a custom-error path also verifies over its incoming stack
and caller address.
Morpho's calldata-expression reordering now verifies after its shared memory
prologue, using the same checked source window and a charged composition proof.
WETH's `transferFrom` width guard now verifies in about 1.2 seconds with a
compositional effect-tree certificate; this is not a full-state execution proof.
Morpho's swap cleanup around `LOG4` now also verifies with the effectful stack
prefix certificate, without assuming a stack height or constant memory addresses.
Its decoder's `block-cse` pair now verifies through the four conditional exits,
preserving the same effect requests and live stack on every taken edge.
WETH's `DOMAIN_SEPARATOR` SCCP pair also verifies through its allocating helper
using the separately reported compositional call/effect certificate.
Its post-precompile predicate peephole also verifies, with all 103 shared
prefix instructions bound into the contextual suffix certificate.
WETH's deletion of two unreferenced token-transfer hooks now verifies with the
separately reported rooted function-table certificate described above.
ERC20Mock's allocation-coalescing constructor now discharges the conditional
prefix obligation described above. This result includes its explicit allocation
producer contract; it does not establish that contract at every caller.

The original ten-second budget verifies **16 of 24**, with that larger MIR proof
still timing out. It closes in about 25 seconds without increasing Lean's
heartbeat limit. Shared source tables, delayed result observation and canonical
width-guard lemmas avoid unnecessary symbolic expansion. The reduced pairs and
a broken-width counterexample are in `examples/mir/width-guard/` and
`examples/mir/erc20-width-guard/`. The counterexample is independently replayed;
its concrete fixture never establishes the positive universal result.

After rebasing onto `main`, a fresh 24-check sample starts at 15 verified and
9 unsupported. Rechecking those frozen pairs verifies **24 of 24** with
no unsupported results, errors, timeouts or counterexamples. The final two
checks cover OpenZeppelin's storage-byte writer and Morpho's CALL and
returndata lowering. The certificates carry the storage model limits and
the shared gas-boundary conditions described above. The same sample also
checks WETH's fresh-heap producer condition and Morpho's resolved function
graphs after unused type declarations are removed.
Selected check identities and compiler-output fingerprints are unchanged;
plain outputs from the updated validation compiler match all three saved
contract fingerprints. This uses a new
compiler/capture fingerprint; it is not comparable to the older 17-of-24 selection above.
See [CORRECTNESS.md](CORRECTNESS.md#remaining-obligations) for the fingerprints.

```sh
solar-tv/solar-tv mir solar-tv/examples/mir/erc20-width-guard/{before,after}.mir \
  --function burn --timeout 30
solar-tv/solar-tv mir solar-tv/examples/mir/width-guard/{before,broken}.mir \
  --function f --world solar-tv/examples/mir/width-guard/world.json
```

This is local coverage, not verification of complete contracts or passes. See
[CORRECTNESS.md](CORRECTNESS.md) for the remaining obligations.
