# LSP pull-request benchmark

This adapter runs the pinned `asyncswap/lsp-bench` v0.3.3 release against two
Solar binaries. It is an on-demand, portable signal for pull-request discussion,
not an authoritative benchmark or a merge gate.

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

The GitHub workflow accepts only an exact `/bench lsp` comment on an ordinary
pull request from an allowed association. It intentionally has no manual
dispatch entry: a default-branch manual run that checks out and executes a
PR revision would give untrusted build code access to the default branch's
Actions cache authority. The comment-triggered path keeps every job that builds
or executes PR code separate from the trusted renderer.

The workflow downloads the Linux archive described in `upstream.json` and
checks its SHA-256 before extracting it. The adapter deliberately supplies
absolute server commands in generated JSON (which is valid YAML); it never uses
the upstream `commit` or `repo` server modes.

The pinned upstream runner calls its diagnostics workload
`textDocument/diagnostic`, but it does not send that pull-diagnostic request. It
measures from `didOpen` until a diagnostics notification arrives. Generated
upstream configs and raw results retain that selector for provenance, while the
trusted comparison reports the metric as `didOpen/publishDiagnostics`.
`lsp_filter.py` discards unrelated notifications until the fixture's exact
`Main.sol` warning arrives. It also validates initialization and provides the
unmeasured project-ready signal that v0.3.3 expects. The same proxy is used for
both roles and both pass orders, so its fixed overhead is symmetric.

Each comparison runs `initialize`, `didOpen/publishDiagnostics`, `hover`,
`definition`, `references`, `completion`, and `documentSymbol` with five warmups
and ten measured iterations in two passes: base-first and head-first. Every pass
gets a fresh fixture copy, home directory, temporary directory, and XDG
directories. Only a small fixed environment is inherited by child processes.
The trusted renderer requires both roles, both pass orders, every benchmark,
exactly 20 valid samples per role and benchmark, finite positive timings, the
expected JSON-RPC request input, and a correct response for every sample.

The fixture deliberately produces warning 2018 as that indexing-ready marker.
Receiving it ends the `didOpen/publishDiagnostics` measurement and establishes
the precondition for the later request metrics.

`didOpen/publishDiagnostics` is an end-to-end user metric and includes the
production source-change debounce. Analysis-only compiler and symbol-table
rebuild latency is measured independently by the [`solar-lsp` Criterion/CodSpeed
benchmarks](https://codspeed.io/paradigmxyz/solar), which bypass the LSP scheduler
and debounce. Those benchmarks use separate workloads; neither result is derived
by subtracting a fixed delay from the other.

The renderer recomputes nearest-rank p50 and p95 values. A metric is a regression
only when both head percentiles are at least 10 percent slower, and an improvement
only when both are at least 10 percent faster. RSS remains in the raw upstream
results for inspection but does not affect the verdict. Any malformed,
incomplete, failed, or semantically incorrect result makes the whole comparison
inconclusive.

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

`run` writes a versioned `manifest.json`, both generated configs, and both raw
`results.json` files. `render` treats all of those files as untrusted input and
produces a versioned comparison plus the Markdown used for the sticky PR comment.
The raw provenance contains only frozen D/F/M values: `base_sha` and `main_sha`
are D, `pr_head_sha` is F, and `head_sha` and `merge_candidate_sha` are M. The
renderer adds `freshness`, `current_main_sha`, and `current_pr_head_sha` only to
conclusive comparisons. Inconclusive fallbacks may omit those publication-time
fields when the current-state query or benchmark fails.
`raw.schema.json` describes the manifest and `comparison.schema.json` describes
both conclusive and inconclusive trusted outputs.
