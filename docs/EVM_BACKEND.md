# MIR to EVM IR: architecture review

This note reviews the boundary between lowered MIR and EVM IR: stack
scheduling, spilling, frames, calling conventions, switch lowering, and the
EVM IR passes that clean up after them. It compares our design with solc
(legacy, optimized, and the experimental SSA-CFG generator), solx's LLVM EVM
backend, Sonatina (Fe), Venom (Vyper), and Plank, and proposes a staged plan.

The review was made on top of
[paradigmxyz/solar#1609](https://github.com/paradigmxyz/solar/pull/1609)
(`5062a1f48`). Every number below was measured on that revision; the
proposals carry no predicted numbers.

## Summary

**Is stack scheduling in the right place?** Yes. Scheduling belongs at the
MIR-to-EVM boundary: MIR still has SSA values and phis, and EVM IR only has
physical stack operations. Every compiler we surveyed puts it at the same
point. The problem is not placement but shape: the boundary is one fused
pass that selects instructions, chooses block layouts, spills, assigns frames,
chooses calling conventions, and lowers switches while it emits. There is no
intermediate plan that can be checked, cached, or recomputed per function.
Solc's SSA-CFG generator and Sonatina both compute a per-function plan as
plain data and then replay it; we should do the same.

**Why is it slow?** No single hotspot dominates. MIR-to-EVM lowering takes a
quarter to a third of a whole-project compile, and the time goes to four
architectural habits:

1. Local decisions are made by speculation: plan a binary operation's
   operands in both orders, replay the next two instructions for each start
   on a cloned scheduler, and keep the cheaper one. We call the operand
   planner about 1.5 times per emitted instruction, and binary operations
   alone take 10% of the Seaport compile.
2. Decisions made during emission can invalidate earlier ones, so failure is
   handled by re-emitting the whole runtime with a function's stack-only
   convention or a global policy disabled.
3. Each stack policy (loop phis, live joins, selector globals, resident
   arguments, call-site preservation) brings its own fixpoint and its own
   validation.
4. The EVM IR passes re-derive stack state that the scheduler knew but did
   not record, and several of them exist to clean up scheduler output.

**How good is the output?** Already better than solc and solx on stack traffic
in most of the runtime corpus. The weak point is values that live across
blocks: outside a few narrow policies, they go to memory. Large
assembly-heavy contracts such as Solady's LibString pay for that with hundreds
of constant-address loads and stores, many of them spill traffic, that solc
avoids.

**Is it too coupled?** Yes. `EvmCodegen` has 62 fields that mix module,
runtime-attempt, function, and block state. The stack model and emitted code
are kept in step by hand at hundreds of call sites, and nothing checks that
they agree. `codegen::stack` and
`ir::passes` depend on each other. Four separate cost types price stack
code.

**Are the files too big?** Some are, but size is a symptom. `stack/scheduler.rs`
has 4,811 lines, `switch.rs` 3,089, and `stack/layout/phi.rs` 1,990, and
`generate_function_body` alone runs to 936 lines. Splitting files without
splitting responsibilities would not help; the staged plan below splits both.

## Measurements

All timings come from a `profiling` build of the compiler, compiling the
pinned project archives from `testdata/projects/` through `--standard-json` with
`-j1`, sampled with `perf` at 2–4 kHz with DWARF call graphs.

### Where compile time goes

Inclusive share of samples:

| Phase | Seaport 1.6 (12.9 s) | Solady 0.1.26 (3.1 s) |
| --- | ---: | ---: |
| MIR pass pipeline | 39.1% | 39.3% |
| MIR to EVM IR (`emit_runtime`) | 31.7% | 24.8% |
| of which `generate_function_body` | 25.5% | 22.2% |
| of which `emit_binary_op_with_result` | 10.4% | 8.3% |
| of which `StackPhiPlan` construction | 3.4% | 2.6% |
| Assembly, including most EVM IR pass runs | 10.9% | 13.3% |
| Outlining checkpoint | 3.4% | 3.9% |
| EVM IR pass pipeline, all runs | 11.3% | 13.3% |

The front end is small: 88% of Seaport's samples fall under
`generate_contract_bytecodes`. The Seaport archive has 432 contracts, many
of them tests that compile the same library code again; no single contract
takes more than half a second of pass time.

Inside `emit_binary_op_with_result`, 7.6 points go to `plan_operands` and 2.0
to `prefer_binary_plan`. The whole backend is flat beyond that: the largest
self-time entries are `generate_function_body` itself (2.3%), the A* lower
bound (1.9%), MIR operand visiting (1.6%), and the operand planner body
(1.5%).

### How often the scheduler plans

Counts from temporary counters (not committed) over whole projects:

| Project | Emitted instructions | `plan_operands` calls | A* searches | Runtime emissions / modules | Function bodies |
| --- | ---: | ---: | ---: | ---: | ---: |
| Seaport 1.6 | 1,559,084 | 2,421,905 | 176,732 | 380 / 348 | 14,506 |
| Solady 0.1.26 | 279,346 | 391,446 | 41,182 | 225 / 221 | 6,105 |
| OpenZeppelin 5.6.1 | 119,344 | 168,005 | 12,951 | 238 / 233 | 4,990 |
| v4-core 4.0.0 | 157,997 | 237,204 | 16,037 | 134 / 130 | 3,512 |

We plan operands 1.4 to 1.6 times per emitted instruction, and 8% to 15% of
emitted instructions reach A*. Whole-runtime re-emission is uncommon at
module level (up to 9% of modules), but each retry re-emits every function in
the module.

### Output quality

The main-branch codegen benchmark
([run 36525376310](https://github.com/paradigmxyz/solar/actions/runs/36525376310))
compiles the runtime corpus with our compiler, solc, and solx. Counting
runtime opcodes, with CBOR metadata stripped, over the 21 cases that all three
compilers build:

| Compiler | Instructions | DUP | SWAP | POP | Stack ops | Stack share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ours | 26,911 | 3,707 | 2,258 | 973 | 6,938 | 25.8% |
| solx | 31,502 | 3,895 | 2,514 | 1,533 | 7,942 | 25.2% |
| solc | 32,311 | 4,969 | 3,395 | 1,098 | 9,462 | 29.3% |

We emit the fewest stack operations in absolute terms. The two cases where our
code is larger than solc's are both large Solady contracts that solx does not
compile:

`solady-lib-string` (60,701 bytes against solc's 50,738) and
`solady-signature-checker` (20,837 against 18,539). From the disassembly of
LibString:

| Opcode | ours | solc |
| --- | ---: | ---: |
| all instructions | 39,219 | 30,355 |
| SWAP1–SWAP3 | 4,049 | 1,437 |
| MLOAD | 1,819 | 1,085 |
| JUMPI | 1,078 | 647 |
| SHL | 900 | 390 |

In LibString, 701 of our `PUSH c; MLOAD` pairs and 311 `PUSH c; MSTORE` pairs
use constant addresses from `0x80` upward, where the static spill frame and
static allocations live. solc has almost none: nearly all of its
constant-address traffic touches `0x40`. That count mixes spill traffic with
static allocations, so it is an upper bound on spills, but it points at the
same weakness the module docs describe: values outside a selected stack layout
"keep their stable spill homes". Across the whole corpus, such pairs make up
5.8% of our runtime instructions (counting two instructions per pair), and up
to 15.6% in small cases such as `seeded-words`.

Gas is a different story: solc spends 13% more runtime gas than we do on
LibString, so the extra memory traffic mostly costs bytes. Gas-mode size
still matters for contracts near the EIP-170 limit.

## Current design

The boundary runs in this order.

1. **Lowered MIR.** The pipeline ends with DCE and `evm-inst-schedule`, a
   dependency-first traversal inside barrier segments adapted from Venom's
   DFT pass. It is the only pass that orders instructions within a block for
   the stack scheduler, and it does not know stack costs; it keeps the
   producer order of binary operations whose lowering prices both
   orientations.
2. **Per module.** Phi critical-edge splitting, argument canonicalization
   (and immediate canonicalization in size mode), call-graph analysis, then the runtime retry loop
   (`runtime.rs:36`). Each attempt classifies frames, plans calling
   conventions for the whole module (stack argument masks, a subset search
   over up to 255 resident-argument candidates, stack returns), emits every
   function body, and then packs and resolves static frames. If a function's
   stack-only convention fails, that function is disabled; if caller stack
   prefixes overflow, one of three global policies is switched off. Either way
   the whole runtime is emitted again.
3. **Per function** (`generate_function_body`, 936 lines). Liveness, spill
   hazards, phi parallel copies, `StackPhiPlan` (loop phis, live joins with a
   fixpoint of up to 64 rounds, branch phis), `GlobalStackPlan` for selector
   functions, merging of resident layouts, spill-slot preallocation and
   colouring, cold blocks, block order, emission, then removal of dead spill
   stores by dataflow over the emitted EVM IR.
4. **Per block.** Pick an entry stack from one of five sources, intersect
   spill availability over predecessors, emit instructions, and pick one of
   about eight exit mechanisms, falling back to spilling every live-out value.
5. **Per instruction.** Lazy argument materialization, live-out operand
   spills, ISLE alternatives each planned on a cloned scheduler, operand
   planning (exact prefix, five linear shapes, gas-only one-action and unary
   plans, a lower-bound-certified greedy walk, bounded A*), a per-arity fallback
   emitter when planning fails, result spill, and dead-value cleanup.
6. **EVM IR.** 53 pass invocations, then assembly and, in gas mode, a
   code-size rescue that resumes the pipeline from an outlining checkpoint.

## Survey

| | solc optimized | solc SSA-CFG | solx (LLVM) | Sonatina | Venom | Plank | ours |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Input | Yul variables | SSA, phi and upsilon | vregs after phi elimination and coalescing | post-isel machine SSA | single-use SSA, DFT-ordered | SSA with block arguments | lowered MIR SSA |
| Layout direction | backward, loop fixpoint | forward, topological, one pass | backward, loop fixpoint | forward RPO, frozen merge templates | forward DFS | static per edge group | per-instruction local, plus several edge policies |
| Join choice | Heap-style permutation walk | best predecessor proposal, O(P²) shuffles | permutation walk priced in gas | lexicographic tiebreak | liveness order | sorted union | policies: loop phis ≤ 8 words, live joins ≤ 12, selector globals |
| Operand shuffling | greedy, capped at 1000 steps | greedy with liveness-driven DUP choice | greedy, both orders for commutative ops | tiered, bounded A* with caches | greedy, both orders | greedy with bounded permutation DFS | tiered, bounded A*, both orders plus two-instruction lookahead |
| Reordering | none | none | single-use expression sinking | none (separate code sinking) | stack-aware DFT pass | DAG built, program order used | MIR `evm-inst-schedule`, DFT-derived, stack-blind |
| Spill decision | Yul rewrite before codegen | found by the shuffler, monotone fixpoint | after solving, up to 100 whole-function re-solves | fixpoint of whole-function replans, up to 64 section rounds | reactive at emission | reactive at emission | preallocated for every cross-block value, reactive within blocks |
| Spill memory | shared across disjoint call paths | one word per value | per-function region, slot colouring | scratch colouring and arena objects | free list | one word per spill | stable slots with gas-mode colouring |
| Core size | ~3.2k lines | ~4.5k | ~2.9k | ~10k | ~2.6k | ~2.6k | ~17k non-test lines in `codegen/` |

The designs that matter for us:

- **solc SSA-CFG** (`libyul/backends/evm/ssa/`) is the closest match to our
  input. It computes liveness with use counts once, lays blocks out in one
  forward topological pass, chooses each join's entry layout from its
  predecessors' exit proposals by real cost, treats phis as renaming on the
  edge shuffle, and lets the shuffler name a spill candidate when a value
  falls out of reach. It repeats layout until the spill set stops growing,
  which terminates because the set only grows. Layout, spill set, and memory
  addressing are plain data; `CodeTransform` only replays them. It needs no
  loop fixpoint because backedges are checked against a header entry that is
  already fixed.
- **solx** shows the cost of the backward design: unbounded loop
  re-propagation, and a spill loop that re-solves the whole function up to
  100 times. It also rematerializes literals only below a size threshold,
  and prices joins in gas.
- **Sonatina** has the most thorough local search (packed `u128` states,
  bounded A* seeded by greedy and beam upper bounds, 4096-entry plan caches
  keyed by the relevant stack window) and a clean plan/replay split. Its cost
  is size: about 10k lines, and whole-section replanning coupled to memory
  placement.
- **Venom** orders instructions with a DFT pass that sorts dependencies by
  the exit stack order it expects, and flips commutative operations there.
  Emission is then greedy. Our `evm-inst-schedule` adopted the traversal but
  not the stack-order input, which MIR should not carry. Stack-aware ordering
  in the plan layer is the cheap alternative to speculative lookahead.
- **Plank** keeps each block a pure function of its dependency graph, entry
  layout, and exit layout. That is the most modular shape, although its
  current layouts and ordering are placeholders.

## Assessment

### Placement

Keep scheduling at the lowering boundary. What should move is the set of
decisions made during emission:

- **Calling conventions** (stack argument masks, resident arguments, stack
  returns, caller-stack preservation) are decided per module, but their
  failure is found during emission, which forces whole-runtime retries. They
  should be proved before emission from the callee's plan, with retries
  scoped to one function and its callers.
- **Cross-block layouts** are the output of several narrow policies layered
  over a default of spilling. A general per-function layout pass would
  replace them.
- **Instruction order** is chosen in MIR without stack costs, then partly
  repaired by per-instruction lookahead. A stack-aware ordering step inside
  the plan (Venom's stack-order input, or Plank's dependency graph with a
  real choice function) could make the lookahead unnecessary without putting
  stack layouts into MIR.
- **Switch lowering** reads assembler state and predicts block-layout label
  widths. It belongs after layout, or it should query a narrow interface
  rather than the assembler itself.

### Algorithmic cost

These are the concrete sources of repeated work, in rough order of measured
or likely weight:

- **Speculative operand planning.** A commutative binary operation plans both
  orders and, through `prefer_binary_plan`, clones the scheduler and plans the
  next two instructions in both orders for each candidate
  (`planning.rs:183-345`). An instruction with ISLE alternatives repeats this
  per alternative. The function-wide A* budget lives in a `Cell` inside the
  cloned `StackScheduler` (`scheduler.rs:261`), and by design
  (`planning.rs:26`) speculation does not consume it, so nothing bounds the
  total speculative search in a function.
- **ISLE availability extractors.** `inst_data` checks
  `instructions[..index].contains(inst)` for up to 16 stack words per query
  (`planning/isle.rs:69-84`), which makes those rules quadratic in block
  length. `zero_value` scans every live value on each call.
- **Whole-runtime retries.** `runtime.rs:36-69` re-emits every function after
  any stack-only failure or caller-stack overflow. Liveness and phi plans are
  cached across attempts, except for the block-local `entry` function, whose
  liveness is computed twice per attempt (`runtime.rs:385`,
  `function.rs:121`); convention planning, global plans, spill colouring, and
  emission are not.
- **Stack model representation.** `StackModel` keeps the top at index 0, so
  every push and pop moves the whole vector; `find` and `contains` are linear
  scans. Search states clone and hash the full stack.
- **Repeated analyses.** `LoopAnalyzer` runs up to three times per body,
  from four call sites; `StackPhiPlan` is deep-cloned per body; spill
  availability clones a hash set per block; the resident-argument subset
  search hoists one `CfgInfo`, but `GlobalStackPlan::analyze_resident_args`
  (`global.rs:220`) still builds a new one for each of up to 255 subsets; call-site preserve-or-drain planning deep-copies the whole
  `SpillManager` through `Rc::make_mut`.
- **Late re-derivation.** `block_cse`, `dce`, `stack_normalize`,
  `reorder_pushes`, the peephole extractors, outlining, and the verifier each
  rebuild block stack state from nothing, because EVM IR records no entry
  layout.

### Output quality

- **Cross-block values default to memory.** The scheduler stores every value
  that crosses a block at its definition unless a layout policy claims it,
  and removes dead stores afterwards. solc SSA-CFG, solx and Sonatina keep
  cross-block values on the stack by default and spill only what cannot be
  reached. This is the most likely cause of the LibString gap.
- **Dead-value cleanup looks at one point.** `drop_dead_values`
  (`scheduler.rs:2544`) asks `Liveness::is_dead_after`, which is true only at
  a value's last use or for a value unused in the block
  (`liveness.rs:355-371`). A copy that survives its last use, for instance
  because it was too deep to pop then, is never dead again in the block, so
  it stays until the block exits, and `dce` and `stack_normalize` have to
  remove it later. The right query already exists:
  `Liveness::is_used_at_or_after` (`liveness.rs:332`), which
  `preserved_operands_for` uses.
- **Cleanup passes repair scheduler output.** `reorder_pushes` removes
  `producer; PUSH; SWAP1`, `dce` removes DUPs whose copy only reaches a POP,
  and `stack_normalize` resynthesizes runs built by concatenating the plan,
  dead-value cleanup, and edge shuffles. Each is cheap in isolation, but each
  hides a scheduler decision that could have been right the first time.
- **Join layouts are chosen by policy, not by cost.** Loop phis are limited to
  eight words, live joins to twelve, and selector globals to functions with
  two or three decoded arguments. Choosing a join layout from predecessor
  proposals by cost, as solc SSA-CFG does, needs no such limits.

### Coupling and file size

- `EvmCodegen` (`mod.rs:248`) has 62 fields, with two hand-written reset
  functions and per-function fields cleared by hand in `function.rs`.
- `stack/spills.rs`, `stack/edges.rs` and `stack/layout/select.rs` are
  `impl EvmCodegen` blocks, so the stack subtree is private in name only.
- The stack model and emitted code are updated separately: about 160–200 lines
  outside the scheduler change the model directly, and about 260 call
  `self.asm.emit_*`. Nothing checks that they agree:
  `emit_op_with_effect`'s debug assertion compares the depth against its own
  update, and `ir/verify.rs` checks only physical heights.
- `ir/passes/stack_normalize.rs` imports `codegen::{StackModel,
  resynthesize_physical_ops, lowered_stack_cost}` and reuses `mir::ValueId`
  as a placeholder type for synthetic identities, and `ir/verify.rs` imports
  `codegen::MAX_STACK_DEPTH`, while `codegen/stack/scheduler.rs` imports
  `ir::immediate_materialization_cost`. The two layers depend on each other.
- Four cost types price stack code: `ScheduleCost` with its own objective key,
  `target::Cost`, the `lowered_stack_cost` tuple, and `switch.rs`'s
  `LoweringCost` with byte literals. `calls/mod.rs:372` defines a unitless
  literal cost, and `analyze_resident_subset` uses an unpriced
  `uses < padding * 2` test (`select.rs:229`); the project rules put such
  choices in the target cost model.
- The "plan, apply, retire, drop dead values" sequence is simulated in five
  places (`generate_inst`, `expression_plan`, `binary_window_start`,
  `binary_window`, `plan_static_call_stack`), each with its own
  simplifications. The edge idiom "pop unneeded, materialize missing, shuffle"
  is repeated in about ten places.

Functions over 200 lines:

| Function | Location | Lines |
| --- | --- | ---: |
| `generate_function_body` | `codegen/function.rs:117` | 936 |
| `resolve_static_frames` | `codegen/frames/mod.rs:465` | 468 |
| `emit_icall_static` | `codegen/calls/mod.rs:518` | 363 |
| `plan_operands` | `codegen/stack/scheduler.rs:840` | 319 |
| `plan_live_joins` | `codegen/stack/layout/phi.rs:424` | 304 |
| `select_switch_plan_with_linear_values_and_budget` | `codegen/switch.rs:263` | 290 |
| `generate_custom_inst` | `codegen/instructions.rs:254` | 260 |
| `regenerate_block` | `ir/passes/block_cse.rs:71` | 234 |
| `emit_value_fresh` | `codegen/values.rs:296` | 227 |
| `outline_machine_runs` | `ir/passes/outline.rs:213` | 225 |
| `plan_static_call_stack` | `codegen/calls/mod.rs:240` | 224 |
| `outline_parametric_machine_runs` | `ir/passes/outline.rs:458` | 222 |
| `emit_icall` | `codegen/calls/mod.rs:28` | 210 |

## Proposal

The target shape splits the boundary into a plan and an emitter:

```text
lowered MIR function
  -> FunctionAnalyses   liveness with use counts, loops, CFG, dominators; computed once
  -> StackPlan          instruction order, block entry layouts, spill set, frame needs,
                        calling convention; plain data, one per function
  -> Emitter            replays the plan into EVM IR through one API that updates the
                        stack model and the instruction stream together
  -> EVM IR             carries each block's entry shape for later passes
```

A `StackPlan` depends only on its MIR function, the target, and its callees'
conventions. That makes it testable in isolation, cacheable across retries,
and computable in parallel over the call graph, callees first, as Sonatina
does.

The stages below are ordered so that each one stands alone, keeps output
byte-identical where it says so, and can be measured with the existing
benchmark scripts before the next begins.

### Stage 1: remove repeated work (output unchanged)

- Replace `inst_data`'s slice scan with a per-block position map, and cache
  the zero value per function.
- Store `StackModel` with the top at the end of the vector.
- Run `LoopAnalyzer` once per function and share it; give each body a
  copy-on-write overlay of `StackPhiPlan` instead of a deep clone, since
  bodies change it (`merge_resident`, loop-block inserts); represent spill
  availability as bitsets.
- Compute the `entry` function's block-local liveness once per attempt.
- Pass the hoisted `CfgInfo` from `resident_search_context` into
  `GlobalStackPlan::analyze_resident_args`.
- Avoid the `SpillManager` deep copy in call-site planning by planning on a
  read-only view.
- Cache `plan_operands` results per instruction, start state, and remaining
  search budget for the duration of one `prefer_binary_plan` comparison; the
  same plans are recomputed for the window's shared suffix.

Measure each change against the recorded baseline and require byte-identical
standard-JSON output over `testdata/projects`, as #1609 does.

### Stage 2: fix the scheduler's local decisions

- Bound speculation: charge speculative searches to a separate
  per-function budget, or pass the budget to `plan_operands` explicitly.
  This changes output when the budget runs out, so it belongs here rather
  than in Stage 1.
- Make `drop_dead_values` use `Liveness::is_used_at_or_after`, so a
  surviving dead copy can be popped at the next opportunity rather than at
  block exit.
- Make the scheduler emit what `reorder_pushes` and the DUP-to-POP part of
  `dce` produce, then check whether those passes still change anything on the
  corpus before removing them.
- Merge the operand plan and the following dead-value cleanup into one
  shuffle, so `stack_normalize` sees fewer non-minimal runs.
- Replace the two-instruction lookahead with a block-level ordering step in
  the backend plan, not in MIR, that feeds the expected stack order into the
  DFT traversal as Venom does. `evm-inst-schedule` deliberately leaves
  binary operand orientation to the backend, so the ordering step should
  choose orientation too. Keep the lookahead only if the benchmark shows the
  ordering step loses gas.

### Stage 3: split the god object

- Split `EvmCodegen` into module state (call graph, conventions, frames),
  function state (analyses, plan, spill slots), and block emission state,
  each with its own lifetime so resets are structural, not hand-written.
- Put all emission behind one `Emitter` type that changes the model and the
  instruction stream in one call, and make direct `asm` access private to it.
- Move `ImmediateMaterialization` and its cost into `target.rs`, and move the
  stack resynthesis shared with `stack_normalize` into a module both layers
  can depend on, removing the cycle and the MIR `ValueId` import from EVM IR.
- Fold `ScheduleCost`, `lowered_stack_cost` and `switch::LoweringCost` into
  `target::Cost` queries.
- Split files by responsibility:

| File | Split into |
| --- | --- |
| `stack/scheduler.rs` | operand planner tiers; A* search; cost and objective (to `target`); dead-value cleanup; MIR rematerialization queries |
| `function.rs` | critical-edge splitting (a MIR pre-pass); plan composition; block emission loop; exit-mechanism choice; block order |
| `stack/spills.rs` | slot allocation and colouring; store and reload emission; post-emission dead-store removal (an EVM IR pass) |
| `switch.rs` | plan search and cost model, pure and unit-testable; emission |
| `calls/mod.rs` | call emission; preserve-or-drain planning |
| `frames/mod.rs` | address emission; frame packing and placement; heap-prefix analysis |

### Stage 4: plan before emitting

- Compute block entry layouts with a forward pass in reverse postorder over
  the function: single-predecessor blocks inherit the predecessor's exit;
  a join chooses among its predecessors' exit proposals by real shuffle cost
  under `Target`; backedges are shuffled to the header's fixed entry. This
  replaces `StackPhiPlan`'s live-join fixpoint, the loop and branch phi
  policies, and `GlobalStackPlan` with one mechanism and no layout-size
  limits.
- Let values stay on the stack across blocks by default. Find spills inside
  the shuffler, when a value would fall below reach, and iterate the layout
  pass until the spill set stops growing. The set only grows, so the loop
  terminates, and it runs per function instead of per runtime.
- Keep the existing operand planner for instruction-local work; it already
  improves on the greedy shufflers the surveyed compilers use.
- Decide calling conventions from callee plans in call-graph order, so a
  failure invalidates only that callee and its callers.

Stage 4 changes output. Land it behind an unstable flag first, compare both
corpora with `benchmark-compare.py`, and keep the current path until the new
one wins on `-Ogas` gas and does not regress `-Osize`.

### Stage 5: let EVM IR use the plan

- Extend the existing text-only `BlockMetadata::entry_depth` into a recorded
  entry shape: depth plus anonymous equivalence classes of entry words, with
  no MIR value identities, so scheduler layouts stay private as the
  architecture requires. Verify it in `ir/verify.rs`, and make `tail_merge`,
  `outline`, and `block_layout` keep it correct when they rewrite blocks.
- Let `block_cse`, `dce`, `stack_normalize`, the peephole extractors, and
  outlining start from the recorded shape instead of rebuilding it.
- Once plans are per function, emit functions in parallel within a module.

## Risks

- Stage 4 replaces policies that were each tuned against the benchmark
  corpus. A general layout pass may lose on cases a policy was written for;
  treat each policy's tests as regression tests for the new pass.
- Keeping more values on the stack across blocks raises stack pressure and
  can push operands out of reach. The spill fixpoint handles correctness,
  but its choice of which value to spill decides output quality; solx's
  spill-weight heuristic (use count, loop depth) is a reasonable start.
- Parallel emission requires that nothing outside a function's plan is
  mutated during its emission, which Stage 3 must make true first.
