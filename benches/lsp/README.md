# LSP pull-request benchmark

This adapter runs the pinned `asyncswap/lsp-bench` v0.3.3 release against two
Solar binaries. It is an on-demand, portable signal for pull-request discussion,
not an authoritative benchmark or a merge gate.

The GitHub workflow accepts only an exact `/bench lsp` comment on an ordinary
pull request from an allowed association. It intentionally has no manual
dispatch entry: a default-branch manual run that checks out and executes a
PR revision would give untrusted build code access to the default branch's
Actions cache authority. The comment-triggered path keeps the untrusted
compute job separate from the trusted renderer.

The workflow downloads the Linux archive described in `upstream.json` and
checks its SHA-256 before extracting it. The adapter deliberately supplies
absolute server commands in generated JSON (which is valid YAML); it never uses
the upstream `commit` or `repo` server modes.

The pinned upstream runner accepts the first notification after `didOpen` as a
diagnostic response. `lsp_filter.py` compensates by discarding unrelated
notifications until the fixture's exact `Main.sol` warning arrives. It also
validates initialization and provides the unmeasured project-ready signal that
v0.3.3 expects. The same proxy is used for both roles and both pass orders, so
its fixed overhead is symmetric.

Each comparison runs `initialize`, `textDocument/diagnostic`, `hover`,
`definition`, `references`, `completion`, and `documentSymbol` with five warmups
and ten measured iterations in two passes: base-first and head-first. Every pass
gets a fresh fixture copy, home directory, temporary directory, and XDG
directories. Only a small fixed environment is inherited by child processes.
The trusted renderer requires both roles, both pass orders, every method,
exactly 20 valid samples per role and method, finite positive timings, the
expected JSON-RPC request input, and a correct response for every sample.

The fixture deliberately produces warning 2018 as that indexing-ready marker.
The compatibility work happens before method timing and does not add work to
measured requests.

The renderer recomputes nearest-rank p50 and p95 values. A method is a regression
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
  --head-repository contributor/solar \
  --workflow-repository paradigmxyz/solar \
  --pr-number 1234 \
  --base-sha BASE_SHA \
  --head-sha HEAD_SHA \
  --run-url https://github.com/paradigmxyz/solar/actions/runs/123 \
  --output target/lsp-bench/raw

python3 benches/lsp/benchmark.py render \
  --input target/lsp-bench/raw \
  --repository paradigmxyz/solar \
  --head-repository contributor/solar \
  --workflow-repository paradigmxyz/solar \
  --pr-number 1234 \
  --base-sha BASE_SHA \
  --head-sha HEAD_SHA \
  --run-url https://github.com/paradigmxyz/solar/actions/runs/123 \
  --report target/lsp-bench/report.md \
  --comparison target/lsp-bench/comparison.json
```

`run` writes a versioned `manifest.json`, both generated configs, and both raw
`results.json` files. `render` treats all of those files as untrusted input and
produces a versioned comparison plus the Markdown used for the sticky PR comment.
`raw.schema.json` describes the manifest and `comparison.schema.json` describes
both conclusive and inconclusive trusted outputs.
