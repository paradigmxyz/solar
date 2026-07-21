# solar-lsp

Solar LSP definitions and implementation.

## Benchmarks

Run the LSP benchmarks locally with:

```console
cargo bench -p solar-lsp --bench lsp --features bench
```

The current suite measures in-memory project analysis, edits, and queries. Loading manifests and
corpora from disk, resolving anchors, constructing requests, and preflight correctness checks stay
outside the timed closure. Use stable `lsp/<operation>/<case>` names and add new cases instead of
renaming existing benchmark IDs.

To add a scenario, prepare the project outside the timed closure, resolve its request anchors, run
the request once and assert the expected response, then register only the analysis, edit, or query
as the measured operation. Full filesystem, JSON-RPC, and process latency belongs in a future
walltime benchmark.
