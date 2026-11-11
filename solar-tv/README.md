# solar-tv

Check a MIR function or EVM IR block window before and after a compiler pass.
The CLI uses the compiler to parse and validate both files, translates its
canonical output to Lean, and asks Lean to prove refinement for MIR or equal
observations for the EVM IR fragment, for every input. A counterexample is checked again at concrete input values.

From the repository root, with Python 3.10+ and the pinned Lean toolchain:

```sh
cargo build -p solar-compiler
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
inconclusive (including unsupported IR and timeouts). Use `--solar PATH` to
select a compiler binary.

| Layer | Supported fragment | Observation |
| --- | --- | --- |
| MIR | One integer function using i1, i160 and i256, arithmetic, casts, checks, phis and branches in an acyclic CFG; constant-address memory operations. | Returned bits and live memory, exact modeled revert payload, or exceptional halt. |
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
width and are checked again at concrete values. Undefined values and other
integer widths remain inconclusive.

## Outcomes and refinement

[Outcome.lean](SolarTV/Outcome.lean) defines returns, reverts and exceptional
halts, together with refinement as inclusion of possible observable outcomes.
For the deterministic fragment, each input has exactly one outcome, so a
candidate must preserve the returned bits, the revert payload or the exceptional
halt. A failing overflow check is a defined `Panic(0x11)` revert. Replacing it
with a successful return fails refinement.

Checked addition, subtraction and multiplication support signed and unsigned
i160 and i256 values. Execution stops at the first failure, including when a
checked result is unused. Semantic panic checks retain the complete panic code.
Empty reverts and `invalid` have distinct outcomes. Unsupported operations,
MIR memory-backed revert data, undefined values and checker limits produce
an inconclusive result. Checked division, remainder and exponentiation remain
outside the modeled fragment.

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

The supported operations are literal PUSH, DUP1..16, SWAP1..16, POP, ADD, MUL,
SUB, AND, OR, XOR, EQ, NOT, ISZERO, CALLDATASIZE and RETURNDATASIZE. Static
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

`MLOAD`, `MSTORE`, `MSTORE8` and `MSIZE` are modeled in both layers. Bytes use
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
The current subset supports Cancun, Prague and Osaka, with Cancun as the CLI
default. `--evm-version` selects the parser target and checked schedule; other
versions are inconclusive. Calls, storage, refunds and transaction intrinsic
gas remain unsupported.

These are explicit opcode windows: execution starts after any entry JUMPDEST,
and ends before the outgoing successor executes. A static transfer includes its
label PUSH and JUMP/JUMPI. Layout-dependent structured jumps remain unsupported.
The checker does not estimate EVM gas from MIR; `mir --gas=...` is inconclusive.

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

The host flag also applies to compilation from Solidity. Each pass manager
compares its module text before and after each pass that runs. A changed pair
goes to the checker, including edits a pass forgot to report. The supported
fragments above apply to the whole snapshot.

A verified pair produces a message on stderr. A counterexample stops the
pipeline. An inconclusive result emits a warning and compilation continues.
A checker launch or protocol failure is a compiler error. Each unsuccessful
check gets a separate directory containing `before`, `after` (with the IR
extension), and `result.txt` with the pass, module and checker response.
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
## Agreement with the pass validation encoder

[Agreement/Encoder.lean](SolarTV/Agreement/Encoder.lean) proves that the SMT terms
`-Zvalidate-passes` builds are the Lean semantics. The `lean_agreement` test in
`crates/codegen/src/mir/transform/validate_passes.rs` generates it: it asks the real
encoder for the term of each operation on symbolic operands and translates the term
with the certifier's `word_term`, so a change to either shows up as a snapshot diff
and must still prove. Regenerate it with
`SNAPSHOTS=overwrite cargo nextest run -p solar-codegen lean_agreement`.

- `word_X` and `step_X`: every pure opcode the encoder models exactly (ADD, MUL, SUB,
  DIV, SDIV, MOD, SMOD, ADDMOD, MULMOD, SIGNEXTEND, LT, GT, SLT, SGT, EQ, ISZERO, AND,
  OR, XOR, NOT, BYTE, SHL, SHR and SAR) equals its `EvmRules.Evm` definition and the
  word `EVM.step` pushes.
- `exp_base_X` and `exp_power_X`: the shapes of EXP the encoder computes exactly, a
  base of 0, 1, 2, 16 or 256 and an exponent of 0, 1, 2 or 8.
- `abstract_X`: the laws of the abstract tier hold of the exact MUL, DIV, MOD, ADDMOD
  and MULMOD, so the abstraction only adds models.
- `mir_X_iN`: each MIR operation on i1, i160 and i256 operands, which the encoder
  keeps as zero-extended words masked to their width, gives the zero-extended result
  of its [MIR/Ops.lean](SolarTV/MIR/Ops.lean) definition, including casts, `ne` and
  `select`. Signed operations appear at i256 only, since the encoder rejects them on
  narrow integers.

[Agreement/Tactic.lean](SolarTV/Agreement/Tactic.lean) holds the tactics. Division,
remainder, products and powers go through rewriting and `BitVec.toNat`; the rest is
bit-blasted. CLZ, EXP outside these shapes, memory, storage, calls and environment
reads are uninterpreted in the encoder and have no agreement theorem.

EVM IR windows also accept DIV, SDIV, MOD, SMOD, ADDMOD, MULMOD, EXP, SIGNEXTEND, LT,
GT, SLT, SGT, BYTE, SHL, SHR and SAR, with their Cancun prices in
[EVMIR/Gas.lean](SolarTV/EVMIR/Gas.lean), EXP's 50 gas per exponent byte included.
## Contract semantics and stages

[Sem.lean](SolarTV/Sem.lean) restates solidity-lean's interface verbatim: a `Call`
(calldata, call value, initial storage), its `Observable` result (success, output, logs,
storage), a `Sem` that runs a program on a call, and `Stage A B p q scope`, which says
that every result `p` gives on a call in scope, `q` gives too. Our additions follow a
marker: `Stage.refl` and `Stage.mono`, the one-frame `environment` (caller 0x2000,
callee 0x1000) and `Sem.FailureConvention` (a failed call logs nothing and keeps its
initial storage).

[Machine.lean](SolarTV/Machine.lean) is the state both layers share. Memory is the model
of [Memory.lean](SolarTV/Memory.lean) on every byte; `Mem.project` is its 256-byte
`Memory.State` and the `project_` lemmas show that expansion, `MSIZE`, loads, stores and
byte stores agree with it. Storage and transient storage are total maps and the world is
the history of logs and calls. Environment reads and call results come from an `Oracle`
of functions of the world, as the encoder's uninterpreted functions. `observe` commits
storage and logs only on success, and `Agree` compares two ends as `Encoder::differ`
does. A `Layer` runs a program with fuel, monotonically, and its `sem` is fuel-free: a
call's result is the end some fuel reaches. `Certified` (every end of one program has an
agreeing end of the other) gives a stage (`stage_of_certified`). Gas is not priced: `GAS`
is an environment read.

[MIR/Program.lean](SolarTV/MIR/Program.lean) is MIR at the lowered phase, the input of
`-Zvalidate-passes`, with phis, branches, the pure EVM operations of `EVM.step`,
effects, and `ret`, `revert`, `stop` and `invalid`. `lower-abi` and `lower-dispatch` run
before it, so they are outside this semantics. [EVMIR/Program.lean](SolarTV/EVMIR/Program.lean)
runs scheduled EVM IR blocks over one stack with the checked operations of
[EVMIR/Memory.lean](SolarTV/EVMIR/Memory.lean) (`stepOp_project`), effects, static
`jump`/`jumpi` and halts. Both satisfy the failure convention.

[Composition.lean](SolarTV/Composition.lean) chains the stages of a `Compilation` (the MIR,
each MIR pass's output, the scheduled EVM IR and each EVM IR pass's output) into one stage
from the MIR to the final EVM IR (`mir_to_evmir`, or `mir_to_evmir_certified` from
certificates), and `mir_to_bytecode` appends `AssemblyStage`, the stage to bytecode that
solidity-lean's bytecode semantics states.
## Lifting a pass validation query

[MIR/Validation/AddZero.lean](SolarTV/MIR/Validation/AddZero.lean) lifts one real query to
a stage. [lift_egraph.mir](../tests/ui/codegen/mir/validate-passes/lift_egraph.mir) runs
`egraph` on a dispatcher that adds zero to a calldata word before shifting by it and
storing the result, and pins the query `-Zvalidate-passes` writes. `Query` states it as
the certifier translates it, and `query_unsat` proves it by hand: the certifier's
`bv_decide` proof does not handle its top-level array disequality. `certified_of_query`
uses the query only as a hypothesis: it runs both functions, rewrites their words with
the agreement theorems `step_add` and `step_shl`, and instantiates the query's
uninterpreted calldata read, memory, storage and transient storage with the call and the
initial state. `stage` is the resulting stage of the lowered MIR semantics. It is one
instance: no theorem yet says that every query `-Zvalidate-passes` finds `unsat` gives a
certificate, which would need the encoder itself in Lean.
