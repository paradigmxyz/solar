# Checked Solady experiment

**This is a partial port and a reproducible comparison. The CTO's full
API-compatibility and equal-or-better gas target is not met.**

The implementations in `src/utils/` contain neither inline assembly nor
`unchecked` blocks. Arithmetic and array indexing retain Solidity's checks;
explicit bit operations and `mulmod` keep their defined Solidity semantics.
This is the experiment's concrete meaning of “safe”, not a security audit.
The benchmark checks the compiler AST of every safe source and dependency,
including the generated harness. It never substitutes an upstream assembly
implementation for a missing safe operation.

The reference is Solady v0.1.26, commit
[`acd959aa4bd04720d640bf4e6a5c71037510cc4b`](https://github.com/Vectorized/solady/tree/acd959aa4bd04720d640bf4e6a5c71037510cc4b),
from the repository's pinned
[`solady-0.1.26.json.gz`](../../testdata/projects/solady-0.1.26.json.gz).
That archive contains a 90-source profile; it is not the entire current
upstream repository. Derived code retains the upstream MIT license in
[`LICENSE-UPSTREAM`](LICENSE-UPSTREAM), including SafeCastLib's original
author attribution. `LibBit` uses upstream's bit-permutation masks, and
`LibSort` uses its `mulmod` hash with typed, bounds-checked array accesses.

## Implemented surface

| Library | Implemented non-private functions | Pinned function surface |
|---|---:|---:|
| SafeCastLib | 95 | 95 |
| LibBit | 24 | 24 |
| Base64 | 4 | 4 |
| LibSort | 28 | 57 |
| LibString | 21 | 57 |

These are function-declaration counts, not a claim of complete source or
behavioral compatibility. The runner checks parameter names, types and locations,
return types and locations, visibility, and mutability against upstream.
This includes preserving named-call syntax. It records
the missing functions across the archive in `api-coverage.json`.

The port currently covers checked casts, bit operations, Base64, four typed
array overloads for sorting/copying/reversing/duplicate checks, and selected
string conversion and inspection functions. Tokens, authentication, proxies,
cryptography, storage utilities, most strings, and other libraries remain
outside this port. Missing constants and user-defined types are also outside
the function audit.

## Run the comparison

Install `uv`, `anvil`, and the solc version pinned in
[`.github/workflows/bench.yml`](../../.github/workflows/bench.yml). The measured
run below used solc 0.8.37. Set `BENCH_SOLC` to its absolute binary path and
run from the repository root:

```sh
cargo build -p solar-compiler --bin solar
uv run benches/safe-solady/benchmark.py \
  --solc "$BENCH_SOLC" --solar target/debug/solar \
  --runs 200 --evm-version cancun \
  --output target/safe-solady/run-200

uv run benches/safe-solady/benchmark.py \
  --solc "$BENCH_SOLC" --solar target/debug/solar \
  --runs 1000000 --evm-version cancun \
  --output target/safe-solady/run-1000000
```

Output directories must be fresh. **The current comparison exits 1** because
of the compatibility discrepancies below; it still writes the full results.
There is no switch to count a mismatch as a performance win.

For each run, the exact same generated external wrappers, calldata, EVM fork,
optimizer runs, and metadata settings are used in six configurations:

| Compiler | Original Solady | Checked port |
|---|---|---|
| solc legacy | measured | measured |
| solc via-IR | measured | measured |
| this compiler | measured | measured |

The primary comparison is the checked port compiled here against the cheaper
of the two upstream solc results **for each call**. This is stricter than
selecting one solc pipeline for an entire deployed contract. The other three
configurations distinguish compiler effects from rewrite effects.

The runner starts an isolated Anvil instance, deploys each harness with the
normal code-size limit, and traces independent pure calls. Opcode gas includes
memory expansion, dispatch, decoding and encoding; it excludes transaction
intrinsic gas. Deployment gas and creation/runtime size are separate fields.
Harness size measures the same exposed function subset, not a production
application. Compilation time is one observed sample per configuration, not
a statistically meaningful compiler-speed benchmark.

Every call is checked against a Python oracle, including exact return or
revert bytes. A disagreement in **any** configuration excludes that case from
gas rankings. All cases, including failures, remain in `results.json` with
replay calldata. Coverage includes every integer narrowing boundary, every bit
position and population count, signed/unsigned/address/bytes32 extrema, empty
and duplicate arrays, partition thresholds, and buffer lengths across word
boundaries. Base64 uses Python's standard encoder as its independent oracle.

The output includes:

- `report.md`: summary, sizes, and compatibility failures.
- `api-gas.md`: per-function wins, ties, losses, and worst regressions.
- `results.json`: all six measurements for each case, deployment data,
  compiler versions/hashes, source hashes, and mismatches.
- `api-coverage.json`: declaration coverage and missing functions.
- `<configuration>/input.json` and `output.json`: exact compilation inputs,
  ABI, and bytecode, sufficient to reproduce a failing call.

## Measured result at 200 runs

The local run on branch `feat/safe-solady` used compiler commit `f9a57a2d3`,
solc `0.8.37+commit.f401782d`, Cancun, and no CBOR metadata. All **9,201** cases
matched the oracle in the three configurations compiling the checked source.
There were **120** mismatching upstream executions across **40** cases,
excluded from performance comparisons. These are bounded tests, not an
all-input equivalence proof.

Representative opcode gas measurements, including identical harness overhead:

| Call | Original / best solc | Checked / this compiler |
|---|---:|---:|
| `SafeCastLib.toInt128(0)` | 461 | 215 |
| `LibBit.reverseBytes(0)` | 719 | 566 |
| `LibBit.reverseBits(0)` | 818 | 762 |
| `Base64.encode`, 256-byte input | 15,039 | 127,545 |
| `LibSort.sort(uint256[])`, 64 mixed values | 33,187 | 167,582 |
| `LibSort.insertionSort(uint256[])`, 64 descending values | 112,215 | 636,561 |

Both wins and losses are shown deliberately. These examples are not an
aggregate score. The full per-function report retains every regression.
Several simple operations already meet the gas target, but bulk bytes/string
processing and sorting need substantial further work.

A second full run at **1,000,000 optimizer runs** also checked 9,201 cases,
with the same 40 upstream discrepancies and no mismatches for checked code.
The target remains unmet at that setting. For example, checked signed
narrowing costs 215 gas versus 478 for upstream/best-solc, and byte reversal
costs 502 versus 618. Bit reversal costs 764 versus 760, so its small win at
200 runs does **not** generalize to this setting. Both full reports retain
the per-call measurements.

| Identical harness | Original / solc legacy bytes | Original / solc IR bytes | Checked / this compiler bytes |
|---|---:|---:|---:|
| SafeCastLib | 7,352 | 6,597 | 3,425 |
| LibBit | 3,342 | 3,058 | 2,670 |
| Base64 | 1,300 | 1,673 | 2,285 |
| LibSort, partial | 2,777 | 2,505 | 8,330 |
| LibString, partial | 3,033 | 2,954 | 3,001 |

## Compatibility findings and remaining boundaries

The pinned original behaves differently from the intended value-level oracle
in two reproducible cases. Both solc pipelines and this compiler reproduce
these differences when compiling the original source:

- `LibString.toHexString[NoPrefix](value, 0)` exhausts the call's gas budget.
  Its assembly loop runs at least once and decrements away from its initial
  end pointer. The checked port returns the empty representation for zero
  or `HexLengthInsufficient()` for a nonzero value. This is an explicit
  compatibility difference, never a gas win.
- `LibBit.toNibbles` returns corrupted bytes for the two 256-byte test inputs
  through the shared wrapper. The original uses the advanced input pointer
  `s`, instead of the input length `n`, when updating the free-memory pointer
  and zeroing after the result. The checked port returns the oracle's nibbles.

The original files for these cases were also compared byte-for-byte against
the pinned GitHub tag. No upstream source was patched for the comparison.
The raw failure records are retained for reproduction, not allowlisted.

Full API compatibility needs a separate decision for APIs ordinary Solidity
cannot express while retaining the original semantics:

- `LibString.directReturn` returns successfully from the enclosing external
  EVM call. An ordinary `return` in an internal helper only returns from that
  helper, so it cannot implement the same API.
- `LibSort.uniquifySorted` changes a memory array's length in place, including
  through aliases. Returning a newly allocated array changes its signature
  and alias behavior. Solidity does not expose a memory-array resize method.
- Raw storage-reference conversions and custom storage layouts require
  preserving existing representation and aliasing, not merely substituting
  ordinary state variables.

We must either exclude such APIs from the safe compatibility target or add
well-defined, checked language/compiler primitives for them. Hiding assembly
in a dependency or adding `assembly ("memory-safe")` does not meet this
experiment's policy. Implementing compiler primitives is a separate change;
none were added here.

Typed custom-error reverts can add error entries to the generated ABI that
the original assembly implementation did not expose. Callable ABI entries
are checked identically, and actual revert data is checked separately. The
whole JSON ABI is therefore not necessarily identical. Malformed Base64
inputs have unspecified output upstream; the comparison covers documented
alphabets and padding modes, not a claim about all malformed inputs.

## Runner checks

```sh
uv run --with eth-abi==5.2.0 --with 'eth-hash[pycryptodome]==0.7.1' \
  python -m unittest discover -s benches/safe-solady -p 'test_*.py'
```

The original pinned test suites for the three complete function surfaces
can also be run without checking out or modifying an external repository:

```sh
uv run benches/safe-solady/upstream_tests.py \
  --solc "$BENCH_SOLC" --solar target/debug/solar \
  --output target/safe-solady/upstream-tests-new
```

The checked implementations passed **60 tests**, including fuzz tests at
256 runs, under both solc via-IR and this compiler: 11 SafeCastLib tests,
34 LibBit tests, 13 Base64 tests, and two shared helper tests. The original
solc baseline passed 59 and failed `testToNibblesDifferential` with
`Insufficient memory allocation!`, independently reproducing the memory
allocation discrepancy above. The runner retains this baseline failure and
exits nonzero; it does not suppress the test.

Only the three target library files are replaced in the checked legs.
Original test helpers, including their assembly and LibString's reference
`replace` implementation, remain in the **test harness**. They are not
dependencies of the checked library implementations and are not included in
the safe-source gas benchmark. Test-contract gas is not used in performance
claims. Each leg's original/rewritten source hashes, raw Forge output and
reproduction data are preserved.

`CompilerDifferential.sol` also provides small targets for the repository's
[`solsymdiff`](../../fuzz/fandango/README.md) compiler check. For example:

```sh
fuzz/bin/solsymdiff \
  --source benches/safe-solady/CompilerDifferential.sol \
  --contract CompilerDifferential --signature 'popCount(uint256)' \
  --solc "$BENCH_SOLC" --solar target/debug/solar --forge "$SYMBOLIC_FORGE" \
  --evm-version cancun --via-ir --timeout 180 --symbolic-timeout 60
```

`narrow(int256)` and `popCount(uint256)` reported bounded compiler agreement
locally. This compares the two compilers on the same checked source; it is
not a proof of equivalence to the original assembly implementation.
