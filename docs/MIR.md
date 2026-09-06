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

Each transform maintains these structures at its edit sites. There is no
whole-function phi repair pass. A repair scan could remove stale inputs but
could not supply values for newly added edges or fix dominance violations.
The parser's deferred inference of aggregate call and phi types is separate:
it resolves forward references once, without changing control-flow edges.

Use operations with narrow, explicit contracts:

- `replace_terminator` updates only affected successors. It requires phi inputs
  for a new edge before changing the terminator, retains inputs on kept edges,
  and removes them from dropped edges. It preserves debug metadata.
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

SCCP, check elimination, DCE, ADCE, CFG simplification, and jump threading use
these operations. Loop preheader creation prepares its phis before redirecting
edges. PRE uses `split_edge`; ABI continuation splitting transfers successor
links and phi inputs with the terminator. Replacing a body with a helper call
also removes the original body's outgoing edges. Frame promotion preserves
control flow, and pure evaluation reconstructs the whole body directly.

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

A raw `u256` field can carry all bits of a nominal object reference. Keep that
loss of type information explicit: `word_cast` preserves the bits and yields a
raw word; `memory_object_from_ptr` gives a word an object type without proving
validity or ownership. Neither operation allocates or copies memory. Aggregate
lowering inserts `word_cast` when a raw field contains a nominal reference;
memory-object lowering erases both conversions. Alias analysis follows their
unchanged addresses.

The verifier checks nominal object kinds against semantic accesses, while
retaining compatibility with raw pointer carriers during lowering. It also
checks ordinary return counts and void signatures. A shared returnability
analysis follows tail-call chains, including cycles, so forwarding a call
cannot hide an incompatible return signature. Proven nonreturning chains stay
exempt. Slices as well as structs must be gone at the EVM-shaped boundary.

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

Instruction simplification forwards projections through insertion chains before
lowering. Its bounded walk handles overwritten fields and nested aggregates
without hanging on malformed unreachable cycles. It preserves nominal type
changes for the explicit conversion at the lowering boundary. Both struct and
memory-object lowering prune unreachable definitions before resolving value
substitutions.

Further output optimizations can remove unused fields and return components
where all callers allow it, or simplify scalar phis exposed by lowering. Those
are separate transforms with their own profitability constraints.
Early flattening can expose scalar optimizations but also enlarge IR and extend
live ranges; materializing aggregates in memory adds aliasing and memory costs.
Neither should be chosen without checking generated code.

Compare runtime gas first, then gas- and size-mode bytecode, with compile time
as a tie-breaker. Check the same successful corpus cases and serialized bytecode,
not only total size. CFG maintenance changes must retain behavior; fewer repair
scans alone are not evidence of better generated code. Debug metadata must stay
bytecode-neutral throughout these rewrites.
