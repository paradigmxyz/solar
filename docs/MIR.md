# MIR: CFG updates and SSA aggregates

MIR keeps typed values and function calls through optimization. The backend
receives scalar values after progressive lowering, then schedules them onto the
EVM stack. These boundaries serve different purposes: maintaining SSA is a
correctness requirement for every transform; choosing a physical representation
is a late lowering decision.

## CFG edits own phi maintenance

A control-flow edge appears in three places: the source terminator, the target's
stored predecessor list, and each phi's incoming list. Phis use one value per
predecessor block, even when a switch has several cases targeting that block.
Changing only a terminator leaves those other representations stale.

`mir::utils::repair_reachability_phis` reconstructs all predecessor lists and
removes phi inputs whose source no longer branches to the target. It cannot
supply a value for a newly added edge or fix a dominance violation. Calling it
after every pass would both scan unaffected code and hide incomplete rewrites.
The parser's deferred inference of aggregate call and phi types is separate:
it resolves forward references once, without changing control-flow edges.

Use operations with narrow, explicit contracts:

- `fold_terminator_to_jump` keeps an existing successor, removes the other
  successors' phi inputs, and collapses duplicate kept edges without losing
  their value. It preserves the terminator's debug context.
- `invalidate_unreachable_block` clears a block after the caller proves it
  cannot execute. It removes outgoing edges and their phi inputs. Incoming
  backlinks stay until their source terminators change, so dead cycles can be
  processed in either order.
- `split_edge` inserts one block on a logical edge and rekeys the target's phis.
  All duplicate occurrences of that edge go through the same new block.
- A redirect that introduces a predecessor must explicitly supply or construct
  each new phi value. Neither a generic setter nor a repair scan can infer it.

SCCP, check elimination, and DCE use the first two operations. Frame promotion
preserves control flow, so it needs no repair scan. Pure evaluation replaces the
whole body and constructs its links directly. More complex rewrites, including
loop preheader creation, jump threading, and some ABI lowering, still use the
bulk repair helper. Migrating them requires preserving their phi construction
rules, not simply deleting the calls.

The stored CFG and cached analyses are distinct. Updating phis and predecessor
lists does not update dominance, reachability, alias analysis, or loop facts.
The pass machinery invalidates analyses after relevant changes; a pass that
continues querying them after an edit must recompute or update them first.
The verifier checks IR at pass boundaries in debug builds and with
`-Zvalidate-ir`. It should report broken invariants rather than repair them.

### LLVM comparison

LLVM's predecessor iterator follows basic-block uses in terminator operands,
so it does not need our separate predecessor-vector rebuild. Phi incoming
operands still need maintenance: `BasicBlock::removePredecessor` updates phis,
and utilities such as `SplitEdge` handle particular CFG rewrites. Changing a
successor operand alone is not a complete SSA update. `SSAUpdater` helps when
moving or duplicating definitions requires new merges; it is not an automatic
cleanup after every pass. See LLVM's [CFG iterator](https://llvm.org/doxygen/IR_2CFG_8h_source.html),
[BasicBlock implementation](https://llvm.org/doxygen/IR_2BasicBlock_8cpp_source.html),
[CFG utilities](https://llvm.org/doxygen/BasicBlockUtils_8h_source.html), and
[SSAUpdater](https://llvm.org/doxygen/SSAUpdater_8cpp_source.html).

For this index-based IR, local mutation helpers avoid adding an intrusive
use-list system solely to maintain CFG backlinks. More precise analysis
preservation and a general SSA updater can be added when measurements and
transforms justify their cost.

## Aggregate values are not memory objects

`insert_value` and `extract_value` construct and project fixed SSA structs.
They do not allocate storage, copy bytes, or imply an address. A slice field
carries its pointer and length; a memory-object field carries a typed reference,
not a copy of the referenced object.

LLVM calls the matching aggregate operations `insertvalue` and `extractvalue`.
Its `insertelement` and `extractelement` operate on vectors, can take dynamic
indices, and have different target-lowering rules. In the SelectionDAG path,
LLVM flattens aggregate values into component DAG values: insertion substitutes
the selected components and extraction selects them. Vector operations instead
use vector-element DAG nodes, which undergo target legalization. See the
[language reference](https://llvm.org/docs/LangRef.html#aggregate-operations) and
[SelectionDAG builder](https://llvm.org/doxygen/SelectionDAGBuilder_8cpp_source.html).

We already have a dedicated `lower-structs` pass. It keeps field order, flattens
nested structs and slice fields, and rewrites aggregate parameters, results,
phis, selects, and calls. It reserves scalar placeholders before rewriting so
loop-carried aggregates do not depend on block traversal order. For example:

```text
s0 = insert_value {u256, u256}, undef, 0, a
s1 = insert_value {u256, u256}, s0, 1, b
x = extract_value {u256, u256}, s1, 0
```

becomes the value substitution `x = a`, with no load or store. An aggregate
`phi [left: {a, b}], [right: {c, d}]` becomes two scalar phis. Stack scheduling
later decides whether their values need stack moves or spills.

The pass runs after ABI and dispatch construction, before frame and memory
lowering. Internal calls remain typed and aggregate-valued until this boundary.
Here, their results adopt the backend convention: the first scalar result plus
reads of the remaining result words. Those reads occur immediately after the
call so a later call cannot overwrite them. Only this calling convention, real
memory objects, and eventual spills require memory traffic; insertion and
extraction alone do not.

## Output quality and remaining work

Keep aggregates through inlining and high-level optimization, then lower once
at the calling-convention boundary. This keeps return values explicit for
analysis without exposing the shared return buffer too early. Keep physical
stack moves in the MIR-to-EVM scheduler and byte offsets in the assembler.
Adding another insert/extract lowering pass would duplicate `lower-structs`.

There is room to improve aggregate-aware simplification before lowering:
forward fields through insert/extract chains, remove unused fields and return
components where all callers allow it, and simplify scalar phis exposed by
lowering. These are distinct optimizations, not prerequisites for valid SSA.
Early flattening can expose scalar optimizations but also enlarge IR and extend
live ranges; materializing aggregates in memory adds aliasing and memory costs.
Neither should be chosen without checking generated code.

Compare runtime gas first, then gas- and size-mode bytecode, with compile time
as a tie-breaker. Check the same successful corpus cases and serialized bytecode,
not only total size. CFG maintenance changes must retain behavior; fewer repair
scans alone are not evidence of better generated code. Debug metadata must stay
bytecode-neutral throughout these rewrites.
