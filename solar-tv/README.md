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
  arg0 = 0
  before = 1
  after  = 0
```

Exit codes are `0` for verified, `1` for a replayed counterexample and `2` for
inconclusive (including unsupported IR and timeouts). Use `--solar PATH` to
select a compiler binary.

| Layer | Supported fragment | Observation |
| --- | --- | --- |
| MIR | One integer function using i1, i160 and i256, pure arithmetic and bitwise operations, casts, comparisons, checked add/sub/mul, semantic panic and empty-revert checks, phis and branches in an acyclic CFG. | Returned bit pattern, exact modeled revert payload, or exceptional halt for all well-typed arguments. |
| EVM IR | A block window with symbolic live-in words, pure word/stack operations, static transfers and terminal halts. | Outgoing edge and live stack, empty successful/reverted output, or execution exception, given sufficient gas. |

The importer and the mapping from compiler operations to Lean are trusted.
EVM windows compare live stack values at outgoing edges before executing either
successor. Terminal halts discard the remaining stack. Gas costs, physical PCs,
memory effects and general control flow remain outside this fragment.

| Layer | Operation semantics | Transform proofs |
| --- | --- | --- |
| MIR | [MIR/Ops.lean](SolarTV/MIR/Ops.lean) | [Dead-code elimination](SolarTV/MIR/Transforms/DCE.lean), [trim trailing default switch cases](SolarTV/MIR/Transforms/TrimSwitchDefaults.lean), [rename phi predecessors](SolarTV/MIR/Transforms/RenamePhiPredecessors.lean), [cast cleanup](SolarTV/MIR/Transforms/Casts.lean), [overflow-check elimination](SolarTV/MIR/Transforms/CheckElim.lean) |
| EVM IR | [EVMIR/Ops.lean](SolarTV/EVMIR/Ops.lean) | [EQ/ISZERO branch peephole](SolarTV/EVMIR/Transforms/EqIszeroJumpi.lean), [commutative swap](SolarTV/EVMIR/Transforms/StackPeephole.lean), [stack safety](SolarTV/EVMIR/StackSafety.lean) |

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
unknown memory-backed revert data, undefined values and checker limits produce
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
outcomes. RETURN/REVERT require an explicit zero-size suffix; other memory-backed
output is inconclusive. The compiler also rejects structurally invalid stack
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

## Check passes during compilation

From the repository root:

```sh
export SOLAR_TV="$PWD/solar-tv/solar-tv"
target/debug/solar solar-tv/examples/mir/dce/before.mir \
  -Zmir-pipeline=dce -Zsolar-tv=target/tv
target/debug/solar solar-tv/examples/evmir/peephole/before.evmir \
  -Zevm-ir-pipeline=peephole -Zsolar-tv=target/tv
```

The host flag also applies to compilation from Solidity. Each pass manager
compares its module text before and after each pass that runs. A changed pair
goes to the checker, including edits a pass forgot to report. The supported
fragments above apply to the whole snapshot.

A verified pair produces a message on stderr. A counterexample stops the
pipeline. An inconclusive result emits a warning and compilation continues.
A checker launch or protocol failure is a compiler error. Each unsuccessful
check gets a separate directory containing `before`, `after` (with the IR
extension), and `result.txt` with the pass, module and checker response.
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
