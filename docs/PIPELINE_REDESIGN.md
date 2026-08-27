# IR pipeline design

The MIR pipeline separates representation contracts from optimization history.
Each lowering pass owns one phase transition, and the validator checks the
operations and entry shapes legal at that boundary. Stable stages group passes
by purpose; stable checkpoints name the resulting IR without tying tools to an
internal pass order.

## MIR boundaries

| Phase | Contract |
| --- | --- |
| `built` | Typed semantic MIR with implicit ABI handling and no dispatch. |
| `abi` | External entries take no callable MIR arguments and terminate through ABI returndata. Retained argument types preserve lazy calldata values. |
| `dispatch` | One ordinary `entry` routes each selector, receive, and fallback entry. Internal-only libraries may omit it. |
| `intrinsics-lowered` | Mapping hashes, ABI encoders, aggregates, slices, and memory objects have become scalar memory, storage, loop, and CFG MIR. |
| `target-lowered` | Free-memory-pointer allocation, memory zeroing, immutables, and target-version operations have been lowered. Proven constant-size deferred allocations may remain as backend placement markers. |
| `evm-shaped` | Non-returning framed calls are explicit `tail_call` edges and the backend call protocol is complete. |

Text MIR records the phase, module kind, selector and entry attributes, ABI
return layout, retained argument types, and frame sizes. Parsing an intermediate
checkpoint therefore preserves the state needed to resume the default pipeline.
The retired `optimized` and `memory-lowered` phase names are intentionally
rejected because they did not define complete representation contracts.

## MIR stages

| Stage | Classification and purpose |
| --- | --- |
| `optimize-semantic-hashes` | Representation-independent scalar, CFG, and hash reuse while mapping slots remain semantic. |
| `lower-mapping-hashes` | Semantic lowering of mapping hashes. |
| `optimize-source-low-level` | Source MIR scalar, loop, memory, storage, CFG, inlining, and function cleanup. Gas-only and size-only wrappers keep objective choices explicit. |
| `lower-abi` | Semantic materialization of argument-free external entries. |
| `optimize-abi` | Function and dead-code cleanup over generated ABI entries. |
| `plan-allocations` | Proves which constant, local allocations may use backend placement. |
| `lower-codecs` | Lowers ABI encoders and aggregates, cleans their helpers, then lowers slices. |
| `lower-dispatch` | Materializes selector, receive, and fallback routing. |
| `lower-intrinsics` | One coordinated representation boundary for mappings, encoders, aggregates, slices, and memory objects that remain. |
| `optimize-low-level` | GVN, copy elimination, and ADCE over arithmetic and memory exposed by intrinsic lowering. |
| `lower-target` | Lowers immutables, coalesces allocations, and coordinates allocation, zeroing, and target-version lowering. |
| `optimize-target-generated` | SCCP folds arithmetic and CFG introduced by target lowering. |
| `evm-shape` | Makes the backend call protocol explicit. |
| `final-cleanup` | Removes dead MIR after call shaping. |
| `schedule` | Orders MIR for physical EVM stack scheduling. |

`lower-intrinsics` and `lower-target` combine transforms that share one
representation boundary. They keep internal ordering but share phase ownership,
analysis invalidation, and one all-or-nothing output contract. Profitability
passes, target-version choices, and diagnostics remain separate.

## EVM IR stages

Required target legalization runs for default and custom pipelines, including
`-O none`. The optimized pipeline then uses these bounded stages:

| Stage | Passes and purpose |
| --- | --- |
| `normalize` | Peephole, constant data, compact pushes, CFG simplify, layout, and revert sharing establish local canonical form. |
| `share-structure` round 1 | Terminal deduplication, CFG cleanup, two tail-merge opportunities, and outlining reshape the explicit CFG. |
| `regenerate` | Compact pushes, block CSE, DCE, peephole, layout, and revert sharing clean stack code exposed by structural work. |
| `share-structure` round 2 | Tail merge and outline recheck only work created by regeneration. |
| `finalize` | Compact pushes, peephole, block CSE, DCE, layout, and revert sharing make the address-sensitive final form. |

No canonical pipeline invokes a transform more than three times. Repeated MIR
scalar passes see source, generated semantic, or target-generated code at
different boundaries. Repeated EVM passes see local input, regenerated code, or
the final address-sensitive layout. A second terminal-dedup round produced
identical corpus output and was removed.

## Observability

`-Zprint-after-stage` prints stable checkpoints from `mir.fresh` to
`mir.scheduled` and from `evm.scheduled-input` to `evm.final`.
`-Zprint-after-each` and `-Zpass-diff` identify an exact invocation. Timing and
dump headers include the IR, module, artifact, pipeline run, stage, round, pass,
invocation, outcome, IR change, and phase-only state change. Failed lowering
contracts report `outcome=failed` and suppress later checkpoints. Output names
include module and artifact identity, so concurrent deployment and runtime
pipelines do not overwrite one another.

## Candidate decisions

The saved baseline predates all pipeline changes. Each accepted output change
used the same successful project IDs, contracts, optimization settings, EVM
version, and hot-gas labels. Compile failures and runtime mismatches reject a
candidate.

| Decision | Evidence |
| --- | --- |
| Keep semantic hash optimization before mapping expansion and low-level optimization after it | Preserves semantic CSE while exposing the generated memory hash code to GVN and cleanup. |
| Coordinate intrinsic and target lowerings | Gives each boundary one phase owner and removes nested pass adapters without hiding internal transform results. |
| Keep SCCP after target lowering | Removed 156 creation and runtime bytes from SignatureChecker without changing its measured hot calls. |
| Remove the second EVM terminal-dedup run | The runtime compile corpus produced identical serialized output. |
| Reject broad low-level cleanup after target lowering | The aggregate fell, but Nitro grew by 293 bytes and Governor by 11 bytes. |
| Reject early ABI materialization | LilFractional grew by 1,184 bytes. |
| Reject allocation planning after codec lowering | The exact 15-project hot run passed correctness and saved 749 bytes, but Nitro regressed by 162 gas and Governor by 384; total call gas rose by 129. |

The benchmark data below are the compact, tracked record. Full raw JSON, logs,
per-project rows, and per-label rows remain under `target/codegen-bench/` in the
working checkout.

## Final measurements

The frozen baseline compiler SHA-256 is
`8a035c1dd2d2fe1bea92fb09e756865afa1d880affd353672ed9d7846824a090`.
The final compiler SHA-256 is
`ec6591559afec1ced2287417d8d199aee006031489d18eb1c0ef55393085425a`.

The pinned runtime corpus matched all 15 ordered project IDs and all 175
ordered gas labels, calls, and arguments. Every compile, deployment, call, and
runtime check passed with no mismatch.

| Runtime corpus metric | Baseline | Final | Delta |
| --- | ---: | ---: | ---: |
| Hot call gas | 5,187,072 | 5,186,649 | -423 (-0.0082%) |
| Deployment gas | 32,851,068 | 32,709,053 | -142,015 (-0.4323%) |
| Creation bytes | 150,986 | 150,332 | -654 (-0.4332%) |
| Runtime bytes | 146,126 | 145,472 | -654 (-0.4476%) |

Twelve gas labels improved and 163 stayed equal. No label or project regressed
in call gas, deployment gas, creation size, or runtime size. Single compile
samples fell from 6.274768 to 5.728022 seconds in total. The largest peak RSS
sample rose from 48,848,896 to 50,847,744 bytes; every project RSS sample rose,
so memory use remains the measured cost of this design.

The UI comparison used the same current corpus hash, 336 ordered successful
fixtures per mode, and 16 diagnostic exclusions on both compilers.

| UI mode | Metric | Baseline | Final | Delta |
| --- | --- | ---: | ---: | ---: |
| `-Ogas` | Creation bytes | 918,014 | 917,023 | -991 (-0.1080%) |
| `-Ogas` | Runtime bytes | 552,348 | 551,434 | -914 (-0.1655%) |
| `-Osize` | Creation bytes | 917,374 | 916,519 | -855 (-0.0932%) |
| `-Osize` | Runtime bytes | 550,324 | 549,503 | -821 (-0.1492%) |

The UI corpus contains local tradeoffs. `-Ogas` improved 41 runtime cases and
regressed 23; `-Osize` improved 35 and regressed 25. The largest regression was
88 runtime bytes in `stack-too-deep/call_args.sol`; the largest improvements
were 124 bytes for `memory_fixed_array_alloc.sol` under `-Ogas` and 117 bytes
for `fmp_reload_computed_operand.sol` under `-Osize`. The aggregate bytecode and
the higher-priority runtime gas corpus both improved, with no runtime-corpus
output regression, so the final ordering keeps this tradeoff.

The complete two-mode UI harness took 472.38 seconds for the final compiler and
476.25 seconds for the baseline. Maximum harness RSS rose by 1,372 KiB. These
are harness-level observations, not isolated compiler measurements.

[`PIPELINE_REDESIGN_RESULTS.json`](PIPELINE_REDESIGN_RESULTS.json) records the
exact raw artifact hashes used for these totals.
