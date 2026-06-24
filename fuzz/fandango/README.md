# Fandango ABI Prototype

This is a local-only feasibility spike for using Fandango to generate runtime
differential inputs. It is not part of normal CI.

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
  -n 8 \
  --separator $'\n' \
  --progress-bar off
```

Generate encoded calldata vectors:

```bash
PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 8 \
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
  -n 8 \
  --separator $'\n' \
  --progress-bar off \
  | python3 fuzz/fandango/encode_abi_vectors.py --seed 1 \
  | python3 fuzz/fandango/run_abi_vectors.py
```

Mismatches are saved under `fuzz/fandango/out/failures/`.

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

Fandango can also write one generated input per file:

```bash
PYTHONHASHSEED=1 /tmp/solar-fandango-venv/bin/fandango fuzz \
  -f fuzz/fandango/abi-values.fan \
  --random-seed 1 \
  -n 8 \
  --directory fuzz/fandango/out \
  --filename-extension .json \
  --progress-bar off
```

The next step is to feed these JSON vectors into the runtime differential
runner. Keep generated `fuzz/fandango/out/` artifacts out of git; promote only
minimized confirmed bugs into deterministic runtime tests.
