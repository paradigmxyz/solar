# Cross-server Solidity LSP benchmark

`solar-lsp-bench` drives the same manifest-defined LSP workloads against a
locked inventory of Solidity language servers. Every response is checked for
runtime correctness before its sample contributes to latency or resource
statistics, so an unsupported or incorrect response cannot appear fast by
doing less work.

The v1 inventory is the **core4** set: Solar, Asyncswap, Nomic Foundation, and
official `solc --lsp`. All servers use stdio. `servers.lock.yaml` records their
versions, source revisions, installation commands, and artifact digests. The
`solar` entry points at the workspace compiler binary; a workflow run
supplies that exact binary, source revision, and executable digest as provenance.

`fixtures.lock.yaml` pins the synthetic fixture and the Uniswap v4-core, Aave
v3.7.0 origin, and Optimism contracts-bedrock corpora, including compiler and
dependency locks. `benchmark.yaml` is the single source of truth for profiles,
fixtures, servers, and workloads.

## Methodology

The workload design is informed by Asyncswap's
[`lsp-bench` commit `ca0651f86f430290dacdbeb62c9c6987a3ad6966`](https://github.com/asyncswap/lsp-bench/tree/ca0651f86f430290dacdbeb62c9c6987a3ad6966),
especially its sequential warmup and sampling model. This executor additionally
keeps fixture copies, HOME/XDG/package-cache isolation, deterministic server
rotation, capability negotiation, server-initiated request handling, durable
per-sample correctness, cache lifecycle controls, and process-tree accounting.

The workloads cover cold initialization/readiness, warm hover, definition,
references, completion, and document-symbol requests; incremental edit/save;
symbol rename; file create/rename/delete notifications; and fresh, reused, and
invalidated caches. Unsupported operations are recorded separately and are
excluded from performance aggregates. Failed samples remain in the raw output
and determine the run status; `--allow-failures` lets an exploratory run finish
successfully while retaining those failures.

## Requirements

Run commands from the repository root. Preparation requires Git, curl, tar,
Rust and Cargo, Node.js, and npm. Downloads, installed servers, compiler
artifacts, and generated reports stay below `target/lsp-bench/`.

The checked-in external artifacts target x86_64 Linux. The npm server uses its
checked-in lockfile-v3 manifest and `npm ci`. Network-isolated profiles also
require `unshare` from util-linux and `ip` from iproute2.

## CLI

Build the harness, fetch the locked inputs, and inspect the audit table:

```bash
cargo build --locked -p solar-lsp-bench
target/debug/solar-lsp-bench prepare
target/debug/solar-lsp-bench doctor
```

`prepare` is the only phase expected to access the network. It checks out
external fixture and server revisions, downloads checksum-pinned compiler
artifacts, and installs declared server versions. It accepts repeatable
`--server ID` and `--fixture ID` filters. `doctor` accepts the server filter and
checks executable versions, artifacts, source checkouts, fixtures, and host
accounting capabilities.

The profiles in `benchmark.yaml` are:

| Profile | Warmup | Samples | Cold | Lifecycle | Scope |
| --- | ---: | ---: | ---: | ---: | --- |
| `smoke` | 1 | 2 | 1 | 1 | local synthetic subset |
| `pr-smoke` | 5 | 20 | 4 | 4 | synthetic scenarios |
| `full` | 10 | 100 | 8 | 8 | all scenarios for all core4 fixtures |

The CLI defaults `run` to `pr-smoke`; pass `--profile full` for the complete
core4 matrix. A run publishes all result views in one operation:

```bash
target/debug/solar-lsp-bench run \
  --profile smoke \
  --output target/lsp-bench/smoke

target/debug/solar-lsp-bench run \
  --profile pr-smoke \
  --server solar --server asyncswap --server nomic-foundation --server solc \
  --output target/lsp-bench/pr-smoke

target/debug/solar-lsp-bench run \
  --profile full \
  --output target/lsp-bench/full
```

For a workflow-built compiler executable, add the paired provenance options
`--solar-binary PATH --solar-revision SHA`. `--server ID` and `--workload ID`
are repeatable filters; `--repeat N` and `--timeout-secs N` override the
profile's process-run count and operation timeout.

The `report` command remains an offline Markdown renderer for an existing
summary and is useful when a report needs to be regenerated locally:

```bash
target/debug/solar-lsp-bench report \
  --input target/lsp-bench/full/summary.json \
  --output target/lsp-bench/full/summary.md
```

## CI

`.github/workflows/lsp-bench.yml` runs `pr-smoke` against core4 and the
synthetic fixture for pull requests. The PR job is reference-only and tolerates
sample failures. A manual `full` dispatch on `main` runs the complete matrix
strictly. Both jobs build the harness and compiler from the checked-out commit,
record provenance, and upload the run's summary and raw samples. The generated
`summary.md` is appended directly to the GitHub job summary; no second
validation or rendering phase is required.

## Accounting

Every run is portable by default. Process reports include wall time, CPU, and
peak memory. Linux cgroup v2 process-tree metrics are the authoritative backend
for session and per-request accounting; a portable fallback remains visible
when cgroups are unavailable. The summary records whether the environment met
the authoritative requirements and retains the observed accounting backends.

## Results and provenance

`run` atomically writes `summary.json`, `samples.json`, `samples.jsonl`, and
`summary.md` below the selected output directory. Raw samples retain failures
and outliers for audit; aggregates include only samples with passing status.

Summaries bind the benchmark and lock-manifest digests to harness and Git
state, observed server versions and executable/artifact hashes, Solar runtime
source provenance, fixture content hashes, compiler and dependency metadata,
Node.js/npm versions for npm-backed servers, platform details, accounting
backends, and the final status. The npm dependency closure is fixed by the
checked-in package lock and its integrity hashes. Atomic publication prevents
partially written reports from being mistaken for a complete run.
