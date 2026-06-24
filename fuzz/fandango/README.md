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
