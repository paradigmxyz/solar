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

Mismatches are saved under `fuzz/fandango/out/failures/`, including the ordered
transaction history needed to reproduce a stateful divergence.

## What the runner compares

The runner talks to anvil over JSON-RPC directly (no `cast` in the replay loop).
For every vector both runtimes are exercised with `eth_call` and the raw
return-data (on success) or revert-data (on revert) bytes are compared
byte-for-byte; the JSON-RPC `message` decode is ignored on purpose, so a panic is
checked as its exact `Panic(uint256)` payload rather than a human string.
`"mode":"tx"` vectors are additionally sent as transactions and compared on:

- receipt status (mined ok vs reverted),
- emitted logs by topics + data (the contract address is excluded, since the two
  runtimes live at different addresses), and
- the contract's storage-trie root via `eth_getProof` (so a divergent storage
  write is caught even when no later `eth_call` reads the slot back).

Gas is intentionally not compared: the two code generators legitimately differ.

## Constraints

- Both runtimes are installed with `anvil_setCode`, so no constructor runs. The
  fixture must not depend on constructor logic, `immutable`s, or preset storage;
  an added immutable would read as zero on both sides and hide a real divergence.
- Keep the fixture independent of block/environment opcodes such as
  `block.number`, `block.timestamp`, `block.basefee`, and `tx.gasprice`.
  Transactions are replayed sequentially against two different runtime
  addresses, so environment-sensitive behavior can differ for reasons unrelated
  to codegen.
- Only the committed `corpus.jsonl` is fully deterministic. The seeded Fandango
  lane is bounded sampling, reproducible only for a fixed Fandango/Python version
  and `PYTHONHASHSEED`. Treat it as a source of new cases to *promote into the
  corpus*, not as a stable gate on its own.

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
runtime differential suite inside the existing codegen runtime job. The replay
step uses `set -euo pipefail`, so a mismatch fails the step; for that to *block*
a merge the `codegen-runtime` check must be marked required in branch protection
(this lives in the benchmark workflow, separate from the `ci-success` gate, so it
cannot be enforced from the workflow file alone).

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
