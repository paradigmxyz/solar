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
bytecode, method identifiers, userdoc, and devdoc. `run --ast` (also `fuzz`)
adds the AST of every source (`"": ["ast"]`) for the `ast` check; solc's AST
export is slow, so it is off by default and part of the job identity. Inputs are content-addressed
with their target, so repeat imports are harmless. Sources stay inline; names
such as `../C.sol` never become filesystem paths.

Compilers read standard JSON on stdin. Give wrapper arguments absolute paths,
and change `--tag` when wrapper dependencies or environment change. `run` stops
at the first failure unless `--continue-on-failure` is set; `--retry-failures`
retries cached failures. Any failure exits 1. See `run --help` for all options.
Each attempt keeps its input, output, compiler identity, timing, diagnostics,
and a `replay.sh` that uses the recorded executable path.

Every attempt also records wall time, peak RSS, and whether it timed out, in
`result.json` and the `attempts` table (`seconds`, `max_rss_kb`, `timed_out`;
attempts from older runs have none). Peak RSS is the largest resident set of
the compiler or any descendant it waited for, taken with `wait4` from a small
helper process, because a child forked from the large runner would inherit the
runner's own peak; the numbers include the helper's own 11 MB or so on Linux. A
timeout counts as a failure, as does an attempt whose peak RSS exceeds
`--max-rss-mb`, which is checked after the compiler exits. Both limits are part
of the job identity (an unset RSS limit changes nothing). A timed-out attempt
has no RSS, since its process group is killed. `run` and `status` end with the
timeouts and the ten worst inputs by time and by peak RSS, each with every
compiler's numbers on the same input, for example:

```text
timeouts: 1
  solar 0x1a2b: /tmp/solar-sourcify/0.8.37/failures/9f3c
worst by time:
  0x1a2b: solar 120.0 s, solc 1.1 s 85 MB
```

## Saved comparisons

```sh
# Latest attempt from each compiler, for every compilation in the corpus.
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
| `ast` | Compare `sources.<name>.ast` node by node after renumbering IDs; see [AST comparisons](#ast-comparisons). Needs `run --ast`. |

Both ABI policies expand tuple signatures recursively, keep parameter and
component order and duplicate entries, and neither guess the meaning of
user-defined library types nor erase enum/type differences. Bytecode and
storage layouts need their own semantic rules, so `compare` skips them. ABI
agreement does not imply runtime equivalence.

Each check reports `equal`, `different`, `unsupported` (outputs missing on both
sides), or `error` (failed compilation or malformed output); output missing on
one side is a difference. A compilation that a compiler never ran is an `error`,
so a partial run (`run --limit`) fails; the report counts corpus size and
available and processed pairs. The CLI prints PASS, FAIL, or KNOWN per pair; failures show compiler
commands, versions and hashes, JSON paths, both values, and replay commands. It
shows five differences per check and shortens large values unless `--full` is
set; reports keep everything, grouped by comparator and path. The summary gives
processed/available pairs and corpus size. Comparisons stop at the first
unexpected difference or incomplete check unless `--continue-on-failure` is
set; either way these exit 1.

Reports and mismatch bundles live in `comparisons/<id>/`, indexed by the local
DuckDB database. Bundles copy both sides' attempt files (`stdout.txt` is a hard
link, so do not edit it in place) and list JSON-pointer differences with both
values. When both sides exist, a bundle also holds
`compare.sh` and a snapshot of its expectation rules; `sh <bundle>/compare.sh`
repeats the comparison from the bundle, needs this checkout and uv, and writes
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

## AST comparisons

```sh
compiler-diff run --ast --compiler 'solc=...' --compiler 'solar=...'
compiler-diff compare --check ast --continue-on-failure [--ast-rules rules.json]
```

Node IDs never match between compilers, so the check renames them first. Each
node's ID becomes the JSON pointer of the node in its output, on each side, and
every ID-valued field follows: `id`, `referencedDeclaration`,
`overloadedDeclarations`, `scope`, `sourceUnit`, `linearizedBaseContracts`,
`contractDependencies`, `baseFunctions`, `baseModifiers`, `usedEvents`,
`usedErrors`, `functionReturnParameters`, `assignments`, `function`,
`exportedSymbols`, `externalReferences[].declaration`, the keys of
`internalFunctionIDs`, and the IDs in `typeIdentifier` of `t_struct`,
`t_contract`, `t_enum`, `t_userDefinedValueType` and `t_super` types and in
`t_module_N`; `typeString` is compared as written. Solc's negative builtin IDs
stay. The source index at the end of every span (`src`, `nameLocation`, ...)
becomes the name of the source, since compilers number sources in their own
order; source IDs themselves are not compared. A reference agrees only if both
sides point at the node in the same place, so a wrong declaration, a wrong
builtin, or an ID with no node (shown as `?N@left` or `?N@right`) differs. A
duplicate, negative or non-integer ID is an error. The sets solc sorts by ID (`contractDependencies`, `baseFunctions`, `baseModifiers`,
`usedEvents`, `usedErrors`) compare sorted; `linearizedBaseContracts`,
`overloadedDeclarations` and every other list keep their order.

Paired nodes always share a type and have matching IDs, so their `nodeType` and
`id` fields are not counted unless one side lacks the ID.

Both ASTs are walked in step. A field that holds child nodes compares only the
child's `nodeType`; the children are compared in turn. Children are paired by
position, so a missing statement shows as a length difference of its list and
the statements after it are compared against their neighbors. Because IDs follow
position, one missing node also makes every later reference to a node of that
list differ; look for the first difference in the sampled paths. A node whose
type differs, or that is missing, is not entered. A field absent on one side, or
a source without `ast`, is `missing_left` (the reference lacks it) or
`missing_right`; a missing list item shows as `different` on its list.

The report holds, for each `(nodeType, field)`, the nodes compared, `equal`,
`different`, `missing_left`, `missing_right` and `ignored`, and for each node
type the node count of both sides. Node types that the reference has and the
candidate never emits are `never_emitted`. `report.json` sums them under `ast`
and keeps nothing per pair; the CLI prints the
agreement, how many reference nodes were paired (nodes under a missing field or
list item are not), the never-emitted types and the ten least-equal fields.
`differences` keeps 20 samples per field with normalized values and pointers;
the rest are counted as `omitted`, which expectations cannot excuse.
`difference_groups` counts all differences, sampled or not, by
`nodeType.field`.

`--ast-rules FILE` lists accepted differences, each with a version (1), the
`nodeType` (or `*`), the `field`, an `action` and a `reason`:

```json
[{"version": 1, "nodeType": "FunctionCall", "field": "isPure",
  "action": "ignore", "reason": "Solar does not track purity; docs/SOLC_DIVERGENCE.md"}]
```

`ignore` drops the field on both sides; `presence` compares only whether it is
present. Both skip the nodes below the field. Reports count what each rule
applied to and list unused rules. No rule applies by default. A rule is a
permanent, wide exception, so give each an entry in
[SOLC_DIVERGENCE.md](../../docs/SOLC_DIVERGENCE.md), and keep the file with the
corpus it applies to. Bundles keep a copy as `ast-rules.json` and `compare.sh`
passes it.

## Directory corpora and solc tests

```sh
compiler-diff --dir /tmp/solc-suite import-directory \
  testdata/solidity/test/libsolidity --format solc --allow-skips
compiler-diff --dir /tmp/solc-suite run --continue-on-failure \
  --compiler 'solc=/absolute/path/to/solc --standard-json' \
  --compiler 'solar=/absolute/path/to/solar --standard-json -Zvalidate-ir=true'
compiler-diff --dir /tmp/solc-suite compare --continue-on-failure
```

`import-directory` reads `.sol` files recursively, with `--format solc`
(default) for solc fixtures or `--format solidity` for plain sources. Each file
is an entry point that pulls in its imports. Plain sources keep their path
relative to the imported directory as the source-unit name, so `test/A.t.sol`
importing `../src/A.sol` resolves to `src/A.sol`. Identical inputs deduplicate.
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
  --compiler 'solar=/absolute/path/to/solar --standard-json -Zvalidate-ir=true'
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
  --compiler 'candidate=/absolute/path/to/solar --standard-json -Zvalidate-ir=true' \
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
