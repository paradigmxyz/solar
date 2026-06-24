# Fandango ABI Prototype

This is a bounded runtime differential suite for Fandango-generated ABI inputs.
The benchmark workflow runs the deterministic corpus plus a small generated
sample on every PR; larger fuzzing runs stay local or manual.

Install the pinned version in a disposable environment:

```bash
python3 -m venv /tmp/solar-fandango-venv
/tmp/solar-fandango-venv/bin/pip install 'fandango-fuzzer==1.1.1'
```

Generate deterministic ABI-value vectors:

```bash
PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 32 \
  --separator $'\n' \
  --progress-bar off
```

Generate encoded calldata vectors:

```bash
PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 32 \
  --separator $'\n' \
  --progress-bar off \
  | python3 fuzz/fandango/encode_abi_vectors.py --seed 1
```

Run those vectors against solc and this compiler on a local anvil:

```bash
anvil

PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 32 \
  --separator $'\n' \
  --progress-bar off \
  | python3 fuzz/fandango/encode_abi_vectors.py --seed 1 \
  | python3 fuzz/fandango/run_abi_vectors.py \
      --max-vectors 256 \
      --max-calldata-bytes 4096 \
      --timeout 20
```

Mismatches are saved under `fuzz/fandango/out/failures/`.

The generator covers:

- dynamic ABI values: `uint256`, `bool`, `bytes`, `string`
- signed integers, fixed bytes, and addresses
- dynamic and fixed `uint256` arrays
- checked arithmetic and array-bounds revert paths
- stateful mapping and storage-bytes calls through transaction vectors

`corpus.jsonl` contains a small deterministic corpus that is encoded before
generated vectors in CI. Keep it focused on stable edge cases that should run
on every PR, such as panic payloads, storage mutation/readback, and boundary ABI
values.

## Promoting Failures

Keep raw generated artifacts under `fuzz/fandango/out/`; that directory is
ignored. When a mismatch is confirmed:

1. Minimize the vector or source by hand.
2. Add the minimized case to the deterministic runtime suite when it depends on
   execution behavior.
3. Add a `tests/ui/codegen/` case only when the emitted compiler output is the
   useful regression signal.
4. Record the Fandango seed, generated vector, compiler flags, and solc version
   in the promoted test or its nearby comments.

Do not commit bulk generated corpora. Commit only minimized regression tests and
small hand-written generator specs.

## CI Lanes

Keep these lanes separate:

- PR CI: deterministic runtime checks only, through `cargo nextest run --workspace`
  and `cargo xtask test runtime`
- Manual or nightly fuzzing: bounded Fandango runs with explicit `--random-seed`,
  `--max-vectors`, `--max-transactions`, `--max-calldata-bytes`, and `--timeout`
- Local debugging: use the commands above and keep generated artifacts under
  `fuzz/fandango/out/`

Fandango mismatches are correctness failures for the fuzz job. Gas or bytecode
size differences should be reported by benchmark jobs, not by the fuzz runner.
The benchmark workflow runs the same ABI-vector replay as a deterministic
runtime differential suite inside the existing codegen runtime job.

Fandango can also write one generated input per file:

```bash
PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 32 \
  --directory fuzz/fandango/out \
  --filename-extension .json \
  --progress-bar off
```

The next step is to feed these JSON vectors into the runtime differential
runner. Keep generated `fuzz/fandango/out/` artifacts out of git; promote only
minimized confirmed bugs into deterministic runtime tests.
