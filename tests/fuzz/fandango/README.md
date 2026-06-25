# Fandango Fuzzing

This directory contains bounded Fandango-based differential tests.

- `abi-values.fan` generates ABI values for `AbiVectorFixture.sol`. The runner
  compiles the fixture with solc and this compiler, installs both runtimes into
  anvil, then compares exact runtime behavior.
- `solidity-source.fan` generates complete Solidity source files. The source
  runner compiles each generated `.sol` file with solc and this compiler and
  records compile-result disagreements.

Generated artifacts belong under `tests/fuzz/fandango/out/`, which is ignored.
Promote only minimized, stable failures into `corpus.jsonl` or `tests/ui/`.

## CI

The normal CI test job runs:

- the committed ABI corpus,
- a small bounded sample from `abi-values.fan`, and
- a small bounded sample from `solidity-source.fan`.

The ABI runner compares:

- raw `eth_call` return bytes or revert bytes,
- transaction receipt status,
- emitted log topics and data, excluding contract address, and
- the contract storage root via `eth_getProof`.

Gas and bytecode size are not compared here; benchmark jobs report those.

## Run Locally

Install the pinned Fandango version:

```bash
python3 -m venv /tmp/solar-fandango-venv
/tmp/solar-fandango-venv/bin/pip install 'fandango-fuzzer==1.1.1'
```

Run ABI runtime differentials against a local anvil:

```bash
anvil --silent --port 8545

PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f tests/fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 32 \
  --separator $'\n' \
  --progress-bar off \
  | python3 tests/fuzz/fandango/encode_abi_vectors.py --seed 1 \
  | python3 tests/fuzz/fandango/run_abi_vectors.py \
      --max-vectors 256 \
      --max-calldata-bytes 4096 \
      --timeout 20
```

Generate Solidity sources and compile-check each file:

```bash
mkdir -p tests/fuzz/fandango/out/sources

PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f tests/fuzz/fandango/solidity-source.fan \
  --random-seed 1 \
  -n 32 \
  --directory tests/fuzz/fandango/out/sources \
  --filename-extension .sol \
  --progress-bar off

python3 tests/fuzz/fandango/run_solidity_sources.py \
  --source-dir tests/fuzz/fandango/out/sources \
  --max-sources 256 \
  --timeout 20 \
  --verbose
```

Failures are saved under `tests/fuzz/fandango/out/failures/` or
`tests/fuzz/fandango/out/source-failures/`.

## Extend

For ABI fuzzing:

1. Add value shapes to `abi-values.fan`.
2. Add or update methods in `AbiVectorFixture.sol`.
3. Teach `encode_abi_vectors.py` how to encode the new shape and whether it is
   a `call` or `tx` vector.
4. Add minimized stable cases to `corpus.jsonl` when they should run on every PR.

For source fuzzing:

1. Add valid-by-construction source shapes to `solidity-source.fan`.
2. Keep the grammar inside the subset both solc and this compiler are expected
   to compile.
3. Promote confirmed source mismatches into ordinary `tests/ui/` cases.

## Constraints

- ABI runtimes are installed with `anvil_setCode`, so constructors do not run.
  The fixture must not depend on constructor state, `immutable`s, or preset
  storage.
- Keep the ABI fixture independent of block and transaction environment values
  such as `block.timestamp`, `block.number`, `block.basefee`, and `tx.gasprice`.
- Only `corpus.jsonl` is a stable deterministic corpus. Seeded Fandango samples
  are bounded sampling and should be used to find cases worth promoting.
