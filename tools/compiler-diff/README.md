# Compiler comparisons

Import Solidity inputs, run standard-JSON compilers, and compare their saved
artifacts. Compilation and comparison have separate records: changing comparison
rules does not rerun compilers. The Sourcify database from `scripts/sourcify.py`
is reused without migration. `uv run scripts/sourcify.py` remains a compatible
entry point. Run the commands below from the repository root. Agents should also
follow the [repository guidance](../../AGENTS.md#compiler-comparisons).

```sh
export UV_CACHE_DIR=/tmp/solar-sourcify/uv-cache
export UV_PROJECT_ENVIRONMENT=/tmp/solar-sourcify/python
uv run --project tools/compiler-diff compiler-diff --help
uv run --project tools/compiler-diff compiler-diff self-test
```

All commands accept `--dir` and `--version` **before** the subcommand. Data goes
under `<dir>/<version>`; defaults are `/tmp/solar-sourcify` and `0.8.36`.
Use the environment variables above to keep uv's cache and environment there too.

## Inputs and compilation

```sh
uv run --project tools/compiler-diff compiler-diff --version 0.8.36 sync
uv run --project tools/compiler-diff compiler-diff run \
  --compiler 'solc=/path/to/solc --standard-json' \
  --compiler 'solar=/path/to/solar --standard-json' --limit 20

# Use a separate directory for local repros.
uv run --project tools/compiler-diff compiler-diff \
  --dir /tmp/compiler-repro import-input input.json \
  --target 'C.sol:C'
uv run --project tools/compiler-diff compiler-diff --dir /tmp/compiler-repro run
```

Local imports require Solidity standard JSON with inline source content. Settings
are retained except `outputSelection`: runs request ABI, creation/runtime bytecode,
method identifiers, user documentation and developer documentation. Inputs are
content-addressed with their target; repeated imports are harmless. Sources remain
inline, so source names such as `../C.sol` never become filesystem paths.

`run` stops on the first failure. `--continue-on-failure` processes more attempts;
`--retry-failures` retries cached failures. Any failure returns exit status 1.
Compiler commands accept standard JSON on stdin. Use absolute wrapper argument
paths and change `--tag` when wrapper dependencies or environment change. See
`run --help` for all options. Each attempt retains input, output, compiler identity,
timing, diagnostics and `replay.sh`; replay uses the recorded executable path.

## Saved comparisons

```sh
# Compare the latest attempt from each compiler for every attempted compilation.
uv run --project tools/compiler-diff compiler-diff compare --continue-on-failure
uv run --project tools/compiler-diff compiler-diff compare --check abi --policy exact
uv run --project tools/compiler-diff compiler-diff compare --check userdoc --check devdoc

# Compare particular attempts or raw standard-JSON output files.
uv run --project tools/compiler-diff compiler-diff compare --left /path/to/solc-attempt \
  --right /path/to/solar-attempt --check abi --check methods
```

The default compares ABI interface compatibility and method identifiers. Compiler
names default to `solc` and `solar`; override `--reference` and `--candidate`.
Latest attempts include failures: we never fall back to an older successful run.
Attempt pairs must contain identical inputs. Raw JSON files have no input or
process provenance, so use attempt directories when these checks matter.

Each selected comparison reports `equal`, `different`, `unsupported`, or `error`.
Missing outputs on both sides are unsupported, not equal. Missing outputs on one
side are differences. Failed compilation or malformed outputs are errors. Unrun
corpus entries are excluded; reports include total corpus size and available and
processed pair counts so a partial run cannot be mistaken for full coverage.

The CLI prints PASS, FAIL or KNOWN for each pair. Failures show compiler commands,
versions and hashes, the affected JSON paths, both values, and replay commands.
Default output limits each check to five differences and shortens large values;
`compare --full` prints all differences and complete values. Reports always retain
the full data. The final summary shows processed/available pairs and corpus size.

Reports and mismatch bundles live under `comparisons/<id>/`, indexed by the local
DuckDB database. Bundles copy the original attempt files for both sides and include
JSON-pointer differences with both values. When both sides are available, each
bundle also contains `compare.sh` and a snapshot of its expectation rules. Run
`sh /path/to/bundle/compare.sh` to repeat the comparison from the copied artifacts.
The replay needs this checkout and uv; it creates a new report. Compiler replay
scripts use the recorded executable paths, not bundled binaries. Standard-JSON
compilers may exit 0 while emitting error diagnostics; inspect `replay.stdout.txt`
or pass the outputs to `compare`, which rejects compiler errors.
Reports group differences by comparator and path. Comparisons stop on the first
unexpected difference or incomplete check unless `--continue-on-failure` is set. Both modes return 1 on such results.

| Check | Rules |
| --- | --- |
| `abi --policy interface` | Ignore parameter names and `internalType`; compare functions, returns, mutability, events/indexing, errors, constructors, fallback and receive. |
| `abi --policy exact` | Preserve all ABI fields; ignore object-key and ABI-entry order. |
| `methods` | Compare signature-to-selector maps. |
| `userdoc`, `devdoc` | Compare parsed JSON structurally, including names and descriptions. |

Both ABI policies expand tuple signatures recursively, preserve parameter/component
order and retain duplicate entries. They do not guess the meaning of user-defined
library types or erase enum/type differences. Bytecode, ASTs and storage layouts
are not compared as generic JSON: they need separate semantic rules. ABI agreement
does not imply runtime equivalence.

## Expected divergences

Pass `--expectations expectations.json`. Each rule requires an exact comparator,
rule version, policy, JSON-pointer difference (including both values), and reason:

```json
[
  {
    "comparator": "abi",
    "version": 1,
    "policy": "interface",
    "difference": {
      "path": "/contracts/C.sol/C/value/function:f()/0/stateMutability",
      "kind": "value",
      "left": "view",
      "right": "pure"
    },
    "reason": "Tracked compiler discrepancy; replace with an issue link"
  }
]
```

Copy differences from a report after inspecting them. Matching differences remain
visible and are marked with the reason, but do not fail the comparison. Other
differences still fail, even within the same function. Errors and unsupported
checks cannot be allowlisted. Unused rules are reported, including when a
comparison stops before reaching them. No divergences are accepted by default.
Rules apply to matching contract paths across inputs; use separate expectation
files when a rule is intended for one corpus only.

## Directory corpora and solc tests

Import an existing directory before running and comparing its saved compilations:

```sh
uv run --project tools/compiler-diff compiler-diff --dir /tmp/solc-suite import-directory \
  testdata/solidity/test/libsolidity --format solc --allow-skips
uv run --project tools/compiler-diff compiler-diff --dir /tmp/solc-suite run \
  --compiler 'solc=/absolute/path/to/solc --standard-json' \
  --compiler 'solar=/absolute/path/to/solar --standard-json' --continue-on-failure
uv run --project tools/compiler-diff compiler-diff --dir /tmp/solc-suite compare \
  --continue-on-failure
```

The importer recursively reads `.sol` files. Use `--format solidity` for ordinary
source directories. Each file is an entry point; imports bring in its dependencies.
Identical inputs deduplicate in the database. All emitted contracts are compared;
there is no need to choose a single target contract.

For solc fixtures, the importer splits `==== Source:` sections, loads
`==== ExternalSource:` aliases, resolves imports, and snapshots source text inline.
Dependencies must stay inside the imported directory. Import the common parent
when fixtures reference sibling directories. Escaped import paths and settings
remappings are unsupported. Source-unit names remain JSON keys, never output paths.

`--settings FILE` supplies default standard-JSON settings, with `evmVersion`
defaulting to `osaka`. Explicit solc `EVMVersion` settings select a target or check
that the target meets a restriction. `compileViaYul: also` imports both pipeline
variants; `true` and `false` select one. `revertStrings` is also translated. Other
test settings are reported as unsupported, rather than silently dropped.

Intentional compiler-error tests are retained in the import report and skipped:
ABI and bytecode comparisons do not apply to those tests. This command does not
check diagnostic text or execute upstream runtime expectations. Warnings and
runtime expectation text remain in fixture metadata. Use `cargo tq solc-solidity`
for the existing upstream suite runner.

Every import writes `imports/<id>/report.json`, with per-file status, skip reasons,
compilation IDs, original fixtures, and standard-JSON inputs. New compiler attempts
and comparison failure bundles include `import.json` provenance. The CLI summarizes
coverage and common skip reasons; `--verbose` prints each fixture. Skips cause exit
1 unless `--allow-skips` is explicit; an entirely skipped import always exits 1.
Read the report before interpreting a later successful comparison as suite coverage.

## Fandango campaigns

`fuzz` generates complete Solidity sources, imports each as standard JSON, then
runs the selected compilers and comparisons before advancing to the next case:

```sh
uv run --project tools/compiler-diff compiler-diff --dir /tmp/compiler-fuzz fuzz \
  --seed 7 --count 16 \
  --compiler 'baseline=/absolute/path/to/solc --standard-json' \
  --compiler 'candidate=/absolute/path/to/solar --standard-json'
```

The default grammar is `fuzz/fandango/solidity-source.fan`, targeting
`FandangoSource`. Use `--grammar /path/to/grammar.fan --contract Name` for another
self-contained Solidity-source grammar. The adapter snapshots the grammar file;
external grammar resources are not copied. It uses the Fandango version in `compiler_diff/fandango.py` and the Python
version in the repository's `.python-version`, managed by uv. It sets
`PYTHONHASHSEED` to `--seed`. The generator's managed Python,
tool environment and cache live under `<dir>/fandango-tools/`.

Seed generation from a directory, then run several bounded batches:

```sh
uv run --project tools/compiler-diff compiler-diff --dir /tmp/compiler-fuzz fuzz \
  --grammar fuzz/fandango/solidity-runtime-source.fan --contract FandangoRuntime \
  --initial-population fuzz/fandango/runtime-corpus \
  --population-size 24 --mutation-rate 0.4 --crossover-rate 0.4 \
  --seed 7 --count 64 --rounds 10 \
  --compiler 'solc=/absolute/path/to/solc --standard-json' \
  --compiler 'solar=/absolute/path/to/solar --standard-json'
```

`--initial-population` recursively snapshots `.sol` files, including their original
relative names and content hashes. Seed content and mutation options form part of
the campaign identity. Seeds must parse with the selected grammar; incompatible
seeds fail generation, with stderr and a campaign error report. A directory of
arbitrary solc tests is not automatically compatible with the small runtime grammar.

`--rounds` runs finite batches with consecutive seeds, each retaining its own
campaign report. `--count` bounds outputs per round; the total is at most
`rounds * count`. Fandango evolves its population within a batch. Outputs can include
initial seeds, so a small count does not prove mutation occurred. Each round starts
from the specified seed corpus; there is no automatic promotion of outputs into
later rounds. Failure stops the loop unless `--continue-on-failure` is set; any
failed round still makes the command exit 1. Generation/setup errors and interrupts
stop immediately.

Repeat `--compiler NAME='COMMAND ARGS'` for any standard-JSON compilers. The first
is the default reference, compared with every other compiler. `--reference NAME`
and repeated `--candidate NAME` select a subset. ABI and selectors are checked by
default; `--check`, `--policy`, `--expectations`, and `--full` work as for `compare`.
A compilation failure also fails the case, including when both compilers reject
it; it is not by itself evidence of a compiler divergence.

Use `--settings settings.json` for a standard-JSON settings object. `evmVersion`
defaults to `osaka` unless supplied; choose a supported target when using older
compilers. The runner still selects the outputs required for comparisons.
`--generation-timeout` bounds generation including initial tool installation;
`--timeout` bounds each compiler. Generation must produce exactly `--count`
sources before any are imported.

Add a symbolic check for a function present in every generated contract:

```sh
uv run --project tools/compiler-diff compiler-diff --dir /tmp/compiler-fuzz fuzz \
  --grammar /absolute/path/to/pure-function.fan --contract C --seed 7 --count 8 \
  --compiler 'baseline=/absolute/path/to/solc --standard-json' \
  --compiler 'candidate=/absolute/path/to/solar --standard-json' \
  --symbolic-signature 'f(uint256)' --symbolic-solc /absolute/path/to/solc \
  --symbolic-args '--max-paths 64 --symbolic-timeout 10'
```

Symbolic checks consume the saved attempts for each selected compiler pair.
`--symbolic-solc` compiles the harness. `--symbolic-args` forwards quoted engine
options such as `--include-view`; `--engine-timeout` bounds each engine process.
A missing function or incomplete symbolic check fails the case.

Campaigns live under `<dir>/<version>/fuzz/<id>/`. Their identity includes the
grammar content, generator version, seed, count, target and settings. Repeating
those options verifies and reuses generated files; changed compiler commands
start new compilation jobs without regenerating sources. Campaign comparisons use
the exact attempts selected for that invocation, including older cache hits. Comparisons run again
so changed rules take effect. `--retry-failures` retries failed compiler jobs.
`--continue-on-failure` processes later cases; the default stops at the first
failed case. Any failed case returns exit status 1.

Each campaign retains `grammar.fan`, `campaign.json`, generation logs and source
hashes, generated sources and standard-JSON inputs, and a latest `report.json`
linking cases to compiler attempts, comparison bundles and symbolic reports.
Attempts and mismatch bundles include `generator.json` with the grammar and seed.
The corpus database and run artifacts live in the campaign's `<version>/`
subdirectory. The CLI prints the campaign path; use it as `--dir` with `run`,
`compare`, or `status` to inspect or rerun that corpus independently.
`--generate-only` imports sources without running compilers.

This command handles grammars that emit Solidity sources. The existing
[ABI-value and stateful Fandango runners](../../fuzz/fandango/README.md) retain
their specialized execution and reduction workflows.

## Execution engines

The package also provides entry points for the existing execution engines. Pass
engine options after `--`; use absolute paths for files and executables because
the child process runs in its artifact directory. Reports and logs stay under
`<dir>/<version>/engines/<engine>/<id>`. `--engine-timeout` bounds the whole child
process, while engine-specific timeout flags retain their own meaning.

```sh
uv run --project tools/compiler-diff compiler-diff symbolic -- --help
uv run --project tools/compiler-diff compiler-diff runtime -- --help
uv run --project tools/compiler-diff compiler-diff \
  runtime -- --solc /path/to/solc --solar /path/to/solar \
  --mode runtime --suite micro --tests counter --gas --start-anvil

# Reuse saved artifacts without recompiling the target.
uv run --project tools/compiler-diff compiler-diff symbolic -- --source C.sol --contract C \
  --signature 'f(uint256)' --solc /path/to/solc \
  --solc-attempt /absolute/solc-attempt --solar-attempt /absolute/solar-attempt
```

Saved symbolic runs require identical inputs, an explicit `evmVersion`, and
`immutableReferences` and `linkReferences` output fields. `--source` then names a
source unit inside the saved input, not a file on disk. Solc still compiles the
execution harness; Forge with symbolic support and its solver must be installed.
The engine rejects unsupported references and preserves its bounded-agreement,
mismatch and incomplete statuses. Its ABI tuple normalization and saved-attempt
reader are shared with artifact comparisons.

The runtime adapter runs the curated benchmark suite and retains its native
report and exit semantics, including explicit `--allow-failures` if supplied.
Use `--gas --start-anvil` to execute runtime checks; selecting `--mode runtime`
alone only compiles the runtime corpus. This engine owns its suite-specific
compilation and execution setup; it does not accept arbitrary Sourcify bytecode.
Neither engine's bounded checks are a proof of unrestricted program equivalence.

See the [symbolic guide](../../fuzz/fandango/README.md#symbolic-solc-vs-solar-differential)
for execution bounds and the [runtime guide](../../benches/runtime/README.md) for
suite inputs, benchmarks and result comparisons. [Debug-info comparisons](../../tests/debug-diff/README.md)
use their own execution-trace workflow.
