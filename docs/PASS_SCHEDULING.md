# Pass scheduling experiments

This investigation changes the order and repetition of existing MIR and EVM IR
passes. It does not add transforms. The baseline is Solar
`797d916e7a9d8856c68ae1f387bd52db15aded19`.

## Reference compilers

These observations describe the source revisions linked below. A pass's internal
worklist or fixed point is distinct from repeating a group of passes. None of the
schedules below implies that running every optimization to a global fixed point
will improve generated code.

| Compiler | Schedule | Repeated work |
| --- | --- | --- |
| LLVM | Staged module, call-graph, function, and loop pipelines. | SROA, InstCombine, SimplifyCFG, jump threading, and correlated-value propagation appear at several stages. LICM runs before and after loop rotation and again after memory cleanup. Scalar cleanup follows loop transforms. |
| solx LLVM | LLVM's chosen optimization pipeline plus EVM extension points and a separate machine pipeline. | EVM adds late DSE at speed levels above one, EarlyCSE/GVN/CFG simplification/LICM before instruction selection, and branch folding before and after stackification. Final constant unfolding precedes peepholes. |
| Cranelift | CFG and dominators, unreachable-code removal, constant-phi removal, alias resolution, then one e-graph stage when optimization is enabled. | The main optimization driver has no outer fixed-point loop. Rewriting and value numbering live inside the e-graph stage; recursive rewrites have a depth limit of five, plus match, e-class-size, and extractor-fuel caps. |
| rustc MIR | An explicit ordered pass list, with optimization-level gates and cleanup around inlining, GVN, and constant propagation. | InstSimplify runs before inlining and after CFG cleanup. Constant-condition, CFG, and local cleanup recur. DSE runs before GVN and near the end. |
| Vyper Venom | Global preparation/inlining followed by ordered per-function O2, O3, or size schedules. | SCCP, phi/assignment elimination, CFG simplification, and algebraic cleanup recur after SSA construction, memory promotion, and free-memory-pointer lowering. Invoke-copy forwarding runs before inlining and again per function. O3 adds tail merging followed by CFG cleanup. |
| Sonatina / Fe | Module inlining alternates with primary, secondary, and short cleanup groups. EVM lowering has additional cleanup stages. | Four inlining sites in the standard schedule; GVN and SCCP recur. Dead-argument elimination gets its own SCCP/CFG cleanup. Raw-memory lowering runs GVN/LICM, then load/store, range, SCCP, and strength-reduction cleanup. Fe selects Sonatina's optimization level. |
| solc Yul | An explicit abbreviation sequence with bracketed repeat groups and a separate final cleanup sequence. | Bracket groups repeat until the AST code-size metric stays equal, capped at 12 rounds. This is not structural equality. SSA construction/reversal, inlining, expression simplification, and unused-code pruning recur across stages. |
| solc SSACFG | Per-function unreachable-block cleanup, trivial-phi elimination, then identity/no-op removal. | No outer repetition in the inspected driver. The outliner call is disabled. |
| solc core Solidity | Source-level special cases run during code generation: literal operand ordering and unchecked increments for proved simple counter loops. | No general AST optimization fixed-point pipeline in the inspected path. |
| solc legacy Solidity backend | Recursively optimize subassemblies, then iterate inlining, jump-destination removal, peepholes, block deduplication, and CSE while counted changes remain. Constant optimization follows the loop. | Peepholes have an inner loop with a 64,000-change failure guard. Deduplication deliberately triggers another outer round so rewritten label references can expose removable code. |

Sources:

- [LLVM pipeline](https://github.com/llvm/llvm-project/blob/c0a7197865180e8aee11f96fd84e3c1037b69d20/llvm/lib/Passes/PassBuilderPipelines.cpp).
- [solx EVM target pipeline](https://github.com/NomicFoundation/solx-llvm/blob/4cb0e189b86c037fc7b6fb2bd32f081e47953f92/llvm/lib/Target/EVM/EVMTargetMachine.cpp).
- [Cranelift optimization driver](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/context.rs) and [e-graph bounds](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/egraph/mod.rs).
- [rustc MIR pipeline](https://github.com/rust-lang/rust/blob/f45772eb69d6ed3cc23be40625411a75f9f32c9d/compiler/rustc_mir_transform/src/lib.rs).
- [Venom O2](https://github.com/vyperlang/vyper/blob/8f6b7fb0bf1a495fc67c7fc77dc4589d58a50394/vyper/venom/optimization_levels/O2.py), [O3](https://github.com/vyperlang/vyper/blob/8f6b7fb0bf1a495fc67c7fc77dc4589d58a50394/vyper/venom/optimization_levels/O3.py), and [driver](https://github.com/vyperlang/vyper/blob/8f6b7fb0bf1a495fc67c7fc77dc4589d58a50394/vyper/venom/__init__.py).
- [Sonatina optimization pipeline](https://github.com/fe-lang/sonatina/blob/54492147373b8bf45f381bcef39477f0fb194648/crates/codegen/src/optim/pipeline.rs), [EVM lowering pipeline](https://github.com/fe-lang/sonatina/blob/54492147373b8bf45f381bcef39477f0fb194648/crates/codegen/src/isa/evm/pipeline.rs), and [Fe integration](https://github.com/argotorg/fe/blob/8deb62ee79a749f3c812a3440ddb458e060ccb5f/crates/codegen/src/sonatina/mod.rs).
- [solc Yul schedule](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libsolidity/interface/OptimiserSettings.h), [repeat driver](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libyul/optimiser/Suite.cpp), [SSACFG pipeline](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libyul/backends/evm/ssa/transform/OptimizationPipeline.cpp), and [legacy assembly optimizer](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libevmasm/Assembly.cpp).

The core-Solidity observations come from [ContractCompiler's loop visitor](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libsolidity/codegen/ContractCompiler.cpp) and [ExpressionCompiler](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libsolidity/codegen/ExpressionCompiler.cpp). These local lowering choices precede the Yul or legacy assembly schedules.

## Constraints in our pipeline

Semantic optimization, representation lowering, lowered-word optimization, and
physical-stack optimization have different contracts. Repeating the complete MIR
pipeline would cross representation boundaries and rerun transforms after the
semantic information they need has gone. Experiments therefore repeat semantic
optimization separately or add cleanup at a chosen lowering boundary.

Several passes already iterate internally. MIR CSE runs until its elimination
count stops growing; SCCP drains CFG and SSA worklists; PRE repeats candidate
batches under a rewrite budget and remembers expressions it has eliminated.
EVM CFG cleanup repeats until its rewrite group stops changing code, tail merging
replans after productive batches, and peepholes revisit newly exposed tails.
The e-graph explores bounded alternatives and uses a bounded optimistic phi
analysis. Repeating a pass is therefore most useful after another pass exposes
new work. An outer repetition also resets PRE's per-invocation safeguards.

Instruction-position observations also constrain sharing. Terminal deduplication
and tail merging skip modules containing `PC`, so otherwise identical bodies keep
distinct observation sites. The position fixture branches between two sites and
returns their values; both survive the default pipeline. Absolute offsets remain
subject to code layout.

The baseline EVM IR pipeline contains two structural sweeps. Early and final
peephole/CFG modes differ, and constant packing and expression reordering run late
to preserve sharing opportunities. An outer loop can put finalized code back
through earlier choices. Local profitability does not prove that this composition
improves final gas or size.

A pass returning `changed` is not a decreasing cost metric. Opposing layout or
canonicalization choices can keep changing IR. Experimental repeat loops use an
iteration cap and stop early when a complete group reports no change. In the
size-mode factorial experiment, block layout and terminal layout both report
changes on every one of four outer rounds, even though the bytecode after two
and four rounds matches. A plain `while changed` loop would not detect this
stable boundary state. Across the 23 successfully compiled cases, 17 gas-mode
cases and three size-mode cases still report layout changes at round 16.
The round-8 and round-16 output fingerprints match in both modes.

## Measurements

Initial screens use release binaries, with the baseline copied before edits.
Final validation and isolated compile-time measurements use matching debug builds. The existing runtime runner selects 24 cases;
23 compile. Uniswap V2 fails on legacy `chainid` syntax in both baseline and
candidate and does not count as covered. Gas uses the hot workload. Size uses
`--optimizer-runs 1`. Reports use equal-weight geometric means across comparable
cases, and exclude the LibString memory-brutalizer calls whose workload depends
on gas and bytecode. Raw excluded measurements remain in the saved results.

Initial screening runs use only our compiler. A separate baseline run against solc
0.8.36 passes runtime comparisons for all 23 selected cases. Retained candidates
are checked against matching saved reference observations. Compile-time
samples collected while other jobs run are screening data, not performance claims.

| Experiment | Result | Decision |
| --- | --- | --- |
| Up to three semantic rounds | Gas-objective runtime bytes -0.05% geometric mean, runtime gas +0.01%. Solady Algorithms grows 7 bytes and adds 7,531 gas across its calls; individual insertion-sort calls regress up to 2.16%. Size-objective bytes improve 0.02%. | Reverted. |
| Repeat PRE and CSE after semantic cleanup | Algorithms grows 7 bytes in gas mode; reversing the two passes gives the same result. | Does not resolve the semantic-repeat regression. |
| Move DCE before the first e-graph | SignatureChecker shrinks 2 bytes in each mode, but larger project inputs grow, including Seaport, V4, and Solady. | Rejected after broader screening. |
| Repeat the complete EVM IR pipeline | Eight and sixteen rounds produce the same output. Layout changes still reach the cap on many inputs. Extra rounds also grow gas-mode LibString by 35 bytes. | Reject an unbounded outer loop. |
| Repeat ordinary layout after loop layout | Gas-mode byte counts stay equal, but runtime gas grows 0.85%. | Reject this order for gas mode. |
| Final ordinary layout before loop layout | The small corpus saves 28 SignatureChecker bytes and 24 LibString bytes in size mode, with unchanged runtime gas. Larger gas-mode inputs include both gains and growth. | Test size-only scheduling separately. |

| Skip early LICM only in size mode | The combined schedule saves 30,129 project bytes instead of 24,455, but grows two contracts instead of one. The isolated omission adds 45 gas to the comparable LibString workload. | Keep LICM. |
| Remove the e-graph after aggregate lowering | Some small word workloads shrink sharply, but project outputs grow by 2,004 bytes in gas mode and 2,378 in size mode. Affected runtime cases also regress in aggregate. | Keep this stage. |
| Add final tail merging or CFG cleanup | Tail merging grows nine project outputs; CFG cleanup alone saves only another 25 bytes in one output. | Keep the narrower schedule. |

## What the experiments distinguish

Equal bytecode size does not imply equal runtime cost. Rerunning ordinary block
layout after loop layout leaves every gas-mode runtime size unchanged in the
23-case corpus, but increases runtime gas by 0.85% geometric mean. It undoes hot
loop fallthrough choices. Reapplying loop layout restores the output for all but
one of those cases. Size mode skips loop layout; a final ordinary layout instead
improves its runtime gas by 0.10% with no measured per-case gas regression.

Late SCCP has a concrete lowering opportunity: a one-byte pre-Cancun `mcopy`
expansion contains a backward word loop whose initial offset is zero. The loop
body is unreachable, but ordinary CFG cleanup cannot infer the loop-carried
constant. SCCP followed by CFG cleanup removes it. One cleanup round suffices in the
small corpus; two, four, and eight rounds give identical outputs. Removing the
CFG cleanup looks equal there but loses savings in Seaport and Solady project
outputs. The existing pre-Cancun MIR
fixture now exercises the default pipeline and rejects a remaining loop phi or
backward subtraction for that copy.

Final EVM block CSE can reuse calldata loads after late CFG transformations join
their instructions into one block. A final peephole run also folds comparisons
exposed by that forwarding. Extra final DCE contributes no measured reduction in
the first screen.

## Candidate results

The measured combination adds SCCP/CFG cleanup after memory-copy lowering, one
terminal-deduplication/CFG/tail-merging/CFG sweep before constant packing, final
block CSE and peepholes, and size-only block/terminal layout after the last local
rewrites. All transforms already exist. The size-only adapter shares the underlying
pass's unchanged-result cache entry; adapters that add rewrites stay distinct.

| Runtime corpus metric | Gas objective | Size objective |
| --- | ---: | ---: |
| Runtime bytes, geometric mean | -0.037% | -0.076% |
| Runtime gas, geometric mean | unchanged | -0.096% |
| Deployment gas, geometric mean | -0.028% | -0.065% |

All 23 runtime comparisons against saved solc 0.8.36 observations pass. No
comparable case grows or uses more runtime gas. The size-mode factorial workload
falls from 191,225 to 189,652 gas, and arithmetic falls from 125,437 to 123,710.
These are sums over the runner's hot calls, not individual transaction costs.

The larger size check compiles every contract in the nine vendored project
archives, including their tests and helpers. Gas mode compares 1,171 nonempty
runtime objects: 136 shrink, none grow, and the total falls by 6,605 bytes. Size
mode compares 938 nonempty objects: 279 shrink, one grows, and the total falls by
24,455 bytes. The exception is SeaportValidatorHelper, which grows 37 bytes.
The OpenZeppelin archive fails in size mode in both baseline and candidate with
the same low-memory forwarding-buffer error and is excluded from that mode.

Forced Paris, Cancun, Osaka, and Amsterdam screens show no runtime-size growth
among comparable small-corpus cases. Paris covers 22 cases: besides the existing
Uniswap failure, Governor requires Cancun's Yul `mcopy`. Other forks cover 23.

The generated differential uses 32 SolSmith programs with seed 921 and two
properties per program, each with 1,024 Foundry fuzz trials. Baseline and candidate
both pass comparison with solc, including return data, logs, and normalized state.
Forty debug-output pairs across Paris/Osaka and gas/size modes produce identical
creation and runtime bytecode with and without ethdebug.

## Compiler-time tradeoff

Matching debug builds take 1.17% longer by geometric mean across the nine
whole-project inputs. Each result is the median of three single-compile samples,
collected in alternating baseline/candidate order after this investigation's
other builds and benchmarks finished. Both revisions were rebuilt with
`cargo build -p solar-compiler --bin solar` in the same isolated worktree and
shared target directory. The host is shared; these samples do not establish a
release-build cost or a statistical confidence interval.

| Project | Baseline median | Candidate median | Change |
| --- | ---: | ---: | ---: |
| seaport-1.6-project | 82.561 s | 80.705 s | -2.25% |
| v4-core-project | 10.456 s | 10.756 s | +2.86% |
| morpho-blue-project | 8.860 s | 9.051 s | +2.15% |
| openzeppelin-5.6.1-project | 10.437 s | 10.571 s | +1.28% |
| solady-0.1.26-project | 18.654 s | 18.922 s | +1.44% |
| forge-std-1.16.1-project | 5.846 s | 5.892 s | +0.79% |
| prb-math-4.1.1-project | 3.453 s | 3.530 s | +2.23% |
| solmate-6-project | 7.815 s | 8.043 s | +2.93% |
| solarray-a547630-project | 0.490 s | 0.486 s | -0.77% |

The schedule trades compiler work for smaller generated code and the measured
size-mode gas reduction. Raw samples and eligibility checks are in
`matched-compile-time/`; no compiler-speed improvement is claimed.

## Reproducing the retained schedule

Keep a baseline executable built from the revision above, then build the candidate
with the same debug profile. The full corpus command is:

```bash
cargo build -p solar-compiler --bin solar
mkdir -p target/codegen-bench/candidate
uv run benches/runtime/benchmark.py \
  --solar target/debug/solar \
  --mode runtime compile-time --suite all --compile-repeats 1 \
  --gas --gas-profile hot --start-anvil --allow-failures \
  --artifacts target/codegen-bench/candidate/artifacts \
  --output target/codegen-bench/candidate/results.json
```

Use a separate output directory and `--optimizer-runs 1` for size mode.
`--allow-failures` retains the known failure in the report; it does not count it
as covered. Compare saved runs with `benches/runtime/benchmark-compare.py`,
including `--diff-output` to inspect MIR, EVM IR, disassembly, and bytecode.
The final debug run covers all 24 runtime cases and nine whole-project inputs;
32 of these 33 inputs compile. All nine whole-project inputs compile with their
original settings. The forced-size project check above is a separate experiment.

Local evidence is retained under `target/codegen-bench/pass-scheduling/`:
`final-debug-full/` and `final-size-artifacts/` hold the final reports and artifact
diffs; `combined-size-final-terminal-tail-project-sizes/` holds the per-contract
project comparison; `convergence/` records bounded outer-loop behavior;
`final-debug-neutrality/` and `fuzz-combined-corpus/` hold correctness checks.
`pc-guard-full/` and `pc-guard-equivalence.json` confirm that the position guard
preserves the measured corpus outputs. `pc-runtime.json` checks the two distinct
position observations in the EVM IR fixture. The screen summaries retain rejected
schedules and their measurements.
