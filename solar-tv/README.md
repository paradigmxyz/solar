# solar-tv

Check a MIR function or EVM IR branch window before and after a compiler pass.
The CLI uses the compiler to parse and validate both files, translates its
canonical output to Lean, and asks Lean to prove the two observations equal
for every input. A counterexample is checked again at concrete input values.

From the repository root, with Python 3.10+ and the pinned Lean toolchain:

```sh
cargo build -p solar-compiler
solar-tv/solar-tv mir solar-tv/examples/dce/{before,after}.mir
solar-tv/solar-tv evmir solar-tv/examples/peephole/{before,after}.evmir
```

The DCE example deletes an unused multiplication and subtraction. Replacing
the live return with the input breaks it:

```console
$ solar-tv/solar-tv mir solar-tv/examples/dce/{before,broken}.mir
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
| MIR | One i256 function with pure add, sub, mul, and, or, xor in its entry block. It can return, jump or switch to blocks that return a word. | Returned word for all defined arguments. |
| EVM IR | An empty-entry-stack window with size inputs, literal pushes, EQ, SUB and ISZERO ending in a conditional jump to a valid label. Both exits stop. | Taken edge and remaining stack for all word-sized calldata and returndata sizes, given sufficient gas. |

The importer and the mapping from compiler operations to Lean are trusted.
The EVM window compares symbolic exits before executing either successor.
Gas costs, physical PCs, memory effects and general control flow are outside
this first fragment.

Operation semantics live in [MIR](SolarTV/MIR/Ops.lean) and
[EVM IR](SolarTV/EVMIR/Ops.lean). [Transform proofs](SolarTV/Transforms)
cover pure DCE, trimming switch cases and the branch peephole. Both layers
reuse the word definitions in the repository's existing EVM rule checker.

```sh
(cd solar-tv && lake build)
python3 -m unittest discover -s solar-tv/tests
```

The tests regenerate the example outputs with real compiler passes.
