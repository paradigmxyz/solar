# Cross-server Solidity LSP benchmark

`solar-lsp-bench` drives multiple Solidity language servers through the same
LSP workloads over stdio or loopback TCP. It validates every response before including a sample in
latency or resource aggregates, so an unsupported or incorrect result cannot
look fast by returning less work.

The checked-in inventory covers Solar, Asyncswap, Nomic Foundation, official
`solc --lsp`, Wake, Juan Blanco, and qiuxiang. `servers.lock.yaml` records the
selected versions, source revisions, installation commands, and available
artifact digests. `fixtures.lock.yaml` records the synthetic fixture and pinned
revisions of Uniswap v4-core, Aave v3.7.0 origin, and Optimism
contracts-bedrock, together with their compiler and dependency provenance.

## Methodology reference

The benchmark design is informed by Asyncswap's
[`lsp-bench` at commit `ca0651f86f430290dacdbeb62c9c6987a3ad6966`](https://github.com/asyncswap/lsp-bench/tree/ca0651f86f430290dacdbeb62c9c6987a3ad6966),
especially its sequential warmup/sampling model and complete file-operation
ordering. This harness remains a separate implementation because the comparison
also requires isolated caches, loopback TCP transport, process-tree resource
accounting, durable per-sample correctness, and authoritative-run gates.

## Runner requirements

Run commands from the repository root. Preparation requires Git, curl, tar,
Rust and Cargo, Node.js and npm, and Python with pip and `venv`. Downloads,
source checkouts, installed servers, compiler artifacts, and generated reports
stay below `target/lsp-bench/`. Network-isolated runs additionally require
`unshare` from util-linux and `ip` from iproute2.

The checked-in server and compiler artifact definitions target x86_64 Linux. The
Wake closure additionally requires Python 3.12, matching Ubuntu 24.04. Its
preparation fails before invoking pip when the host platform, Python minor
version, or requirements-lock digest differs. The full GitHub Actions workflow
runs on the GitHub-hosted `ubuntu-24.04` image and is intentionally a portable,
non-authoritative comparison. It does not require registering or maintaining a
self-hosted runner.

The optional `publish` profile remains available for an operator who has a
separate environment with cgroup v2 delegation, unprivileged user and network
namespaces, and a clean Git worktree. That strict profile is not required by
the GitHub-hosted workflow.

`doctor --publish` checks the platform, cgroup delegation, network namespace,
worktree, server source origins/revisions, fixtures, artifacts, and executable
versions. It cannot establish that hardware and the runner image stayed fixed
across separate workflow runs. Portable runs are useful for cross-server
functional and relative-performance comparisons; their reports retain
`environment.authoritative: false` when the strict accounting gates are absent.

## Prepare and audit

Build the harness, fetch the pinned inputs, and inspect the audit table:

```bash
cargo build --locked -p solar-lsp-bench
target/debug/solar-lsp-bench prepare
target/debug/solar-lsp-bench doctor
```

`prepare` is the only phase expected to access the network. It checks out exact
fixture and server revisions, initializes fixture submodules, downloads
checksum-pinned compiler artifacts, and installs the declared server versions.
The npm servers use checked-in lockfile-v3 manifests with integrity hashes and
`npm ci`. Wake downloads its fully pinned, hashed Python closure into a fresh
wheelhouse, then installs from that wheelhouse with `--no-index` and verifies
the environment with `pip check`. Preparation records a digest of each installed
npm or Python closure under `target/lsp-bench/provenance/installed-closures/`;
`doctor` and `run` reject later mutations to those installed dependencies.
It accepts repeatable `--server ID` and `--fixture ID` filters for debugging.
The same server filter is accepted by `doctor`; result validation accepts it as
well and requires the raw and summary matrices to contain exactly those enabled
manifest servers:

```bash
target/debug/solar-lsp-bench doctor --server solar --server asyncswap
target/debug/solar-lsp-bench validate-results \
  --profile default \
  --server solar \
  --server asyncswap \
  --input target/lsp-bench/publish
```

The ordinary `doctor` command reports `pass`, `unavailable`, `mismatch`, and
`unpinned` checks but does not fail merely because a check is not `pass`. Use
the strict gate before publishing:

```bash
target/debug/solar-lsp-bench doctor --publish
```

Run `doctor` immediately before the benchmark. `run` performs version and
fixture checks needed for execution, but it is not a substitute for the full
artifact and environment audit.

## Run

For a quick functional pass, use the small sampling profile:

```bash
target/debug/solar-lsp-bench run \
  --profile smoke \
  --output target/lsp-bench/smoke
```

The canonical GitHub-hosted full run uses the `default` profile:

```bash
target/debug/solar-lsp-bench doctor
target/debug/solar-lsp-bench run \
  --profile default \
  --output target/lsp-bench/publish
target/debug/solar-lsp-bench report \
  --input target/lsp-bench/publish/summary.json \
  --output target/lsp-bench/publish/COMPARISON.md
```

Do not pass `--allow-failures` for the full comparison. The harness writes all
reports before returning an error, so a server that starts but fails a
correctness assertion remains visible and makes CI fail. Unsupported operations
are recorded separately and are excluded from performance statistics.

`--server ID` and `--workload ID` are repeatable filters. `--repeat N` overrides
the profile's independent process-run counts, while `--timeout-secs N` overrides
its operation and shutdown timeout. These are useful for diagnosis but change
the benchmark protocol and should be disclosed with any resulting report.

Each run rotates server order deterministically and executes samples serially.
Every sample receives a temporary fixture copy and isolated application caches,
pinned `solc` and `forge` aliases, and offline package-manager settings. The
strict `publish` profile re-executes the entire `run` in one private network
namespace. The GitHub-hosted `default` workflow profile leaves network isolation
disabled so the job remains portable; the optional probe records whether the
host happens to provide the stricter capabilities. The harness does not clear
the host page cache.

The workloads cover cold initialization and correctness readiness, warm hover,
definition, references, completion, and document-symbol requests, incremental
edit/save latency, symbol rename, file create/rename/delete notifications, and
fresh, reused, and invalidated caches. Process reports include wall time, CPU,
and peak memory; only cgroup v2 process-tree accounting is authoritative. Raw
process metrics cover the complete server session. Summary resource metrics use
the `session_` prefix and are omitted for warm-request workloads because those
process totals also include startup, indexing, readiness, warmup, and shutdown.
Warm summaries contain individually measured request latencies and, when cgroup
accounting is available, matching per-method `*_cpu_ms` process-tree CPU
metrics. Linux RSS is sampled from every live cgroup member at a 10 ms interval;
cgroup total memory remains a separate secondary metric and is not relabeled as
RSS.

## PR regression signal

`.github/workflows/lsp-bench-pr.yml` runs a bounded, Solar-only comparison on
every PR update and refreshes the baseline after successful `main` pushes. It
uses the tracked synthetic fixture and always invokes the harness with
`--profile pr --server solar`. The profile starts 20 independent LSP sessions;
its five warm workloads collect 300 measured requests after their warmups.

The PR job runs on the repository's ordinary Depot Linux runner. It is a
portable regression signal, not an authoritative cross-server publication.
The workflow checks out the PR head commit at `head.sha`, verifies that revision
before building, and reuses a successful `main`-push baseline artifact
only when that workflow run's head SHA is exactly the PR's `base.sha`. It
validates the summary's Solar source revision, executable digest, configured
repetitions, and passing groups before use. Missing, expired, duplicate, or
invalid artifacts fall back to building and measuring that exact base commit in the PR job;
branch-latest artifacts are never accepted, and stacked PRs always rebuild
their exact base. The PR head checkout supplies the candidate binary and
summary. The comparison requires
matching config and fixture-lock hashes, workload and repetition contracts,
fixture contents, normalized server runtime settings, harness source and
dependency contracts, Rust compiler version, platform shape, and accounting
backends. Role-specific binary paths, revisions, and server-lock hashes are
expected to differ.

The first PR that introduces these workflows has no prior baseline artifact,
and its `workflow_run` commenter is not active until the commenter workflow is
present on the default branch. Its compute job therefore measures the exact base
locally; after the change reaches `main`, the successful push produces the first
reusable baseline and enables comments for subsequent PR runs.

The default comparison deadband is 10 percent. A metric is a regression or
improvement only when both p50 and p95 cross that threshold in the same
direction. Missing baselines, incompatible metadata, incomplete or failed
groups, unequal sample counts, and undersampled metrics are reported as
inconclusive rather than regressions. Reproduce the report locally with:

```bash
target/debug/solar-lsp-bench compare \
  --baseline target/lsp-bench/pr/baseline/summary.json \
  --candidate target/lsp-bench/pr/current/summary.json \
  --output target/lsp-bench/pr/report.md \
  --json-output target/lsp-bench/pr/comparison.json
```

All PR runs upload their comparison as an untrusted artifact. Before the
candidate benchmark runs, the PR job also copies the baseline it selected into
an immutable `lsp-bench-pr-baseline-used` artifact. The separate
`lsp-bench-pr-comment.yml` `workflow_run` workflow verifies that the PR head and
base are still current, downloads that baseline artifact separately from the
untrusted result bundle, checks out the renderer at the trusted workflow commit,
validates the comparison JSON, and updates the sticky comment without executing
PR code with write permissions. If the immutable baseline or result artifact is
missing or invalid, the workflow posts a fixed inconclusive comment.

## Results

`run` atomically writes these files below the selected output directory:

- `samples.json`: schema-versioned raw samples and correctness details;
- `samples.jsonl`: one raw sample per line for streaming analysis;
- `summary.json`: provenance, environment, status counts, and aggregates; and
- `summary.md`: the generated human-readable comparison.

`report` regenerates Markdown from a schema-compatible `summary.json` and is
used to produce `COMPARISON.md`. Aggregates contain only samples with `pass`
status. Keep the raw samples whenever publishing a summary so failures and
outliers remain auditable.

`summary.json` records hashes of the benchmark and lock manifests, the harness
version and Git state, observed server versions and executable/artifact hashes,
source revisions, fixture content hashes, actual compiler hashes,
compiler/dependency metadata, platform, logical CPU count, accounting backends,
and whether all successful measured and setup processes met the
Linux/cgroup/network-isolation requirements. Generated Markdown includes the
same core run, server, and fixture provenance before the comparison table. Its
`environment.authoritative`
flag does not encode CPU model, kernel image, background load, worktree state,
or continuity of the physical runner.

The npm dependency closures are fixed by package lockfiles. The Wake 4.9.0
closure is resolved for x86_64 Linux and Python 3.12 with a cutoff at the UTC day
after that release, then fully pinned in a hashed requirements file. Its explicit
build dependencies are part of the same lock. Every selected Python distribution
must match a declared hash before the offline install. The publication workflow
also captures the resolved npm dependency trees and Python environment for human
audit. A source-built Solar binary has no portable expected digest; its exact
source revision is pinned and the produced binary digest is recorded for that
run.

## CI full comparison

`.github/workflows/lsp-bench.yml` is a manual-only comparison workflow. It runs
on GitHub-hosted `ubuntu-24.04`, accepts dispatches only for the repository's
default branch, does not persist checkout credentials, and pins Rust to `1.96`.
Dispatch defaults to the quick `smoke` profile and the Solar/Asyncswap `core`
server scope; `default` and `all` remain available for an explicitly requested
fuller comparison. The workflow records the optional isolation/accounting probe,
prepares and audits the selected inputs, regenerates `COMPARISON.md`, and uploads
reports, manifests, the probe, doctor audit, and host/tool provenance even when a
correctness check fails. Its results are portable and non-authoritative; the
probe record does not promote any hosted run to a strict publication.

The separate strict `publish` profile is an operator-run path for an environment
that satisfies `doctor --publish`, private-network execution, and complete
cgroup-v2 process-tree accounting. This repository does not currently register
or select such a runner for ordinary CI. The manual
`.github/workflows/lsp-bench-authoritative.yml` workflow is the explicit
operator-triggered entry point; it requires the `[self-hosted, linux, x64,
lsp-bench]` labels, fails on probe/doctor or completeness errors, and writes an
immutable SHA-256 manifest alongside raw JSON/JSONL and `COMPARISON.md`.

The PR workflow is separate from this publication path. It neither runs the
other six servers nor relaxes the `publish` profile's namespace, cgroup, or
authoritative-report requirements.
