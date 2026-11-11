# solar-tv

Check a MIR function or EVM IR branch window before and after a compiler pass.
The CLI uses the compiler to parse and validate both files, translates its
canonical output to Lean, and asks Lean to prove the two observations equal
for every input. A counterexample is checked again at concrete input values.

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
| MIR | One i256 function with pure add, sub, mul, and, or, xor, predecessor-selected phis, jumps and ordered switches in an acyclic CFG. | Returned word for all defined arguments. |
| EVM IR | An empty-entry-stack window with size inputs, literal pushes, EQ, SUB and ISZERO ending in a conditional jump to a valid label. Both exits stop. | Taken edge and remaining stack for all word-sized calldata and returndata sizes, given sufficient gas. |

The importer and the mapping from compiler operations to Lean are trusted.
The EVM window compares symbolic exits before executing either successor.
Gas costs, physical PCs, memory effects and general control flow are outside
this first fragment.

| Layer | Operation semantics | Transform proofs |
| --- | --- | --- |
| MIR | [MIR/Ops.lean](SolarTV/MIR/Ops.lean) | [Dead-code elimination](SolarTV/MIR/Transforms/DCE.lean), [trim trailing default switch cases](SolarTV/MIR/Transforms/TrimSwitchDefaults.lean), [rename phi predecessors](SolarTV/MIR/Transforms/RenamePhiPredecessors.lean) |
| EVM IR | [EVMIR/Ops.lean](SolarTV/EVMIR/Ops.lean) | [EQ/ISZERO branch peephole](SolarTV/EVMIR/Transforms/EqIszeroJumpi.lean) |

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
