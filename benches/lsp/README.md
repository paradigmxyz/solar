# LSP pull-request benchmark

This adapter runs pinned `asyncswap/lsp-bench` v0.3.3 source with the small,
checksum-pinned `lsp-bench-direct.patch` adapter against two compiler binaries.
It is an on-demand, portable signal for pull-request discussion, not an
authoritative benchmark or a merge gate.

The workflow freezes the command-time `main` commit (D), PR head (F), and
GitHub test merge commit (M). It accepts M only when its parents are exactly
`[D, F]`, then compares D with M (`comparison_mode: main-merge-candidate`).
The D and M compilers build concurrently in separate credentialless jobs, and
the benchmark job downloads those exact binary artifacts before measuring them.
The performance verdict is report data only; it does not gate merging or make
the workflow fail.

After the benchmark finishes, the trusted renderer reads the current `main`
and PR-head SHAs and labels the frozen measurement `current`, `main-advanced`,
or `superseded`. A changed PR head takes precedence over a changed main tip.
Stale measurements keep their original performance verdict and full table, but
the report marks them as reference-only or historical and recommends rerunning.

The tip-base GitHub workflow accepts only an exact `/bench lsp` comment on an
ordinary pull request from an allowed association. It intentionally has no
manual dispatch entry: a default-branch manual run that checks out and
executes a PR revision would give untrusted build code access to the default
branch's Actions cache authority. The comment-triggered path keeps every job
that builds or executes PR code separate from the trusted renderer.

The cross-server workflow runs the `pr-smoke` core4/synthetic benchmark on every
pull request. To rerun that benchmark manually, add an exact `/bench
cross-server` comment to the pull request. Its read-only benchmark job checks
out the frozen PR head, while a separate job with comment permissions publishes
the report without checking out pull request content.

The workflow downloads the source archive described in `upstream.json`, checks
its SHA-256, checks the in-repository adapter SHA-256, applies that adapter, and
builds the runner. Generated JSON (which is valid YAML) invokes each absolute
compiler binary directly with `lsp`; it never uses the upstream `commit` or
`repo` server modes and does not put a proxy in the measured process path.

The pinned upstream runner calls its diagnostics workload
`textDocument/diagnostic`, but it does not send that pull-diagnostic request. It
measures from `didOpen` until a diagnostics notification arrives. Generated
upstream configs and raw results retain that selector for provenance, while the
trusted comparison reports the metric as `didOpen/publishDiagnostics`.
The pinned adapter makes the runner wait for the fixture's exact `Main.sol`
warning 2018, requires `initialize` to succeed with a `capabilities` object, and
treats that completed-analysis diagnostic as the readiness boundary instead of
waiting again for an earlier progress event. It also records the compiler's own
version and RSS and serializes measured millisecond samples without the upstream
two-decimal rounding.

Each comparison runs `initialize`, `didOpen/publishDiagnostics`, `hover`,
`definition`, `references`, `completion`, and `documentSymbol` with five warmups
and ten measured iterations per session. It runs five independent sessions in
each server-order stratum, base-first and head-first, for ten sessions and 100
measured samples per role and metric. Which stratum runs first alternates by
round. Every session gets a fresh fixture copy, home directory, temporary
directory, and XDG directories. Only a small fixed environment is inherited by
child processes.

The raw v3 manifest identifies every session and order stratum, and the patched
runner retains each sample as an unrounded floating-point millisecond value.
The trusted renderer requires all ten sessions, both roles, every benchmark,
ten finite positive samples per role and metric in every session, the expected
JSON-RPC request input, and a correct response for every sample. Request repeats
inside one runner invocation are not treated as independent statistical
sessions.

Schema v3 is an explicit break from v2: it adds session identity and precision
to raw provenance and adds session counts, absolute deltas, order-stratified
confidence intervals, and the absolute threshold to trusted comparisons. The
renderer rejects older artifacts instead of guessing how their pooled samples
map to independent sessions.

The fixture deliberately produces warning 2018 as that indexing-ready marker.
Receiving it ends the `didOpen/publishDiagnostics` measurement and establishes
the precondition for the later request metrics.

`didOpen/publishDiagnostics` is an end-to-end user metric and includes the
production source-change debounce. Analysis-only compiler and symbol-table
rebuild latency is measured independently by the [`solar-lsp` Criterion/CodSpeed
benchmarks](https://codspeed.io/paradigmxyz/solar), which bypass the LSP scheduler
and debounce. Those benchmarks use separate workloads; neither result is derived
by subtracting a fixed delay from the other.

The renderer first computes nearest-rank p50 and p95 values inside each session.
The displayed base and head values are means of those per-session percentiles;
they are descriptive values, not percentiles from 100 pooled request samples.
Within each order stratum, the renderer pairs base and head session summaries
and computes an exact, deterministic paired-bootstrap 95% confidence interval.
A metric is a regression only when the lower bounds for both absolute and
percentage changes, for both p50 and p95, in both order strata, reach at least
1.0 ms and 10 percent. An improvement requires the corresponding upper bounds
to reach -1.0 ms and -10 percent. This absolute threshold prevents a tiny
sub-millisecond change from being classified merely because its percentage is
large. Published point deltas round toward zero and confidence bounds round
outward, so presentation rounding cannot make a below-threshold result appear
to pass the rule. RSS remains in the raw results for inspection but does not
affect the verdict. Any malformed, incomplete, failed, or semantically incorrect
result makes the whole comparison inconclusive.

Run from the repository root:

```bash
python3 benches/lsp/benchmark.py run \
  --lsp-bench /path/to/lsp-bench \
  --base-binary /path/to/base/solar \
  --head-binary /path/to/head/solar \
  --repository paradigmxyz/solar \
  --pr-head-repository contributor/solar \
  --workflow-repository paradigmxyz/solar \
  --pr-number 1234 \
  --base-sha MAIN_SHA \
  --main-sha MAIN_SHA \
  --head-sha MERGE_CANDIDATE_SHA \
  --pr-head-sha PR_HEAD_SHA \
  --merge-candidate-sha MERGE_CANDIDATE_SHA \
  --run-url https://github.com/paradigmxyz/solar/actions/runs/123 \
  --output target/lsp-bench/raw

CURRENT_MAIN_SHA=CURRENT_MAIN_SHA \
CURRENT_PR_HEAD_SHA=CURRENT_PR_HEAD_SHA \
  python3 benches/lsp/benchmark.py render \
  --input target/lsp-bench/raw \
  --repository paradigmxyz/solar \
  --pr-head-repository contributor/solar \
  --workflow-repository paradigmxyz/solar \
  --pr-number 1234 \
  --base-sha MAIN_SHA \
  --main-sha MAIN_SHA \
  --head-sha MERGE_CANDIDATE_SHA \
  --pr-head-sha PR_HEAD_SHA \
  --merge-candidate-sha MERGE_CANDIDATE_SHA \
  --run-url https://github.com/paradigmxyz/solar/actions/runs/123 \
  --report target/lsp-bench/report.md \
  --comparison target/lsp-bench/comparison.json
```

`run` writes a versioned `manifest.json` plus one generated config and raw
`results.json` for each of the ten sessions. `render` treats all of those files
as untrusted input and produces a versioned comparison plus the Markdown used
for the sticky PR comment.
The raw provenance contains only frozen D/F/M values: `base_sha` and `main_sha`
are D, `pr_head_sha` is F, and `head_sha` and `merge_candidate_sha` are M. The
renderer adds `freshness`, `current_main_sha`, and `current_pr_head_sha` only to
conclusive comparisons. Inconclusive fallbacks may omit those publication-time
fields when the current-state query or benchmark fails.
`raw.schema.json` describes the manifest and `comparison.schema.json` describes
both conclusive and inconclusive trusted outputs.
