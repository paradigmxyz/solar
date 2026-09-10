# Deeper literal orientation

Commit `a26ab402` extends the existing late EVM IR literal-orientation pass:

```text
push K; swap1; swap d1; ...; swap dn; binary
  -> swap (d1-1); ...; swap (dn-1); push K; reversed binary
```

Each deeper swap leaves the literal immediately below the top. Deferring its
push preserves the entire stack and removes one SWAP; the other depths decrease
and cannot widen. The matcher accepts repeated and decreasing depths, checks
canonical metadata, and rejects unencodable depths before decrementing them.
Invalid Amsterdam depth 236 must not become valid depth 235. The existing
observer, computed-control and indexed-jump exclusions remain.

This adds 18 physical Rust lines to one existing pass, with no new pipeline
invocation, analysis, scheduler state or assembler transform. The backend now
has 17,630 physical lines in 48 Rust files, 17,008 fewer than the 34,638-line
deletion inventory. Counts include comments, blanks and local tests. Private
scheduler cost queries share this helper, so a local stack proof alone does not
establish whole-program profitability.

## Matched measurements

The same-checkout debug baseline precedes production edits. Frozen compilers
are `solar-address-lifetimes-baseline` (SHA-256
`04d5d37a4f2aa663ba5c9d7b6abc59450309b2d3f0b0fce65bd84dde5c8d1bea`)
and `solar-address-lifetimes-orientation` (SHA-256
`62a61b666d548112087b6a623ea039f5f30165aa9476636cf52ccd3643d019ae`).
The manifests pin all 150 codegen source files. The benchmark uses the current
in-repository workflow, pinned solc 0.8.36, hot gas, retained reference rows,
and separate baseline/candidate JSON. Both agents stopped native work during
the timed runs.

| Corpus | Matched objects | Smaller | Larger | Creation + runtime delta |
| --- | ---: | ---: | ---: | ---: |
| UI Gas | 2,576 | 252 | 0 | -948 bytes |
| UI Size | 2,576 | 346 | 0 | -1,426 bytes |
| Heavy projects | 3,344 | 185 | 0 | -1,046 bytes |
| Runtime Gas | 30 | 2 | 0 | -10 bytes |
| Runtime Size | 30 | 4 | 0 | -14 bytes |

These corpora overlap and must not be summed. The UI comparison retains all
846 primary sources, 1,692 rows, 18 expected diagnostic failures and 40
unlinked objects. Unresolved library placeholders are compared as encoded
strings with placeholder identities, not misrepresented as executable bytes.
No equal-size object changes its bytecode. The heavy comparison retains all
1,672 contracts from nine projects; six projects are freshly captured and
three reuse raw outputs only after exact input and complete-output fingerprint
joins to the frozen producer. Every object is checked individually.

Both optimization modes retain all 15 runtime cases, 175 identical gas-call
labels and values, and 139 identical execution observations. The Full workflow
also retains all nine heavy project IDs. The only changed diagnostic text is
the smaller measured size in two existing LibString size warnings.

The geometric mean of per-case compiler medians changes +0.143% in Full and
-2.820% in Size; peak RSS geometric means change -0.226% and +0.163%.
Full elapsed time increases from 344.642 to 365.855 seconds. The benchmark's
existing ten-second cutoff changes OpenZeppelin from one baseline sample
(10.114 seconds) to three candidate samples (9.992, 9.968, 10.037 seconds).
This unequal sampling is retained explicitly; neither elapsed time nor these
medians establish a causal compiler speed claim. Per-case increases and all
raw samples remain in the audit.

## Verification and limits

All 1,577 workspace tests pass, with two existing skips, including the UI and
Foundry suites. Nightly formatting and Clippy pass. Four new EVM IR fixtures
cover longer and repeated runs, reversed comparisons, metadata, observer
exclusion, extended depths and invalid depth refusal. Their eight snapshot
legs pass independently against the frozen compiler; the complete orientation
directory passes 24 cases.

The first workspace run found exactly three existing snapshot differences.
Independent review identified three literal ADD rewrites in linked library
lowering, three literal AND rewrites in pre-Cancun copying, and one literal
AND rewrite in the protected-writer fixture. The disassembly's other changes
are named jump targets shifted by one to three bytes, without width changes.
Only those snapshots were updated. Their sources, FileCheck assertions and
runtime oracles remain unchanged; original snapshots and failure logs are
retained. No test was removed or relaxed.

Earlier tuple-streaming, address-lifetime and arithmetic-cancellation trials
were rejected for individual object growth. Their sources, frozen compilers,
runtime results and failures are archived separately. Their symbolic runs
timed out and establish no agreement; they are not evidence for this commit.
Prior-art research remains in [the stack scheduling report](evm-stack-scheduling-research.md).

Positive sealed heavy size debt falls by 914 bytes to 30,280,048 across 1,007
objects and 524 contracts. This sums individual increases, including embedded
children, rather than net corpus growth. The 15 hot-gas regressions against
retained main `25b0c078` persist. General source-memory ownership and full
rewrite acceptance remain open.

Evidence is retained under
`target/codegen-bench/evm-rewrite-candidate/address-lifetimes-20260910/`.
The final independent audit is `orientation-only/audit/final-receipt.json`,
SHA-256 `03de06ad7253fd9d33aae34d72d05146ca3f9cafb501af25ba7d831c9b1bda21`.
