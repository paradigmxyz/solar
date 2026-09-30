# Compositional dataflow analysis

This document describes the MIR dataflow framework in
`crates/codegen/src/mir/analysis/dataflow/`: the research it builds on, how its
pieces map onto existing MIR analyses, the security and optimization clients it
serves, how tests state facts at program points, and the milestone plan. Sections
marked *planned* describe later milestones. The diagnostics and abstract domains are
experimental, not a soundness certificate for the absence of vulnerabilities.

The framework only reads MIR. Requesting `-Zdataflow` prints facts and emits
diagnostics but leaves generated code unchanged. Transformations that consume its
facts are ordinary MIR passes with their own tests and runtime coverage. By default,
storage summaries are used only as an inlining profitability hint: they never authorize
deleting a storage access or guard. Experimental effect-based transformations require
`-Zdataflow-optimizations`.

## Goals

- A generic framework: lattices with widening, forward and backward transfer
  functions over the MIR CFG, and a worklist solver.
- On-demand, summary-based interprocedural analysis over the internal call graph,
  with configurable call-string sensitivity (`k = 0` gives pure summaries) and
  heap/object sensitivity for storage pointers.
- Path sensitivity: branch and check conditions refine the state on each edge.
- A storage location abstraction with field, index, and mapping-key sensitivity
  that follows storage pointers through internal and library functions, including
  every case collected in [crytic/slither#515](https://github.com/crytic/slither/issues/515).
- Cross-contract reasoning: decide when an external call cannot call back, from the
  compiled code of contracts created here.
- Reentrancy and state-inconsistency lints covering every variant in Sailfish.
- A plug-in point for numeric domains (intervals, rounding direction, units), with
  an SMT backend kept optional.
- Tests that assert facts at every program point.
- Non-local transformations that the facts enable, measured on the runtime benchmark.

## Sources

### Compositional and on-demand analysis

- **Selective control-flow abstraction via jumping** (Blackshear, Chang, Sridharan,
  OOPSLA 2015, [paper](https://plv.colorado.edu/papers/controlfeasibility-oopsla15.pdf)).
  A backward, goal-directed analysis jumps from a query directly to the commands that
  may affect it, found through a precomputed relevance relation, instead of walking
  every predecessor. Skipping a command is sound when its transfer cannot weaken the
  query. *Takeaway:* keep a flow-insensitive index of who writes each storage
  location, and treat reentrant entries as unordered events whose feasibility is
  filtered by guards. Our reentrancy checker does exactly that at each external call.
- **Thresher** (Blackshear, Chang, Sridharan, PLDI 2013,
  [paper](https://plv.colorado.edu/papers/thresher-pldi13.pdf)). Alarms from a cheap
  analysis are refuted on demand by a backward, path-sensitive witness search with a
  per-query budget; running out of budget keeps the alarm. *Takeaway:* separate cheap
  summaries from refutation, keep path constraints separate from heap facts, and make
  every approximation fail toward reporting. Our guards are that refinement step.
- **RacerD** (Blackshear, Gorogiannis, O'Hearn, Sergey, OOPSLA 2018,
  [paper](https://ilyasergey.net/papers/racerd-oopsla18.pdf)). Each method's summary is
  a set of access snapshots over syntactic access paths, with lock and ownership
  state; a separate reporting phase pairs conflicting accesses of non-private methods.
  *Takeaway:* reentrancy is concurrency with one interleaving point per external call.
  Summaries record storage accesses with the external calls that precede them and
  their lock guards; the checker pairs them across public entries.
- **Bi-abduction and Infer** (Calcagno, Distefano, O'Hearn, Yang, POPL 2009 and
  JACM 2011; Distefano, Fähndrich, Logozzo, O'Hearn, "Scaling static analyses at
  Facebook", CACM 2019,
  [article](https://cacm.acm.org/research/scaling-static-analyses-at-facebook)).
  Procedures are summarized bottom-up and independently of callers as
  pre/postcondition pairs, which enables incremental analysis. Infer's abstract
  interpretation framework ([`absint`](https://github.com/facebook/infer/tree/main/infer/src/absint))
  separates `AbstractDomain`, `TransferFunctions`, schedulers, and an
  `analyze_dependency` query that summarizes callees on demand. *Takeaway:* our
  `Analysis`, `solve`, and `SummaryEngine::summary` follow the same split.
- **Incorrectness logic and Pulse** (O'Hearn, POPL 2020; Le, Raad, Villard, Berdine,
  Dreyer, O'Hearn, "Finding real bugs in big programs with incorrectness logic",
  OOPSLA 2022,
  [paper](https://people.mpi-sws.org/~dreyer/papers/finding-real-bugs/paper.pdf)).
  Summaries carry error exits, and an error whose trigger depends on the caller is
  *latent* until a context makes it *manifest*. *Takeaway:* summaries keep their
  normal-return precondition, so a callee's guard becomes a caller fact instead of
  being reported inside the callee.
- **Move** (Blackshear et al., "Resources: a safe language abstraction for money",
  [arXiv:2004.05106](https://arxiv.org/abs/2004.05106); Blackshear, Mitchell, Nowacki,
  Qadeer, "The Move borrow checker",
  [arXiv:2205.05181](https://arxiv.org/abs/2205.05181); Zhong et al., "The Move
  Prover", CAV 2020; Dill et al., TACAS 2022,
  [arXiv:2110.08362](https://arxiv.org/abs/2110.08362); Patrignani, Blackshear,
  "Robust safety for Move", CSF 2023). Move rules out reentrancy by construction: no
  dynamic dispatch, and global storage accessible only to its declaring module. The
  prover's usage analysis summarizes the memory each function touches, and its
  reference elimination turns references into tracked origins. *Takeaway:* the
  analysis must recover what Move's type system guarantees: which storage an external
  call can change (the union of what reentrant entries write) and which references
  point where (storage paths with parameter origins).
- **Interprocedural foundations.** Sharir and Pnueli's functional approach
  ("Two approaches to interprocedural data flow analysis", 1981) computes transformers
  per procedure; IFDS (Reps, Horwitz, Sagiv, POPL 1995) builds per-fact summary edges;
  k-CFA (Shivers, 1991) distinguishes call strings; object sensitivity (Milanova,
  Rountev, Ryder, TOSEM 2005) and "Pick your contexts well" (Smaragdakis, Bravenboer,
  Lhoták, POPL 2011) show that the context element matters more than its depth.
  *Takeaway:* parametric summaries are exact for substitution domains such as storage
  paths, while numeric domains need contexts; the storage pointer passed to a callee is
  the Solidity analogue of the receiver object.
- **ESP** (Das, Lerner, Seigle, PLDI 2002,
  [paper](https://cseweb.ucsd.edu/~lerner/PLDI02-esp.pdf)). Property simulation keeps
  paths apart only when they disagree on the property state and merges the rest.
  *Takeaway:* track only the facts guards need (fixed-slot values and caller checks)
  path-sensitively and join everything else.

### Smart contract analyses

- **Sailfish** (Bose, Das, Chen, Feng, Kruegel, Vigna, "SAILFISH: Vetting smart
  contract state-inconsistency bugs in seconds", IEEE S&P 2022,
  [paper](https://fredfeng.github.io/papers/sailfish.pdf), arXiv:2104.08638). An
  Explorer builds a storage dependency graph and finds hazardous access pairs (a stale
  read or a destructive write) reachable from an external call, or a transfer whose
  guard, amount, or receiver depends on storage another transaction writes. A Refiner
  refutes candidates with value summaries of storage variables. Its false-positive
  analysis names classes that must not be reported. The catalog below lists every
  example and our result.
- **Slither data-flow branches.** `dev-data-flow-z3` adds a generic engine
  (`Domain`, `Analysis`, `apply_condition`, loop widening), a Z3-backed interval
  analysis with overflow queries, and a data-flow reentrancy detector; Z3 is a
  mandatory dependency there and the interval analysis has no detector. The rounding
  analysis lives on `dev-data-flow-rounding`: a finite lattice of `UP`, `DOWN`,
  `NEUTRAL`, and `UNKNOWN` tags with name-based seeds, a ceiling-division idiom, and
  inconsistency findings. Both inline callees per call site rather than caching
  summaries, and neither models storage pointers passed to calls.
- **crytic/slither#515** tracks alias-analysis gaps: writes through storage pointers
  passed to internal and `using for` library functions, pointers returned from
  functions, deletes through pointers, and field or index insensitivity. Current
  Slither still misses several of these; the table below lists our results.
- **Dimensional analysis** (Trail of Bits,
  [blog, 2026-03-25](https://blog.trailofbits.com/2026/03/25/try-our-new-dimensional-analysis-claude-plugin/)).
  An LLM annotates values with units and decimal scales such as `D18{tok/share}`, then
  checks the algebra mechanically: multiplication adds scales and multiplies units,
  addition requires equal dimensions. The algebra is an abstract domain that a
  compiler can check once the annotations exist.

## Architecture

```text
lattice      JoinSemiLattice, Flat, PowerSet, MustSet, MapLattice, Reachable, products
engine       Analysis (instruction, terminator, edge, phi transfer), solve, replay
interproc    InterproceduralAnalysis, SummaryEngine (on demand, SCC fixpoint), Context
storage_path PathTable, PathNode, KeyTerm, PathSet, alias queries, instantiation
storage      value -> paths, per-function storage footprints and returned pointers
taint        value -> sources, storage flows
liveness     backward liveness of storage writes
value        ValueDomain plug-in API and the generic value analysis
interval     unsigned intervals
rounding     rounding directions
units        dimensions and decimal scales
slot_state   SymWord, Pred, SlotState: exact slots relative to entry, guards
reentrancy   events, contract model, trust, checker, findings
```

| Requirement | Implementation | Reused analyses |
| --- | --- | --- |
| Generic lattices and solver | `lattice.rs`, `engine.rs`: forward and backward problems, RPO worklist, widening at retreating edges after two joins, per-edge phi binding, replay for recording | `CfgInfo` for RPO; `Terminator` successors |
| Summaries, k contexts, on demand | `interproc.rs`: `SummaryEngine::summary` computes lazily, iterates recursive cycles Tarjan-style with widening, bounds contexts per function | `CallGraphInfo` for constructor reachability |
| Heap/object sensitivity | the storage analysis' entry abstraction is the actual argument paths; `-Zdataflow-k=1` summarizes a callee once per storage object | |
| Path sensitivity | `Analysis::apply_edge` and check/require transfers; `SlotState::assume` prunes contradicting edges | `Builtin::Check`/`Require` semantics |
| Storage aliasing | `storage_path.rs`: mapping, array data, element, field, region nodes; parameter-relative summaries | `AliasAnalysis::instruction_mod_ref` for accesses inside semantic operations; `StorageAlias` |
| Reentrancy and TOD | `reentrancy.rs` + `slot_state.rs` | `EffectKind` for call classification |
| Cross-contract trust | initcode reconstructed from the creation buffer and scanned with the opcode table's new `EXECUTES_CODE` trait | `backend/evm/op.rs` opcode table |
| Program-point facts | `-Zdataflow=<analyses>` dump, FileCheck in UI tests | tester `filecheck` directive |

Classification follows the repository rules: external calls are recognized through
`EffectKind::ExternalCall`/`Create`, instructions' storage effects through ModRef,
and whether bytecode can call out through a new opcode-table trait. The only matches
on specific operations extract operands whose meaning differs between call forms
or storage operations.

The existing `memory_summary` keeps exact-slot footprints for optimization passes and
widens symbolic slots to the whole address space. The path summaries here are more
precise but live in the dataflow layer; wiring them into shared ModRef queries is a
later milestone, after the transformations prove the need.

### Framework API

```rust
trait JoinSemiLattice: Clone {
    fn join(&mut self, other: &Self) -> bool;
    fn widen(&mut self, other: &Self) -> bool { self.join(other) }
}

trait Analysis {
    type Domain: JoinSemiLattice;
    const DIRECTION: Direction = Direction::Forward;
    fn bottom(&self, func: &Function) -> Self::Domain;
    fn initialize_boundary(&mut self, func: &Function, block: BlockId, state: &mut Self::Domain);
    fn apply_instruction(&mut self, func: &Function, block: BlockId, inst: InstId, state: &mut Self::Domain);
    fn apply_terminator(&mut self, func: &Function, block: BlockId, state: &mut Self::Domain) {}
    fn apply_edge(&mut self, func: &Function, edge: &Edge, state: &mut Self::Domain) {}
    fn apply_phi(&mut self, func: &Function, phi: InstId, incoming: ValueId, edge: &Edge, state: &mut Self::Domain) {}
}

trait InterproceduralAnalysis: Sized {
    type Entry: Clone + Eq + Hash + Debug;
    type Summary: JoinSemiLattice + PartialEq + Debug;
    fn general_entry(&self, module: &Module, func: FunctionId) -> Self::Entry;
    fn bottom_summary(&self, module: &Module, func: FunctionId) -> Self::Summary;
    fn unknown_summary(&self, module: &Module, func: FunctionId) -> Self::Summary;
    fn summarize(engine: &mut SummaryEngine<'_, Self>, func: FunctionId, context: &Context<Self::Entry>) -> Self::Summary;
}
```

A client's `summarize` runs `engine::solve` over the function, calling
`engine.callee_context` and `engine.summary` from its call transfer. Clients that
record events do so in a second `replay` pass over the fixed point so that
intermediate iterations never leak into a summary. `k = 0` makes every context the
callee's most general entry; with `k > 0` a context holds up to `k` call sites plus the
entry abstraction, collapsing to the general entry after sixteen contexts per function
or when a call re-enters an active function.

### Storage paths

A path records how a slot is computed: `slot(c)`, a formal storage-pointer
parameter `argN`, `base[key]` for mapping entries, `data(base)` for dynamic-array data,
`base<index xStride>` for elements, `base.offset` for fields, and `region(base)` for
whole byte arrays and dynamic arrays accessed by semantic operations. A value may
denote up to four alternative paths. Alias queries assume Keccak-256 is collision
free and that hashed locations lie far from low absolute slots and from each other,
the assumptions behind solc's layout. Keys compare per activation: within one
function activation the same SSA value is the same key; across reentrant calls only
constants and `caller` are meaningful; across transactions of different senders,
entries keyed by `caller` are distinct.

Summaries are relative to parameters and callee-local keys become `*`, so a caller
instantiates `arg0[arg1]` in `Roles.add(role, account)` as `slot(1)[arg0]` for its own
arguments. Returned pointers are summarized the same way. Word granularity is a
limitation: packed fields of one slot share a path.

### Clients

- `storage` prints the paths read and written by each storage access and call, and
  each function's footprint and returned pointer paths.
- `taint` prints each value's sources (arguments, environment reads named by their
  operation, storage paths, immutables, external results) and the taint written to
  each path. Memory is one flow-sensitive, weakly updated set per function.
- `reentrancy` prints per-instruction events, each function's exit state and
  precondition, and findings, and emits warnings.
- `liveness` is a backward client: it marks each storage write live or dead, where a
  write is dead when every later path overwrites it or reverts before a read, a call
  that may read it, or a successful end. Internal calls use their instantiated read
  footprint, so a write that a callee does not read can still be dead.

`slot_state.rs` tracks words of exact persistent and transient slots bit by bit:
known constant bits and bits copied from an entry value, so packed booleans written
with read-modify-write sequences stay exact. Branches and checks over those bits or
over `caller` become guards; a contradiction makes the edge unreachable. Summaries
compose guards into the caller: a callee constraint over its entry becomes a caller
constraint when the slot is unchanged, is decided when the caller knows the value,
and is dropped otherwise. OpenZeppelin's `_nonReentrantBefore()` therefore yields
`exit: slot(0)=2 requires entry(slot(0)) != 2`, and every guarded entry is
infeasible at a call made while the lock is held.

### Reentrancy model

At each external call that may run code that calls back, a public entry `g` is
reentrantly callable if it can commit (some normal return or `stop` is feasible in
the state at the call and is not restricted to privileged callers). Let `f1` and `f2`
be the caller's accesses before and after the call. The interleaving `f1 g f2` is
harmless when it is equivalent to a serial order: to `f g` when `f2` has no access
conflicting with `g`, or to `g f` when `f1` has none. The checker reports a storage
hazard only when neither holds:

- a stale read: `f2` writes a path that `g` reads, provided `g` can act on it (a view
  returns it; otherwise `g` has a feasible write, call, or event);
- a destructive write: `g` writes a path that `f2` accesses;
- read-only reentrancy: a stale read by a view function, which is then inconsistent
  only because `f1` already updated another path the view reads;
- event reordering: both `f2` and `g` emit events.

The serial-order check removes the common "call first, update afterwards" pattern,
such as crediting a deposit after `transferFrom`, whose reentrant executions are
equivalent to running `g` first. Each entry reports at most one finding of each kind.

Labels describe the vector: single-function, cross-function, read-only,
delegatecall, contract creation, or cross-contract (a call into compiled code that
itself calls out).

The experimental lint classifies a call as unable to call back when its target is a precompile, the zero address, or code
compiled here without call instructions: held in an immutable
or a slot that only the constructor writes with such an address. Deferred bytecode from
`new C()` is opaque before final linking and is conservatively allowed to call back.
For literal buffers, initcode is
reconstructed from the constant bytes written to the start of the creation buffer,
whether copied with `data_copy` or stored as words. `transfer` and `send` forward only
the stipend, which cannot execute `SSTORE` but can execute `TSTORE`. These classifications
are lint heuristics, not optimization proofs: a constant address may contain code on
another chain or fork, and recognizing an initcode prefix does not establish the bytes
deployed after ordered memory writes and constructor execution. In particular, the
optimizer does not use created-code trust or the stipend to suppress callbacks.
Targets chosen by the deployer or an owner
still run their own code, so they remain reentrancy vectors. After a call that may
call back, the analysis forgets every exact slot that some runtime entry writes.

Owner-only entries are found by a greatest fixed point: an origin is privileged when
it is an immutable, a constant, or a slot that only owner-only entries write at runtime,
and an entry is owner-only when all its writes and calls require `caller` to equal a
privileged origin. Starting from every compared origin lets an owner slot that only the
owner can change stay privileged.

Findings on the archived project inputs are not confirmed bugs. For example, Seaport's
guard chooses between `sstore` and `tstore` at runtime, Morpho relies on owner-enabled
rate models, and opaque created helpers may produce conservative false positives.
`QuietFactory` and `QuietVault` explicitly test this last limitation.

Transaction-order dependence is reported when the value or recipient of a value
transfer in a non-owner-only entry depends on storage that another entry writes.
Owner-only writers still count, as in Sailfish's owner-set price example.

## Sailfish catalog

Every example in the paper is a UI test under
`tests/ui/codegen/dataflow/reentrancy/`. The paper's hazard names are stale read (SR)
and destructive write (DW); TOD is its event-ordering class.

| Example | Hazard | Sailfish | Ours | Test |
| --- | --- | --- | --- | --- |
| Fig. 1a Bank | single-function SR | reported | reported | `sailfish_fig01a_bank.sol` |
| Fig. 1b Queue | DW race, no Ether | not a TOD finding | not reported | `sailfish_fig01b_queue.sol` |
| Fig. 2 Split | cross-function DW; TOD amount | reported | both reported | `sailfish_fig02_split.sol` |
| Fig. 2 without `updateSplit` | none | not reported | not reported | `sailfish_fig02_split_no_update.sol` |
| Fig. 3 Mutex | refuted by mutex | refuted | not reported | `sailfish_fig03_mutex.sol` |
| Fig. 13 cross-function | cross-function DW; TOD receiver | reported | both reported | `sailfish_fig13_cross_function.sol` |
| Fig. 14 delegatecall | delegate-based SR | reported | reported | `sailfish_fig14_delegatecall.sol` |
| Fig. 15 Bet | TOD amount | reported | reported | `sailfish_fig15_bet.sol` |
| Fig. 16 nonReentrant | refuted by lock | refuted | not reported | `sailfish_fig16_nonreentrant.sol` |
| Fig. 17 owner-set target | Sailfish false positive | reported (FP) | not reported | `sailfish_fig17_owner_target.sol` |
| Fig. 18 owner withdraw | TOD false positive | reported (FP) | not reported | `sailfish_fig18_owner_withdraw.sol` |
| App. II-A CREAM/AMP hook | hook reentrancy | reported (simplified) | reported | `sailfish_cream_hook.sol` |
| App. II-A owner-set price | TOD | true TOD | reported | `sailfish_tod_prices.sol` |
| App. II-A supply-dependent price | TOD | true TOD | reported | `sailfish_tod_prices.sol` |
| Sec. VIII-B housing tracker | data-corrupting reentrancy | true bug | reported | `sailfish_housing_tracker.sol` |
| Sec. VIII-B FP classes (a)-(e) | none | not reported | not reported | `sailfish_false_positive_classes.sol` |
| Sec. II create-based | constructor callback | combined SDG | reported | `create_callback.sol` |

The figures are reconstructed for Solidity 0.8. Fig. 2's payees are elided in the
paper; the tests pass them as parameters so that the calls are untrusted without
introducing an unrelated transaction-order finding. Additional variants cover
read-only reentrancy (`read_only.sol`), cross-contract calls through created code
that does or does not call out (`cross_contract.sol`), event reordering
(`event_ordering.sol`), OpenZeppelin's interprocedural guard with one unguarded entry
(`openzeppelin_guard.sol`), and a transient-storage lock (`transient_guard.sol`).

## crytic/slither#515 cases

`tests/ui/codegen/dataflow/storage/slither_515.sol` and
`tests/ui/codegen/dataflow/taint/slither_515.sol` check each case.

| Case | Expected fact | Ours |
| --- | --- | --- |
| #70 local pointer to a state struct | writes `balances.balance` | `slot(0)` |
| #82 local reference to an inner mapping | writes `map[0][0]` | `slot(0)[0][0]` |
| #87 pointer reassigned under a condition | may write `balances1` or `balances2` | `{slot(1), slot(0)}` |
| #112 pointer returned from a private function | writes `a.test` or `b.test` | `{slot(0), slot(2)}` |
| #270 mapping entry passed as a parameter | writes `map[msg.sender].val` | `slot(0)[caller]` |
| #2598 `using for` library parameter | writes `_minters.bearer[a]` | `slot(1)[arg0]` |
| #602 delete of one field via a parameter | clears `tickets`, writes `tail` | `slot(0)`, `data(slot(0))<*>`, `slot(1)` |
| #1286 library over `uint256[1]` | writes `params[key]` | `slot(1)<arg0>` |
| #456 nested member as a parameter | writes `limits[w].dailySpent` | `slot(0)[arg0].1` |
| returned pointer to an array element | writes `x[i].v` | `data(slot(0))<arg0 x2>` |
| nested mapping and struct path | writes `m[k].a.c` only | `slot(0)[arg0].1` |
| delete through a pointer and a parameter | clears `m[k].x`, `m[k].ys`, `single.ys` | exact paths |
| shared callee returning each caller's array | per-caller arrays | `slot(0)`, `slot(1)` |
| #1742 shared identity callee | `a` depends only on `paramA` | per-caller `arg0` |
| #2288 mapping read depends on its key | depends on `msg.sender` | `caller` in the taint |
| #1436 timestamp in one struct field | `posts.length` is not timestamp-tainted | length taint has no `timestamp` |

The object-sensitive revision of `storage/contexts.sol` shows the same summaries
computed per storage object with `-Zdataflow-k=1`.

## Abstract-domain plug-ins

Numeric and tag domains plug in as a value domain over SSA values (`value.rs`):

```rust
trait ValueDomain: JoinSemiLattice + Eq + Hash + Debug + Display {
    const NAME: &'static str;
    fn top() -> Self;
    fn constant(value: U256) -> Self;
    fn transfer(cx: &DomainCx<'_>, op: Op, operands: &[Self]) -> Self;
    fn refine(op: Op, operands: &mut [Self], taken: bool) -> bool { true }
    fn call_seed(callee: &str) -> Option<Self> { None }
    fn is_empty(&self) -> bool { false }
    fn check(cx: &DomainCx<'_>, op: Op, operands: &[Self], findings: &mut Vec<String>) {}
}

trait Seeded: ValueDomain {
    fn parse_seed(text: &str) -> Option<Self> { None }
}
```

`ValueAnalysis<D>` implements the engine's `Analysis` over `MapLattice<ValueId, D>` and
`InterproceduralAnalysis` with the argument abstractions as entry. Transfer functions
match the schema-generated `Op` view, as ISLE rules do, so there is no parallel
operation table. Branch conditions and passing checks call `refine` on the comparison
that produced them, and an empty refinement makes the edge unreachable. After widening,
two descending rounds (`engine::narrow`) recover bounds such as a loop's limit. With
`k = 0` a callee is summarized once over its seeded or unknown arguments; with `k > 0`
each context has its own exact result. Seeds come from NatSpec: the driver matches MIR
functions to their declarations by span and passes each parameter's name and
documentation, and each result's documentation, as text for the domain to parse.

- **Intervals** (`-Zdataflow=intervals`, `interval.rs`) over unsigned words with EVM
  wrapping, checked arithmetic that keeps the non-wrapping part, comparison refinement,
  and widening to the end of the word. A passing `require(i < 5)` proves the following
  `data[i]` bounds check of a ten-element array.
- **Rounding direction** (`-Zdataflow=rounding`, `rounding.rs`): sets of up, down, exact,
  and unknown, following Slither's rounding analysis. Division rounds down unless it is
  the ceiling idiom; subtraction and division invert the second operand; calls named
  like `mulDivUp` or `divWadDown` seed their results, as do parameters named with an
  `_UP` or `_DOWN` suffix. Mixing opposite directions and dividing by a value rounded
  the same way as the numerator are reported.
- **Units and scales** (`-Zdataflow=units`, `units.rs`): products of base units with
  integer exponents and a decimal scale, parsed from `@param amount D18{tok}` and
  `@return D18{share}`. Multiplication adds scales and exponents, powers of ten are pure
  scales, other literals adopt the other operand's dimension in sums, and adding or
  comparing different dimensions is reported.
- **Products** of domains are tuples of lattices; a reduced product only needs a
  `reduce` hook on top of the tuple.

Findings from value domains print in the dump and are emitted as warnings at the
instruction's source location.

An SMT backend stays optional. The repository already drives cvc5 from Python in
`scripts/evm-rules/` for offline proofs; a refutation client would do the same behind a
Cargo feature or an external process, never as a mandatory dependency, and would treat
solver timeouts as "not refuted".

## Program-point facts in tests

`-Zdataflow=storage,taint,reentrancy,liveness,intervals,rounding,units` runs the named analyses on each contract's MIR
before optimization, or on parsed MIR input before its pipeline, and prints one
section per analysis and contract:

```text
// === dataflow reentrancy (k=0): Bank.sol:Bank ===
fn @withdraw:
  bb1:
    v10 = address_call v6, v8, gas v7, value arg0  ; call to caller
    sstore v13, v15  ; write slot(0)[caller] after call#0.11
  exit:
finding: reentrancy single-function @withdraw call#0.11: ...
```

Each instruction line is the MIR text followed by `;` and the analysis' facts, so
FileCheck patterns anchor on the instruction and match the fact. `-Zdataflow-k=N`
selects call-string sensitivity; specialized contexts print their call sites and entry.
Diagnostics from the reentrancy analysis are checked with the tester's `//~ WARN:` and
`//~ NOTE:` annotations.

We chose a textual dump over source annotations such as `@assert` comments because the
tester already runs FileCheck with captures, negative checks, and revisions, the dump
is also useful interactively, and facts about MIR values have no stable source syntax.
The dump runs only when requested and is printed in contract order; contract codegen is
serialized while it is active so output is deterministic.

## Optimizations

`StorageFacts` (`facts.rs`) snapshots the storage path analysis for one module and answers,
for any instruction, which paths it may read or write. Internal calls answer with their
callee's summary instantiated at the call site. Static calls write nothing but can observe
storage through callbacks. Only fork-active precompiles are considered callback-free.
Other external calls, including those reached through internal helpers, may read or write
arbitrary storage. Limiting callbacks to this module's public functions is not sound when
the code executes through delegatecall. Persistent and transient accesses are both included.
Passes select candidate functions before querying summaries and never consult stale facts
for instructions they create.
Querying facts never changes bytecode by itself (`crates/solar/tests/it/dataflow.rs`).

1. **Storage-read inlining** (`inline-storage-reads`, gas mode). Small internal helpers
   whose summaries overlap another read in the caller become inlining candidates. The
   existing inliner retains its recursion, code growth, frame, and lifetime-cost checks.
   Summaries affect profitability only; existing CSE independently proves that an exposed
   load can be removed. Tests: `tests/ui/codegen/mir/inline-storage-reads/`.
2. **Storage forwarding across calls** (`storage-load-cse`, experimental). A cached load survives a call
   whose footprint cannot write its paths. Before this, the pass forgot every load at a
   call. Test: `tests/ui/codegen/mir/storage-load-cse/across_calls.mir`.
3. **Interprocedural and path-identity dead-store elimination** (`storage-dse`, experimental). A call
   kills only the pending stores whose paths it may read; successful transaction termination
   prevents a later overwrite from making an earlier store dead. Stores to equal stable paths count as the same
   slot even through different SSA slot values. Test:
   `tests/ui/codegen/mir/storage-dse/across_calls.mir`.
4. **Guard removal** (`guard-elim`, experimental, gas mode only, after the late storage DSE). It finds an
   exact-slot store and every restore of the slot's prior value that it dominates. If no
   instruction in between can call back into the contract or touch the slot in another
   way, it deletes both stores and replaces reads with the stored value. The entry check
   of the guard stays. Functions with gas observations are skipped. Tests:
   `tests/ui/codegen/mir/guard-elim/guard_elim.mir` and `runtime.sol`, a run-call test
   in which a guard around a call that can call back keeps reverting with `locked`.

The last three extensions require `-Zdataflow-optimizations`; the pre-existing storage
CSE/DSE behavior remains enabled without it. Cross-function check elimination (feeding
interval contexts into `check-elim`) is not implemented.

### Measured results

The existing OpenZeppelin VestingWallet workload provides a production-contract example
of storage-read inlining. Compared with base `4cc64e740`, with the archived input's normal
optimizer settings and the `hot` gas profile:

| Measurement | Base | Candidate | Delta |
| --- | ---: | ---: | ---: |
| `releasable()` transaction, each of two repetitions | 23,746 | 23,601 | -145 gas (-0.61%) |
| `vestedAmount` transactions, each of fourteen calls | varies | base + 11 | +11 gas |
| Sum of the sixteen measured transactions | 379,470 | 379,334 | -136 gas (-0.04%) |
| Runtime bytecode | 1,498 | 1,521 | +23 bytes (+1.54%) |
| Deployment | 407,602 | 411,674 | +4,072 gas (+1.00%) |

The emitted MIR has one load of the released-ETH slot in the inlined `releasable` path;
without this transformation the helper and its caller load it separately. This is a
small measured tradeoff, not evidence of broad gas savings. The experimental guard/CSE/DSE
extensions have no demonstrated gas benefit on the existing production workloads.

Across the full corpus, all 23 runtime workloads and their 416 observations passed,
as did all nine compilation-only projects. Comparable gas totaled 16,823,412 before
and 16,823,276 after. The 22 gas/bytecode-dependent LibString brutalizer calls are
excluded from gas comparisons, not from execution checks. The Uniswap v2 fixture
is incompatible with the pinned Solidity parser (`chainid` without parentheses)
and is not counted as a passing workload. Equal-weight geometric-mean runtime size
increased 0.04%; the other changed runtime sizes were Governor +78 bytes, LibString
+25, SignatureChecker +2, and LilWeb3 Fractional -63. No other comparable runtime
gas changed. Single compile samples do not establish a compiler-speed improvement;
CodSpeed and repeated same-profile measurements are the compile-speed acceptance checks.

Local CLI wall-time measurements used frozen debug binaries, one thread, all-function
codegen, two warmups and nine interleaved samples, with no concurrent builds or benchmarks.
Median `chains` compilation was 241.87 ms on the base and 239.38 ms on revision
`e9fb8d869` (-1.03%). The LilWeb3 project measured
91.50 ms on base and 92.51 ms on this revision (+1.09%). These are wall-clock measurements,
not CodSpeed's simulated instruction counts, and should not be treated as interchangeable.
The [CI runtime benchmark](https://github.com/paradigmxyz/solar/actions/runs/36746997986)
independently reproduced the gas and size results against this same base.

Reproduce with separate frozen base and candidate compiler binaries using
`benches/runtime/benchmark.py --mode runtime compile-time --suite all --gas --gas-profile hot
--start-anvil`, saving results and artifacts for each, then compare them with
`benches/runtime/benchmark-compare.py`. The runner defaults to Solar only. Saved reference
results can be supplied with `--reference-results`; no reference compiler need run locally.

A warm reload costs 100 gas, but guard-removal savings depend on initial slot values,
warmness, and the transaction's refund cap. Refunds do not imply a universal 200-gas
set/restore cost or a sub-10% upper bound on transaction savings.

## Milestones

| Milestone | Scope | Status |
| --- | --- | --- |
| M0 | This design | done |
| M1 | Framework core, storage paths, taint, reentrancy with path-sensitive guards, `-Zdataflow` dumps, Slither #515 and Sailfish tests | implemented; diagnostics experimental |
| M2 | Abstract-domain plug-in API with intervals, rounding direction, and units, NatSpec seeds, and narrowing | implemented; not used as optimization proofs |
| M3 | Storage-read inlining and experimental storage forwarding, DSE, and guard removal | partial; three production-corpus gas wins not established |
| Later | Cross-function check elimination; summaries for public library functions reached through `delegatecall`; privileged origins through role mappings | planned |

## Limitations

- Storage paths have word granularity, so packed fields of one slot conflict.
- Public library functions called through `delegatecall` are opaque and write the
  unknown path; internal library functions are analyzed like any internal function.
- Loop-carried slot arithmetic, such as a pointer stepped in a loop, widens to the
  unknown path.
- Guards capture equalities over fixed slots and `caller`; range checks such as
  `lock < 2`, guards whose slot is chosen at runtime, and role checks through mappings
  are not guards.
- Targets enabled by privileged accounts, such as registered oracles, are untrusted.
- Taint through memory is one set per function.
- Hash inputs are assumed collision-free. Absolute slots may be known hash outputs;
  unconstrained offset arithmetic can wrap. Variable-length mapping keys are unknown
  because the pointer to their bytes is not a key identity. Unbounded regions may alias
  arbitrary absolute slots.
- Created-code trust in the lint is heuristic, not a proof of callback freedom. Optimizers
  treat created targets as opaque and external callbacks as accessing arbitrary storage.
