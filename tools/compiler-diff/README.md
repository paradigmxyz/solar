# Compiler comparisons

Import Solidity inputs, run standard-JSON compilers, and compare their saved
artifacts. Compilation and comparison have separate records: changing comparison
rules does not rerun compilers. The Sourcify database from `scripts/sourcify.py`
is reused without migration. That script remains a compatible entry point.

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
uv run scripts/sourcify.py --version 0.8.36 sync
uv run scripts/sourcify.py run \
  --compiler 'solc=/path/to/solc --standard-json' \
  --compiler 'solar=/path/to/solar --standard-json' --limit 20

# Use a separate directory for local repros.
uv run scripts/sourcify.py --dir /tmp/compiler-repro import-input input.json \
  --target 'C.sol:C'
uv run scripts/sourcify.py --dir /tmp/compiler-repro run
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
uv run scripts/sourcify.py compare --continue-on-failure
uv run scripts/sourcify.py compare --check abi --policy exact
uv run scripts/sourcify.py compare --check userdoc --check devdoc

# Compare particular attempts or raw standard-JSON output files.
uv run scripts/sourcify.py compare --left /path/to/solc-attempt \
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
Reports group differences by comparator
and path. Comparisons stop on the first unexpected difference or incomplete check
unless `--continue-on-failure` is set. Both modes return 1 on such results.

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

## Execution engines

The package also provides entry points for the existing execution engines. Pass
engine options after `--`; use absolute paths for files and executables because
the child process runs in its artifact directory. Reports and logs stay under
`<dir>/<version>/engines/<engine>/<id>`. `--engine-timeout` bounds the whole child
process, while engine-specific timeout flags retain their own meaning.

```sh
uv run scripts/sourcify.py symbolic -- --help
uv run scripts/sourcify.py runtime -- --help
uv run scripts/sourcify.py runtime -- --solc /path/to/solc --solar /path/to/solar \
  --mode runtime --suite micro --tests counter --gas --start-anvil

# Reuse saved artifacts without recompiling the target.
uv run scripts/sourcify.py symbolic -- --source C.sol --contract C \
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
