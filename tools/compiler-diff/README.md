# Compiler comparisons

Import Solidity inputs, run standard-JSON compilers, and compare their saved
artifacts. Compilation and comparison keep separate records, so new comparison
rules do not rerun compilers. The tool reuses the `scripts/sourcify.py` database
without migration; `uv run scripts/sourcify.py` still works. Run commands from
the repository root. Agents also follow the
[repository guidance](../../AGENTS.md#compiler-comparisons).

```sh
export UV_CACHE_DIR=/tmp/solar-sourcify/uv-cache
export UV_PROJECT_ENVIRONMENT=/tmp/solar-sourcify/python
compiler-diff() { uv run --project tools/compiler-diff compiler-diff "$@"; }
compiler-diff --help
compiler-diff self-test
```

Later examples use this `compiler-diff` function; in a shell that does not keep
state between commands, spell out the `uv run` form. `--dir` and `--version` go
before the subcommand. Data lives in
`<dir>/<version>`, by default `/tmp/solar-sourcify/0.8.37`; the uv variables
above keep uv's files there too.

## Inputs and compilation

```sh
compiler-diff --version 0.8.37 sync
compiler-diff run --limit 20 \
  --compiler 'solc=/path/to/solc --standard-json' \
  --compiler 'solar=/path/to/solar --standard-json -Zvalidate-ir=true'

# Keep local repros in a separate directory.
compiler-diff --dir /tmp/compiler-repro import-input input.json --target 'C.sol:C'
compiler-diff --dir /tmp/compiler-repro run
```

`import-input` needs standard JSON with inline sources. Runs keep the input
settings except `outputSelection`, which requests ABI, creation and runtime
bytecode, method identifiers, userdoc, and devdoc. Inputs are content-addressed
with their target, so repeat imports are harmless. Sources stay inline; names
such as `../C.sol` never become filesystem paths.

Compilers read standard JSON on stdin. Give wrapper arguments absolute paths,
and change `--tag` when wrapper dependencies or environment change. `run` stops
at the first failure unless `--continue-on-failure` is set; `--retry-failures`
retries cached failures. Any failure exits 1. See `run --help` for all options.
Each attempt keeps its input, output, compiler identity, timing, diagnostics,
and a `replay.sh` that uses the recorded executable path.

## Saved comparisons

```sh
# Latest attempt from each compiler, for every attempted compilation.
compiler-diff compare --continue-on-failure
compiler-diff compare --check abi --policy exact --check userdoc --check devdoc
# Specific attempt directories or raw standard-JSON output files.
compiler-diff compare --left /path/to/solc-attempt --right /path/to/solar-attempt \
  --check abi --check methods
```

By default `compare` checks ABI interface compatibility and method identifiers
between compilers named `solc` and `solar`; `--reference` and `--candidate`
change the names. It uses the latest attempts, failures included, and never
falls back to an older success. Attempt pairs must have identical inputs. Raw
JSON files carry no input or process provenance; use attempt directories when
that matters.

| Check | Rules |
| --- | --- |
| `abi --policy interface` | Ignore parameter names and `internalType`; compare functions, returns, mutability, events/indexing, errors, constructors, fallback and receive. |
| `abi --policy exact` | Keep all ABI fields; ignore object-key and ABI-entry order. |
| `methods` | Compare signature-to-selector maps. |
| `userdoc`, `devdoc` | Compare parsed JSON structurally, including names and descriptions. |

Both ABI policies expand tuple signatures recursively, keep parameter and
component order and duplicate entries, and neither guess the meaning of
user-defined library types nor erase enum/type differences. Bytecode, ASTs, and
storage layouts need their own semantic rules, so `compare` skips them. ABI
agreement does not imply runtime equivalence.

Each check reports `equal`, `different`, `unsupported` (outputs missing on both
sides), or `error` (failed compilation or malformed output); output missing on
one side is a difference. Reports exclude unrun corpus entries but count corpus
size and available and processed pairs, so a partial run cannot pass for full
coverage. The CLI prints PASS, FAIL, or KNOWN per pair; failures show compiler
commands, versions and hashes, JSON paths, both values, and replay commands. It
shows five differences per check and shortens large values unless `--full` is
set; reports keep everything, grouped by comparator and path. The summary gives
processed/available pairs and corpus size. Comparisons stop at the first
unexpected difference or incomplete check unless `--continue-on-failure` is
set; either way these exit 1.

Reports and mismatch bundles live in `comparisons/<id>/`, indexed by the local
DuckDB database. Bundles copy both sides' attempt files and list JSON-pointer
differences with both values. When both sides exist, a bundle also holds
`compare.sh` and a snapshot of its expectation rules; `sh <bundle>/compare.sh`
repeats the comparison from the copies, needs this checkout and uv, and writes
a new report. Compiler replay scripts use the recorded executable paths, not
bundled binaries. Standard-JSON compilers may exit 0 with error diagnostics;
inspect `replay.stdout.txt` or pass the outputs to `compare`, which rejects
compiler errors.

## Expected divergences

`--expectations FILE` takes rules with an exact comparator, rule version,
policy, JSON-pointer difference with both values, and reason:

```json
[{"comparator": "abi", "version": 1, "policy": "interface",
  "difference": {"path": "/contracts/C.sol/C/value/function:f()/0/stateMutability",
                 "kind": "value", "left": "view", "right": "pure"},
  "reason": "Tracked compiler discrepancy; replace with an issue link"}]
```

Copy differences from a report after inspecting them. Matching differences stay
visible with the reason but do not fail; other differences fail, even in the
same function. Rules cannot excuse errors or unsupported checks. Reports list
unused rules, even when a comparison stops before reaching them. Nothing is
accepted by default. Rules match contract paths across inputs; keep rules for
one corpus in their own file.

## Directory corpora and solc tests

```sh
compiler-diff --dir /tmp/solc-suite import-directory \
  testdata/solidity/test/libsolidity --format solc --allow-skips
compiler-diff --dir /tmp/solc-suite run --continue-on-failure \
  --compiler 'solc=/absolute/path/to/solc --standard-json' \
  --compiler 'solar=/absolute/path/to/solar --standard-json'
compiler-diff --dir /tmp/solc-suite compare --continue-on-failure
```

`import-directory` reads `.sol` files recursively, with `--format solc`
(default) for solc fixtures or `--format solidity` for plain sources. Each file
is an entry point that pulls in its imports. Identical inputs deduplicate.
Comparisons cover every emitted contract, so no target is needed.

For solc fixtures, the importer splits `==== Source:` sections, loads
`==== ExternalSource:` aliases, resolves imports, and snapshots sources inline.
Dependencies must stay inside the imported directory, so import the common
parent of sibling directories. Escaped import paths and settings remappings are
unsupported. Source-unit names stay JSON keys, never output paths.

`--settings FILE` sets default standard-JSON settings; `evmVersion` defaults to
`osaka`. Solc `EVMVersion` settings select a target or check that it meets a
restriction. `compileViaYul: also` imports both pipelines; `true` or `false`
picks one. `revertStrings` is translated too. Other test settings are reported
as unsupported, not dropped.

The importer skips intentional compiler-error tests, since ABI and bytecode
comparisons do not apply, but lists them in the import report. It neither checks
diagnostic text nor runs upstream runtime expectations; warnings and expectation
text stay in fixture metadata. `cargo tq solc-solidity` runs the upstream suite.

Each import writes `imports/<id>/report.json` with per-file status, skip
reasons, compilation IDs, original fixtures, and standard-JSON inputs; new
attempts and failure bundles include `import.json` provenance. The CLI
summarizes coverage and common skip reasons; `--verbose` prints each fixture.
Skips exit 1 unless `--allow-skips` is set; an import that skips everything
always exits 1. Read the report before taking a later passing comparison as
suite coverage.

## Fandango campaigns

`fuzz` generates complete Solidity sources and imports each as standard JSON,
then compiles and compares each case before the next. This example seeds
generation from a directory and runs ten batches:

```sh
compiler-diff --dir /tmp/compiler-fuzz fuzz \
  --grammar fuzz/fandango/solidity-runtime-source.fan --contract FandangoRuntime \
  --initial-population fuzz/fandango/runtime-corpus \
  --population-size 24 --mutation-rate 0.4 --crossover-rate 0.4 \
  --seed 7 --count 64 --rounds 10 \
  --compiler 'solc=/absolute/path/to/solc --standard-json' \
  --compiler 'solar=/absolute/path/to/solar --standard-json'
```

Defaults are `--grammar fuzz/fandango/solidity-source.fan`,
`--contract FandangoSource`, `--seed 1`, `--count 16`, and `--rounds 1`. Other
grammars must be self-contained and emit Solidity sources; the campaign
snapshots the grammar file but not external resources. The generator uses the
Fandango version in `compiler_diff/fandango.py` and the Python in
`.python-version` through uv, with `PYTHONHASHSEED` set to `--seed`. Its
Python, tool environment, and cache live in `<dir>/fandango-tools/`.

`--initial-population` recursively snapshots `.sol` seeds with their relative
names and content hashes. Seeds must parse with the grammar, or generation
fails with stderr and a campaign error report; arbitrary solc tests do not
automatically fit the small runtime grammar.

`--rounds` runs batches with consecutive seeds, each with its own report. Each
round must generate exactly `--count` sources before importing any, so a run
makes at most `rounds * count`. Fandango evolves its population within a batch,
and every round restarts from the given seeds; outputs never feed later rounds.
Outputs may include seeds, so a small count does not prove mutation happened.

Repeat `--compiler NAME='COMMAND ARGS'` for any standard-JSON compilers. The
first is the default reference, compared with each other one; `--reference NAME`
and repeated `--candidate NAME` select a subset. Checks default to ABI and
selectors; `--check`, `--policy`, `--expectations`, and `--full` work as for
`compare`. A compilation failure fails the case, even when both compilers
reject it, but does not alone show a divergence.

`--settings FILE` takes a standard-JSON settings object; `evmVersion` defaults
to `osaka`, so pick a supported target for older compilers. The runner still
selects the outputs that comparisons need. `--generation-timeout` (default
600 s) bounds generation, including first tool install; `--timeout` (default
120 s) bounds each compiler.

Add a symbolic check of a function present in every generated contract:

```sh
compiler-diff --dir /tmp/compiler-fuzz fuzz \
  --grammar /absolute/path/to/pure-function.fan --contract C --seed 7 --count 8 \
  --compiler 'baseline=/absolute/path/to/solc --standard-json' \
  --compiler 'candidate=/absolute/path/to/solar --standard-json' \
  --symbolic-signature 'f(uint256)' --symbolic-solc /absolute/path/to/solc \
  --symbolic-args '--max-paths 64 --symbolic-timeout 10'
```

The check uses each selected pair's saved attempts. `--symbolic-solc` (default
`solc`) compiles the harness; `--symbolic-args` forwards quoted engine options
such as `--include-view`; `--engine-timeout` (default 120 s) bounds each engine
process. A missing function or incomplete check fails the case.

Campaigns live in `<dir>/<version>/fuzz/<id>/`. Their identity covers grammar
content, generator version, seed, count, target, settings, seed content, and
mutation options. Repeating those options verifies and reuses generated files;
new compiler commands start new compile jobs without regenerating sources.
Comparisons use the exact attempts this invocation selects, older cache hits
included, and always rerun so new rules apply. `--retry-failures` retries
failed compiler jobs. A failed case or round stops the command unless
`--continue-on-failure` is set, and always makes it exit 1. Generation or setup
errors and interrupts stop at once. `--generate-only` imports sources without
running compilers.

Each campaign keeps `grammar.fan`, `campaign.json`, generation logs, source
hashes, generated sources, standard-JSON inputs, and a latest `report.json`
that links cases to compiler attempts, comparison bundles, and symbolic reports.
Attempts and mismatch bundles include `generator.json` with the grammar and
seed. The corpus database and run artifacts live in the campaign's `<version>/`
subdirectory. Pass the printed campaign path as `--dir` to `run`, `compare`, or
`status` to inspect or rerun that corpus alone.

The [ABI-value and stateful Fandango runners](../../fuzz/fandango/README.md)
keep their own execution and reduction workflows.

## Execution engines

`symbolic` and `runtime` run the existing execution engines. Pass engine options
after `--`, with absolute paths for files and executables, because the child
runs in its artifact directory. Reports and logs stay in
`<dir>/<version>/engines/<engine>/<id>`. `--engine-timeout` (default 3600 s)
bounds the whole child; engine timeout flags keep their own meaning.

```sh
compiler-diff symbolic -- --help
compiler-diff runtime -- --help
compiler-diff runtime -- --solc /path/to/solc --solar /path/to/solar \
  --mode runtime --suite micro --tests counter --gas --start-anvil

# Reuse saved artifacts without recompiling the target.
compiler-diff symbolic -- --source C.sol --contract C \
  --signature 'f(uint256)' --solc /path/to/solc \
  --solc-attempt /absolute/solc-attempt --solar-attempt /absolute/solar-attempt
```

Saved symbolic runs need identical inputs, an explicit `evmVersion`, and the
`immutableReferences` and `linkReferences` output fields; `--source` then names
a source unit in the saved input, not a file. Solc still compiles the harness,
and Forge with symbolic support and its solver must be installed. The engine
rejects unsupported references and keeps its bounded-agreement, mismatch, and
incomplete statuses. It shares ABI tuple normalization and the saved-attempt
reader with `compare`.

`runtime` runs the curated benchmark suite and keeps its own report and exit
codes, including `--allow-failures` if passed. Only `--gas --start-anvil`
executes runtime checks; `--mode runtime` alone just compiles the runtime
corpus. It owns its suite's compile and execution setup and does not accept
arbitrary Sourcify bytecode. Neither engine's bounded checks prove program
equivalence.

See the [symbolic guide](../../fuzz/fandango/README.md#symbolic-solc-vs-solar-differential)
for execution bounds and the [runtime guide](../../benches/runtime/README.md)
for suite inputs, benchmarks, and result comparisons.
[Debug-info comparisons](../../tests/debug-diff/README.md) use their own
execution-trace workflow.
