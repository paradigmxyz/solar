# ISLE cleanup and canonicalization audit

This is the pinned PR 1514 review. See the [PR 1523 implementation report](ISLE_RULE_IMPLEMENTATION.md) for current changes, coverage, measurements, and remaining limits.

This extends the [rule audit](ISLE_RULE_AUDIT.md) at the same PR1514 head,
`ac257d1a0955f139747b627300b37ec6cb481670`. All reference compiler pins remain
unchanged. It covers cleanup of existing rules, canonical representation,
equal-cost rewrites that enable another fold, and the pass boundaries involved.
No compiler rules were changed or deleted.

The main findings are a type-mismatched boolean-select rewrite, 14 redundant
constant-left spellings under e-graph normalization, one duplicated sign-extension
rewrite, and missing typed boolean/select cleanup before arithmetic selection.
Some cleanups save no operations themselves but expose existing rules. Others
already happen later: inclusive constant comparisons are an important example.

The new [fixture](../tests/ui/codegen/mir/egraph/canonicalization_opportunities.sol)
contains **40 functions and 78 runtime assertions**. All four standard revisions
passed on the working checkout. The retained PR-head binary compiled the same
sources under gas and size objectives; optimized observations appear below.
Runtime execution of this second fixture used the working-checkout compiler,
not a newly built PR harness. No benchmark or new full-width proof run was made
for this extension. Source inspection, runtime boundary checks and optimized
output observations are distinct evidence.

## Existing normal forms and their limits

The e-graph has two different normalization policies. Conflating them either
adds redundant rules or removes needed operand permutations.

| Layer | Established policy | Consequence |
|---|---|---|
| E-graph matcher and emitted nodes | `canonical_operands` puts a lone constant on the right of a schema-declared commutative pair. For ordered comparisons it swaps LT/GT or SLT/SGT while swapping operands. It normalizes roots **and nested definition views**. Two constants or two nonconstants retain order. | Most constant-left e-graph patterns are redundant. Nonconstant permutations are not. ADDMOD/MULMOD normalize their declared pair, not the modulus. |
| E-graph value-numbering keys | `canonical` orders commutative operands by identity and maps GT to reversed LT, likewise signed comparisons. | Expressions can share a key without imposing that order on emitted stack operands or matcher inputs. This is already implemented. |
| Word-sequence matching | Root and earlier pure-segment definitions are exposed without the e-graph normalization contract. A recipe must strictly improve target cost; ties keep the first best. | Do not delete its constant-left variants merely because the default pipeline ran egraph earlier. Custom pipelines are supported. |
| Stack selection | Earlier block definitions are exposed in their existing order; resident lookup canonicalizes equality keys. Actual operand preparation and residual layouts decide cost. | Equality of keys is not a blanket matcher or emitted-order invariant. |
| Branches and checks | Rust strips typed boolean zero tests and swaps successors or check polarity. EVM peepholes remove further truth-only tests. | Branch/check polarity cleanup already exists; the analogous Select cleanup is a separate gap. Do not drop type restrictions because another compiler permits word-valued branches. |
| Casts and copies | Existing rules collapse many zext/sext/trunc, bitcast, pointer/int and equality-of-extension chains; Rust resolves copies, trivial/self phis and CFG edges. | “Add cast cleanup” is too broad. The remaining gaps are particular typed consumers and combinations. |

Sources: [egraph.rs:1245](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/src/mir/transform/egraph.rs#L1245),
[egraph adapter](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/src/mir/transform/egraph/isle.rs),
[word-sequence adapter](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/src/mir/transform/word_sequence/isle.rs),
[stack adapter](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/src/backend/evm/codegen/planning/isle.rs).

## Concrete cleanup findings in the existing rule set

### D01: boolean Select replacement has the wrong immediate type

The [rule at egraph.isle:338](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/isle/mir/egraph.isle#L338)
intends to turn `select(c,false,true)` into `eq(c,false)` for `c:i1`, but builds
`Op.Eq c (imm (u256 0))`. The adapter's `imm` allocates an `Immediate::I256`;
`bool_value` requires `i1`. The e-graph's scalar type check rejects the candidate.

This is a **confirmed missed optimization**, not an executable miscompile.
The existing `scalar_simplification.mir::simplify_booleans` even checks that the
Select survives. Running that fixture through the pinned PR binary reproduced:

```text
v1 = eq arg0, false
v4 = select arg0, false, true
ret arg0, v1, arg1, arg0, v4
```

Use `imm_bool false` for the typed-bool replacement and update that focused
expectation when implementing it. A word-valued Select with 0/1 arms needs a
separate typed result conversion; changing the immediate alone does not cover
that case. Keep `scalar_types_match` and the result-type check.

References: [is_bool_value:228](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/src/mir/transform/egraph/isle.rs#L228),
[imm/imm_bool:356](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/src/mir/transform/egraph/isle.rs#L356),
[type rejection:573](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/crates/codegen/src/mir/transform/egraph.rs#L573),
[existing fixture:34](https://github.com/paradigmxyz/solar/blob/ac257d1a0955f139747b627300b37ec6cb481670/tests/ui/codegen/mir/egraph/scalar_simplification.mir#L34).

### D02: fourteen constant-left spellings have no unique dynamic match

All locations below refer to `crates/codegen/isle/` at the pinned PR head.
These are removal candidates, not edits made by this audit. Both-immediate
shapes can still match syntactically, so verify constant-fold coverage before
removing them. The claim is about unique dynamic matches under the established
normalization contract, not arbitrary ISLE consumers.

| Redundant location | Shape normalized before matching | Surviving location |
|---|---|---|
| `mir/word.isle:25,27` | Nested `add(1,x)` becomes `add(x,1)`. | 24,26 |
| `mir/word.isle:35,37` | Nested `add(MAX,x)` becomes `add(x,MAX)`. | 34,36 |
| `mir/word.isle:89` | Root `mul(MAX,x)` becomes `mul(x,MAX)`. | 88 |
| `mir/word.isle:159` | Root `and(mask,shr(...))` becomes constant-right. | 155 |
| `mir/word.isle:189` | Root `eq(C,shl(...))` becomes constant-right. | 182 |
| `mir/word.isle:210` | `lt(C,shl(...))` becomes `gt(shl(...),C)`. | 224 |
| `mir/word.isle:231` | `gt(C,shl(...))` becomes `lt(shl(...),C)`. | 203 |
| `mir/word.isle:246,252,261` | Nested `and(mask,x)` becomes constant-right. | 243,249,258 |
| `mir/egraph.isle:705` | `lt(0,x)` becomes `gt(x,0)`. | 704 |
| `mir/egraph.isle:714` | `ne(0,bool)` becomes `ne(bool,0)`. | 713 |

Do **not** remove all constant-left or commuted clauses mechanically.
`mulmod(0,5,n)` has two immediate multiplicands and a dynamic modulus: their
order stays intact, and full constant folding cannot remove the instruction.
Both zero-multiplicand rules remain useful. Nonconstant mixed-bitwise spellings
also remain necessary. The word-sequence and stack adapters have different
contracts, so the same deletion is not justified there.

Existing regression coverage: `egraph/identities.mir` constants-right and
constant-multiplicand cases; `bit_slices.mir` nested AND/OR orientations;
`egraph.mir` reversed comparison/commutative GVN. The [complete inventory](ISLE_RULE_INVENTORY.md)
links every cited rule to its source.

### D03: repeated SIGNEXTEND is declared twice in one rule set

`mir/word.isle:238` rewrites `signextend(i,signextend(i,x))` into the inner
operation's spelling. `mir/egraph.isle:492` already simplifies the same shape
to the existing inner value. Both files compile into egraph. Prefer the latter,
which preserves the existing correctly typed value without adding an alternative.
`bit_slices.mir::repeated_dynamic_extension` is the focused coverage.

This does not justify deleting every rule derivable through several others.
The search is bounded: a direct carry/borrow shortcut may reach a result that
a longer chain misses. Check search depth and operand-view coverage, not just
algebraic redundancy.

## Missing cleanup and useful canonicalization

`Observed` means the named source was compiled on the PR head and the shape
survived at the stated layer. `Proposed` means source-backed or derived follow-on
work with a crafted example, not an executed improvement. All formulas retain
the main audit's wrapping EVM semantics and typed MIR restrictions. `!b` below
is logical negation of a proven boolean, not word NOT.

| ID | Cleanup / chosen direction | Evidence, consumer and limits |
|---|---|---|
| C01 | `eq(zext(b:i1),1)=>b`; `ne(zext(b),1)=>eq(b,false)`. | **Observed** `boolExtend` retains NE, ZEXT, EQ1 in both objectives; final EVM retains `ISZERO ISZERO PUSH1 1 EQ`. Sonatina implements all four zext-i1 comparison cases; we already cover comparisons to zero. Keep the replacement i1. |
| C02 | Boolean selects: `select(c,b,false)=>c&b`; `select(c,true,b)=>c\|b`; the other two forms use `!c`. | **Observed** `selectTrue` becomes SUB/MUL/ADD/NE0 under gas; `selectFalse` becomes MUL/NE0. Require both the condition and the arms to be canonical booleans, with compatible types, before general word arithmetic recipes hide the shape. A nominal Solidity bool or arbitrary nonzero word is not sufficient proof after lowering. |
| C03 | `select(eq(c,false),a,b)=>select(c,b,a)`; strip redundant typed nonzero tests from Select. | **Observed** `invertSelect` reaches arithmetic with an inverted predicate. LLVM absorbs NOT into Select with profitability restrictions; Cranelift also normalizes Select predicates. Reuse a condition; do not create an inversion merely to put zero arms on one side. For word conditions, retain legal typed truth conversion. |
| C04 | Compare constant Select arms directly: `eq(select(c,A,B),A)=>c` and the three EQ/NE/other-arm variants, `A!=B`, `c:i1`. | **Observed** `selectTag` retains arithmetic/tag comparison under gas. Cranelift has all four variants. Noncanonical word conditions require a zero/nonzero predicate, not returning the original word. Simplify before arithmetic select lowering. |
| C05 | `select(ne(x,y),x,y)=>x`; `select(eq(x,y),0,x-y)=>x-y`; equality-based arm substitution. | **Observed** `selectDistinct` retains NE/SUB/MUL/ADD. Cranelift supplies these families; our EQ arm-equality sibling already exists. The subtraction form is a crafted follow-on `unchecked { return a==b ? 0 : a-b; }`. Preserve checked-source effects. |
| C06 | Pull a shared pure operation out of both Select arms: `select(c,~x,~y)=>~select(c,x,y)`, negation and compatible casts similarly. | **Proposed**, Cranelift exact patterns and LLVM cast/select guidance. Crafted `return c ? ~x : ~y;`. Require matching types and a useful reduction of unshared producers; never move loads, calls or checked side effects by analogy. Orient contraction one way. |
| C07 | Push an operation into **constant** Select arms when both results fold: `(c?A:B)+K => c?(A+K):(B+K)`. | **Proposed**, Cranelift ADD-select folding. Crafted `unchecked { return (c?5:8)+3; }`. Exposes equal/zero/one arms, but may enlarge pushes or lose sharing. Do not distribute arbitrary operations over nonconstant arms. |
| C08 | Keep boolean combinations below a common zext, or fuse the truth-test consumer: `ne(and(zext(a),zext(b)),0)=>and(a,b)` for i1 a,b; OR/XOR duals. | **Observed context** `boolOr`, `boolAnd`, `bothNonzero`, `eitherNonzero` retain widened intermediates. Cranelift moves bitwise operations below extensions; Sonatina's typed comparison normalization motivates the consumer form. A fused consumer rule avoids otherwise-neutral cast-motion cycles. |
| C09 | `or(ne(x,0),ne(y,0))=>ne(or(x,y),0)`; combine comparisons with matching operands/signedness by their truth sets. | **Observed** `eitherNonzero` retains two normalized words plus OR. **Proposed** `lt(x,y)\|eq(x,y)=>!gt(x,y)`, `lt& gt=>false`, `lt xor gt=>ne`. LLVM/Cranelift have boolean comparison composition. Do not replace AND of nonzero tests with nonzero of word AND: x=1,y=2 disproves it. Preserve short-circuit effects before treating source logic as eager bitwise logic. |
| C10 | General typed extension zero tests, and equal-width ordered comparisons before zext/sext. | **Observed mixed result** `extendedOrder` already loses its extensions in final MIR; `signedExtendedOrder` retains two SIGNEXTENDs. That is not permission to compare raw signed-byte words with SLT. Typed narrow signed comparisons must lower correctly; dirty upper bits prevent `SIGNEXTEND(0,x)==0 => x==0`. Cranelift provides source-width rules. |
| C11 | Inclusive constant bounds to strict target predicates: `!(x<C)=>x>C-1` if unsigned C>0; `!(x>C)=>x<C+1` if C<MAX; signed analogues avoid signed endpoints. | **Already late for several probes:** `upperBound`, `lowerBound`, `signedLower` still show inverted compares in MIR but finish as one strict EVM comparison. `signedUpper` still has SGT/ISZERO after negative constant materialization. Earlier normalization could enable matching or cover that hole, not a blanket new backend optimization. Shared users and push costs still matter. |
| C12 | `~x < ~y => x > y`, signed and unsigned, matching widths; move NOT/XOR constants across EQ/NE. | **Observed** `complementOrder`, `signedComplementOrder`, `invertedEqual`, `xorEqual`. Complement reverses both orders; `eq(x^A,B)=>eq(x,A^B)` and `eq(~x,C)=>eq(x,~C)`. The new immediate may cost more. Prefer stripping matching complements or a consumer that removes an operation. |
| C13 | Canonicalize a costly modular negative ADD to a cheaper positive SUB **by target cost**, not a universal SUB-to-ADD rule. | **Observed** `constantSub` is `sub(x,7)` while `wrappedAdd` stays `add(x,MAX-6)`; final latter uses `PUSH 6; NOT; ...; ADD`. Test both source forms for convergence and shared constants. LLVM's algebraic preference does not price EVM materialization. |
| C14 | Low-byte consumers: `and(byte(31,x),m)=>and(x,m&255)` for constant m. | **Observed control** `byteMask` remains BYTE31, which is not itself a problem. **Proposed** field-nibble case `and(byte(31,word),15)`. solc has BYTE31→AND255; consumer fusion avoids adding a neutral BYTE↔mask cycle. Existing range facts may already fold impossible byte comparisons. |
| C15 | Repeated complements into one NOT: `(flags&~a)&~b=>flags&~(a\|b)`; OR dual. | **Proposed**, LLVM one-use De Morgan reassociation. Crafted repeated field clearing `return flags & ~clear1 & ~clear2;`. Ordinary adjacent De Morgan is already present; the intervening third value is the missing context. Price retained producers. |
| C16 | Consumer-driven NOT/constant or logic/add normalization. Examples `~(x+7)+7=>~x`; `(x+256)\|3 => (x\|3)+256`. | **Proposed**, Cranelift NOT-to-leaf and LLVM logic-before-add. The first contracts a full consumer; the second merely exposes later masks/CSE and needs a precise carry condition. Never substitute a vague “disjoint constants” guard or force negative PUSH32 constants. |
| C17 | Saturating sign-test cleanup: `(sar(255,x)==0)=>!slt(x,0)`; SAR overshift canonicalization from R20. | **Observed** `zeroSign` retains SAR/EQ; `highBit` already returns SHR255 directly. Prefer a useful truth-test consumer, with signed interpretation explicit. This extends main-audit sign families rather than duplicating their count. |
| C18 | Normalize unused **memory-start** operands when corresponding size is zero: empty CALL input/output pointers, LOG data start, REVERT start. | **Observed** `emptyCall` retains both dynamic pointers; `emptyLog` and `emptyRevert` retain theirs. solc implements memory-region-driven start normalization. Retain call/log/revert and pointer-producing effects; only the unused operand changes. This is effect-preserving in-place cleanup, not pure CSE. Price PUSH0/duplicates/spills. |

C01/C02/C04/C05 are the first new cleanup batch to implement. D01 should be
fixed before adding another workaround for that exact typed Select. C06–C10
need typed early-MIR examples as well as lowered word tests. C11/C13/C14 require
inspection of final scheduled code; a cleaner MIR spelling is not enough.
C15/C16 remain bounded, consumer-driven follow-ons.

### Zero-size exceptions and existing work

`emptyHash(pointer)` already folds to the empty Keccak digest under both
objectives, via `constant_hash`; its cost gate is deliberate. `emptyCopy` already
removes zero-length calldata copy. External zero-byte return already becomes
STOP where legal. These are existing cleanup, not new rules.

`emptyReturnCopy(0)` succeeds, but `emptyReturnCopy(1)` must fail when return
data is empty, even though the requested length is zero. The new runtime fixture
checks both. Do not remove RETURNDATACOPY or canonicalize its **source** offset
as though that offset were unused memory. Similarly, replacing empty LOG with
nothing would drop an event, and replacing REVERT with STOP would change success.

## Reference compiler checks

This follow-up inspected drivers as well as rewrite lists. `CL` paths below
are under the pinned wasmtime `cranelift/codegen/src/opts/`; LLVM paths are under
`llvm/lib/`. All versions match the [main source table](ISLE_RULE_AUDIT.md#sources-and-scope).

| Compiler | Additional inspected source and lesson |
|---|---|
| LLVM | `InstCombineSelect.cpp:5151` absorbs NOT with a profitability veto; `:2838` contracts suitable casts. `InstCombineCasts.cpp:244` explains target-dependent cast/select direction. `InstCombineCompares.cpp:7424,7470` makes inversion use-sensitive and canonicalizes i1 comparisons. `InstCombineAndOrXor.cpp:1728,2228` handles De Morgan reassociation and guarded logic-before-add. `Scalar/Reassociate.cpp:209–303,467–550,2558–2720` uses ranks and shared-DAG boundaries; this is a Rust-pass model, not permission for unrestricted ISLE reassociation. |
| Cranelift | `selects.isle:7–12,79–109,122–151,191–219` covers condition orientation, shared-producer contraction, constant-arm folding and equality assumptions. `icmp.isle:179–186,309–327,383–416` composes comparisons and folds constant-select comparisons. `extends.isle:39–46,71–80,105–109` supplies typed zero tests, bitwise contraction and ordered comparisons. `cprop.isle:518–521` has NOT-to-leaf normalization; its negative constants still need EVM costing. |
| solx / solx-LLVM | `solx-codegen-evm/src/optimizer/mod.rs:38` invokes LLVM's default optimization pipeline. The fork contains the same broad cast/select/logic mechanisms; official LLVM source was checked separately. This establishes relevant source machinery, not that every pattern fires in a deployed Solidity workload. Arithmetic zero-divisor and signed-overflow guards remain essential. |
| solc | `libyul/optimiser/ExpressionSimplifier.cpp:42–81` repeatedly applies scope-safe rules and zeros unused memory starts without dropping effectful call expressions. `StructuralSimplifier.cpp:93–122` folds constant control flow; `ControlFlowSimplifier.cpp:134–210` retains evaluation when removing empty control structures. `RuleList.h:187` has low-byte canonicalization. Equivalent CFG/evaluation cleanup belongs in our Rust passes. |
| Venom | `algebraic_optimization.py:86,158,350,357` distinguishes truth-only uses, literal placement, final orientation and comparator boundaries. `analysis/available_expression.py:90–106` makes commutative equality independent of operand order. `assign_elimination.py:17–46` and `phi_elimination.py:12–111` handle copy/phi roots; `branch_optimization.py` uses liveness for inversion. These are not missing scalar rules to copy wholesale. |
| Sonatina | `optim/simplify_expr.rs:131–161` handles four zext-i1 compare cases; `:1014–1044,1397–1466` normalizes typed results and casts. `optim/branch_canonicalize.rs:57–120,194` handles polarity with target costs. `optim/gvn.rs:893–946` canonicalizes congruence leaders, commutative ranks and phi ordering. Its broad lesson is to preserve typed facts through cleanup. |
| plank | `copy_propagation.rs:14–49` substitutes block-local copies; `switch_peephole.rs:8–40` lowers a one-zero-case switch to a branch. These correspond to existing copy/CFG/lowering work here, not dozens of missing ISLE expressions. No new sound nonzero-environment assumption was found to import. |

## Convergence and implementation boundaries

The e-graph lets the most recently added equal-cost node win. Its worklist and
operand views are bounded, so adding inverse equal-cost rules can change the
winner, consume useful search slots, or change repeated-pass behavior. A bound
limits work; it is not a canonical-form proof. No current oscillation was
established by this audit.

Adopt three explicit categories:

1. **Representation normalization:** one declared direction, such as current
   constants-right matching or hash-key ordering. It need not dictate physical
   stack order.
2. **Contraction:** remove a redundant operation/cast or fold a consumer. Prefer
   C01/C04/C05 and the fused form of C08 over neutral intermediates.
3. **Priced alternative:** preserve the original form when another equivalent
   shape may have better push, stack, sharing or lifetime cost. Neutral NOT,
   negative-constant, BYTE/mask and Select/arith rewrites belong here unless a
   visible consumer proves a contraction.

Do not introduce both directions of SUB↔ADD-negative, BYTE↔mask/shift,
Select↔arithmetic, or cast-through-Select without a strict measure or mutually
exclusive applicability. General associative-region normalization belongs in
a separate Rust analysis over bounded pure regions with shared-use boundaries.
LLVM's ranks are a useful algorithm reference, not an EVM profitability model.

Effectful `rewrite_in_place` takes the first type-valid alternative within a
bound; it does not use the pure e-graph's cost extraction. New empty-memory
cleanup there must be directed, type-safe and effect-preserving. Do not add a
reverse rule or treat a memory operation as pure merely because one region is
empty.

For implementation checks, compare one and two runs of the affected pass on
focused fixtures, both operand spellings, shared/cross-block uses, source and
result widths, and interaction with later word selection. Fixed-point checks
are proposed acceptance tests; this review did not run a whole-pipeline
idempotence experiment. Re-run full-width proof where supported, typed MIR
checks, runtime edge cases and then real gas/size benches.

## Unsafe cleanup transfers

* Cranelift's `sext(icmp)=>zext(icmp)` assumes a wider comparison result. For our
  `i1`, sext(true) is all ones and zext(true) is 1. The simplification is false.
* Typed NOT of i1 is logical negation. EVM NOT of word 1 is MAX-1, not false.
* Narrow signed comparisons cannot become raw word SLT without appropriate
  sign extension. Dirty high bits also invalidate full-word zero-test removal.
* Moving an operation across Select cannot add eager evaluation of a source
  call, panic or load. Apply pure identities to already available MIR operands.
* A word range of 0..1 needs a sound fact. Source syntax, a false sign-bit query,
  or the shape of an unrelated comparison does not supply it.
* A prettier constant does not imply a cheaper push sequence. Account for NOT
  materialization, fork PUSH0 support, resident constants and retained producers.
* Logical AND of nonzero words is not nonzero of their bitwise AND. The new
  `bothNonzero(1,2)` assertion protects this distinction.
* Invert a predicate only where users can invert for free, or price the boolean
  materialization. EVM has no native LE/GE, and EQ is already one opcode; LLVM's
  blanket boolean EQ→NOT(XOR) convention can add work.

## Validation

The new fixture passed 78 assertions in each applicable standard runtime
revision through the existing UI harness; all four standard revisions passed,
including the MIR snapshot. The snapshot is for the working checkout. PR-head
MIR for all 40 functions and final EVM output for eight representative isolated
functions were inspected separately. D01 was reproduced with the PR's existing
typed-MIR fixture. No optimizer candidate was enabled, and no performance result
is claimed. `cargo fmt --all -- --check` and default-feature
`cargo clippy --workspace --all-targets` passed; formatting reported the existing
nightly-only configuration warnings.

## Retained PR-head output

These are complete function bodies from the retained PR-head binary, excluding
module headers and the separate dispatcher. They preserve ABI guards, types,
casts and effects. Identical gas/size bodies appear once; differing bodies appear
separately. These observations show the current forms, not the result of any
proposed rule.

Reproduce the PR observations with the pinned compiler and the UI source:

```sh
solar-pr1514 tests/ui/codegen/mir/egraph/canonicalization_opportunities.sol -O gas -Zdump=mir
solar-pr1514 tests/ui/codegen/mir/egraph/canonicalization_opportunities.sol -O size -Zdump=mir
solar-pr1514 tests/ui/codegen/mir/egraph/scalar_simplification.mir -Zmir-pipeline=egraph -Zpass-diff
```

Run the fixture through the existing UI harness after building it:

```sh
ISLE_AUDIT_UI_BINARY=$(cargo nextest list --package=solar-compiler --test=tests --message-format json | jq -r '."rust-suites"."solar-compiler::tests"."binary-path"')
TESTER_MODE=ui "$ISLE_AUDIT_UI_BINARY" canonicalization_opportunities
```

### `upperBound`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = gt arg0, 999
    v1 = eq v0, false
    v2 = zext i1 v1 to i256
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `lowerBound`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = lt arg0, 0x3e8
    v1 = eq v0, false
    v2 = zext i1 v1 to i256
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `signedUpper`

Gas and size:

```text
  bb0:
    v4 = calldatasize
    v5 = lt v4, 36
    jumpi v5, bb1, bb2
  bb2:
    v1 = sgt arg0, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
    v2 = eq v1, false
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `signedLower`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = slt arg0, 2
    v1 = eq v0, false
    v2 = zext i1 v1 to i256
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `complementOrder`

Gas and size:

```text
  bb0:
    v4 = calldatasize
    v5 = lt v4, 68
    jumpi v5, bb1, bb2
  bb2:
    v0 = not arg0
    v1 = not arg1
    v2 = lt v0, v1
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `signedComplementOrder`

Gas and size:

```text
  bb0:
    v4 = calldatasize
    v5 = lt v4, 68
    jumpi v5, bb1, bb2
  bb2:
    v0 = not arg0
    v1 = not arg1
    v2 = slt v0, v1
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `invertSelect`

Gas and size:

```text
  bb0:
    v8 = calldatasize
    v9 = lt v8, 100
    jumpi v9, bb1, bb2
  bb2:
    v10 = calldataload 4
    v11 = lt v10, 2
    jumpi v11, bb3, bb1
  bb3:
    v4 = sub arg1, arg2
    v0 = eq arg0, 0
    v5 = zext i1 v0 to i256
    v6 = mul v5, v4
    v7 = add arg2, v6
    mstore 128, v7
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `selectTrue`

Gas and size:

```text
  bb0:
    v7 = calldatasize
    v8 = lt v7, 68
    jumpi v8, bb1, bb2
  bb2:
    v9 = calldataload 4
    v10 = lt v9, 2
    jumpi v10, bb3, bb1
  bb3:
    v11 = calldataload 36
    v12 = lt v11, 2
    jumpi v12, bb4, bb1
  bb4:
    v3 = sub arg1, 1
    v5 = mul arg0, v3
    v6 = add v5, 1
    v14 = ne v6, 0
    v15 = zext i1 v14 to i256
    mstore 128, v15
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `selectFalse`

Gas and size:

```text
  bb0:
    v5 = calldatasize
    v6 = lt v5, 68
    jumpi v6, bb1, bb2
  bb2:
    v7 = calldataload 4
    v8 = lt v7, 2
    jumpi v8, bb3, bb1
  bb3:
    v9 = calldataload 36
    v10 = lt v9, 2
    jumpi v10, bb4, bb1
  bb4:
    v4 = mul arg0, arg1
    v12 = ne v4, 0
    v13 = zext i1 v12 to i256
    mstore 128, v13
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `bothNonzero`

Gas and size:

```text
  bb0:
    v9 = calldatasize
    v10 = lt v9, 68
    jumpi v10, bb1, bb2
  bb2:
    v2 = ne arg1, 0
    v3 = zext i1 v2 to i256
    v6 = ne arg0, 0
    v7 = zext i1 v6 to i256
    v8 = and v7, v3
    mstore 128, v8
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `eitherNonzero`

Gas and size:

```text
  bb0:
    v9 = calldatasize
    v10 = lt v9, 68
    jumpi v10, bb1, bb2
  bb2:
    v2 = ne arg1, 0
    v3 = zext i1 v2 to i256
    v6 = ne arg0, 0
    v7 = zext i1 v6 to i256
    v8 = or v7, v3
    mstore 128, v8
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `byteMask`

Gas and size:

```text
  bb0:
    v1 = calldatasize
    v2 = lt v1, 36
    jumpi v2, bb1, bb2
  bb2:
    v0 = byte 31, arg0
    mstore 128, v0
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `booleanBit`

Gas and size:

```text
  bb0:
    v1 = calldatasize
    v2 = lt v1, 36
    jumpi v2, bb1, bb2
  bb2:
    v3 = calldataload 4
    v4 = lt v3, 2
    jumpi v4, bb3, bb1
  bb3:
    mstore 128, arg0
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `boolExtend`

Gas and size:

```text
  bb0:
    v6 = calldatasize
    v7 = lt v6, 36
    jumpi v7, bb1, bb2
  bb2:
    v2 = ne arg0, 0
    v3 = zext i1 v2 to i256
    v4 = eq v3, 1
    v5 = zext i1 v4 to i256
    mstore 128, v5
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `extendedOrder`

Gas and size:

```text
  bb0:
    v4 = calldatasize
    v5 = lt v4, 68
    jumpi v5, bb1, bb2
  bb2:
    v6 = calldataload 4
    v7 = shr 8, v6
    v8 = eq v7, 0
    jumpi v8, bb3, bb1
  bb3:
    v9 = calldataload 36
    v10 = shr 8, v9
    v11 = eq v10, 0
    jumpi v11, bb4, bb1
  bb4:
    v2 = lt arg0, arg1
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `signedExtendedOrder`

Gas and size:

```text
  bb0:
    v4 = calldatasize
    v5 = lt v4, 68
    jumpi v5, bb1, bb2
  bb2:
    v6 = calldataload 4
    v7 = signextend 0, v6
    v8 = eq v6, v7
    jumpi v8, bb3, bb1
  bb3:
    v9 = calldataload 36
    v10 = signextend 0, v9
    v11 = eq v9, v10
    jumpi v11, bb4, bb1
  bb4:
    v0 = signextend 0, arg0
    v1 = signextend 0, arg1
    v2 = slt v0, v1
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `narrowAfterNot`

Gas and size:

```text
  bb0:
    v2 = calldatasize
    v3 = lt v2, 36
    jumpi v3, bb1, bb2
  bb2:
    v0 = not arg0
    v1 = and v0, 255
    mstore 128, v1
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `constantSub`

Gas and size:

```text
  bb0:
    v1 = calldatasize
    v2 = lt v1, 36
    jumpi v2, bb1, bb2
  bb2:
    v0 = sub arg0, 7
    mstore 128, v0
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `wrappedAdd`

Gas and size:

```text
  bb0:
    v2 = calldatasize
    v3 = lt v2, 36
    jumpi v3, bb1, bb2
  bb2:
    v1 = add arg0, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9
    mstore 128, v1
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `complementXor`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 68
    jumpi v4, bb1, bb2
  bb2:
    v0 = not arg0
    v1 = xor v0, arg1
    v2 = not v1
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `invertedEqual`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = not arg0
    v1 = eq v0, 0x1234
    v2 = zext i1 v1 to i256
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `xorEqual`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = xor arg0, 0x1234
    v1 = eq v0, 0x1334
    v2 = zext i1 v1 to i256
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `highBit`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = shr 255, arg0
    mstore 128, v0
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `zeroSign`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = sar 255, arg0
    v1 = eq v0, 0
    v2 = zext i1 v1 to i256
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `boolOr`

Gas and size:

```text
  bb0:
    v7 = calldatasize
    v8 = lt v7, 68
    jumpi v8, bb1, bb2
  bb2:
    v9 = calldataload 4
    v10 = lt v9, 2
    jumpi v10, bb3, bb1
  bb3:
    v11 = calldataload 36
    v12 = lt v11, 2
    jumpi v12, bb4, bb1
  bb4:
    v0 = eq arg1, 0
    v1 = zext i1 v0 to i256
    v2 = eq arg0, 0
    v3 = zext i1 v2 to i256
    v4 = and v3, v1
    v5 = eq v4, 0
    v6 = zext i1 v5 to i256
    mstore 128, v6
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `boolAnd`

Gas and size:

```text
  bb0:
    v7 = calldatasize
    v8 = lt v7, 68
    jumpi v8, bb1, bb2
  bb2:
    v9 = calldataload 4
    v10 = lt v9, 2
    jumpi v10, bb3, bb1
  bb3:
    v11 = calldataload 36
    v12 = lt v11, 2
    jumpi v12, bb4, bb1
  bb4:
    v0 = eq arg1, 0
    v1 = zext i1 v0 to i256
    v2 = eq arg0, 0
    v3 = zext i1 v2 to i256
    v4 = or v3, v1
    v5 = eq v4, 0
    v6 = zext i1 v5 to i256
    mstore 128, v6
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `zeroSelect`

Gas and size:

```text
  bb0:
    v8 = calldatasize
    v9 = lt v8, 100
    jumpi v9, bb1, bb2
  bb2:
    v10 = calldataload 4
    v11 = lt v10, 2
    jumpi v11, bb3, bb1
  bb3:
    v4 = sub arg1, arg2
    v6 = mul arg0, v4
    v7 = add arg2, v6
    v2 = eq v7, 0
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `compareSelect`

Gas and size:

```text
  bb0:
    v8 = calldatasize
    v9 = lt v8, 132
    jumpi v9, bb1, bb2
  bb2:
    v10 = calldataload 4
    v11 = lt v10, 2
    jumpi v11, bb3, bb1
  bb3:
    v4 = sub arg1, arg2
    v6 = mul arg0, v4
    v7 = add arg2, v6
    v2 = eq v7, arg3
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `subtractCompare`

Gas and size:

```text
  bb0:
    v4 = calldatasize
    v5 = lt v4, 68
    jumpi v5, bb1, bb2
  bb2:
    v0 = sub arg0, arg1
    v1 = sub arg1, arg0
    v2 = eq v0, v1
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `nestedMask`

Gas and size:

```text
  bb0:
    v2 = calldatasize
    v3 = lt v2, 36
    jumpi v3, bb1, bb2
  bb2:
    v0 = and arg0, 0xffff
    v1 = or v0, 0x10000
    mstore 128, v1
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `widenedTrue`

Gas and size:

```text
  bb0:
    v6 = calldatasize
    v7 = lt v6, 36
    jumpi v7, bb1, bb2
  bb2:
    v8 = calldataload 4
    v9 = lt v8, 2
    jumpi v9, bb3, bb1
  bb3:
    v2 = eq arg0, 1
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `widenedFalse`

Gas and size:

```text
  bb0:
    v6 = calldatasize
    v7 = lt v6, 36
    jumpi v7, bb1, bb2
  bb2:
    v8 = calldataload 4
    v9 = lt v8, 2
    jumpi v9, bb3, bb1
  bb3:
    v2 = ne arg0, 1
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `selectTag`

Gas and size:

```text
  bb0:
    v7 = calldatasize
    v8 = lt v7, 36
    jumpi v8, bb1, bb2
  bb2:
    v9 = calldataload 4
    v10 = lt v9, 2
    jumpi v10, bb3, bb1
  bb3:
    v5 = mul arg0, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc
    v6 = add v5, 11
    v2 = eq v6, 7
    v3 = zext i1 v2 to i256
    mstore 128, v3
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `selectDistinct`

Gas and size:

```text
  bb0:
    v6 = calldatasize
    v7 = lt v6, 68
    jumpi v7, bb1, bb2
  bb2:
    v2 = sub arg0, arg1
    v0 = ne arg0, arg1
    v3 = zext i1 v0 to i256
    v4 = mul v3, v2
    v5 = add arg1, v4
    mstore 128, v5
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `emptyCall`

Gas and size:

```text
  bb0:
    v3 = calldatasize
    v4 = lt v3, 36
    jumpi v4, bb1, bb2
  bb2:
    v0 = gas
    v1 = staticcall v0, 4, arg0, 0, arg0, 0
    v2 = zext i1 v1 to i256
    mstore 128, v2
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `emptyHash`

Gas:

```text
  bb0:
    v1 = calldatasize
    v2 = lt v1, 36
    jumpi v2, bb1, bb2
  bb2:
    mstore 128, 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
    returndata 128, 32
  bb1:
    revert 0, 0
```

Size:

```text
  bb0:
    v1 = calldatasize
    v2 = lt v1, 36
    jumpi v2, bb1, bb2
  bb2:
    v0 = keccak256 arg0, 0
    mstore 128, v0
    returndata 128, 32
  bb1:
    revert 0, 0
```

### `emptyRevert`

Gas and size:

```text
  bb0:
    v0 = calldatasize
    v1 = lt v0, 36
    jumpi v1, bb1, bb2
  bb2:
    revert arg0, 0
  bb1:
    revert 0, 0
```

### `emptyLog`

Gas and size:

```text
  bb0:
    v0 = calldatasize
    v1 = lt v0, 36
    jumpi v1, bb1, bb2
  bb2:
    log0 arg0, 0
    stop
  bb1:
    revert 0, 0
```

### `emptyCopy`

Gas and size:

```text
  bb0:
    v0 = calldatasize
    v1 = lt v0, 36
    jumpi v1, bb1, bb2
  bb2:
    stop
  bb1:
    revert 0, 0
```

### `emptyReturnCopy`

Gas and size:

```text
  bb0:
    v0 = calldatasize
    v1 = lt v0, 36
    jumpi v1, bb1, bb2
  bb2:
    returndatacopy 0, arg0, 0
    stop
  bb1:
    revert 0, 0
```

## Final EVM spot checks

Each function below was compiled alone in a contract named `Probe`, with the
same source body and signature, using `-O gas -Zdump=evmir`. Isolating functions
removes dispatcher-sharing differences; these checks establish only the shown
remaining opcode shapes. The complete runtime EVM IR follows.

### `upperBound`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0x233c1d6a
  sub
  push bb3
  jumpi
  push 36
  calldatasize
  lt
  push bb3
  jumpi
  push 0x3e8
  push 4
  calldataload
  lt
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```

### `lowerBound`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0x74380a78
  sub
  push bb3
  jumpi
  push 36
  calldatasize
  lt
  push bb3
  jumpi
  push 999
  push 4
  calldataload
  gt
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```

### `signedUpper`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0xd00a7c5c
  sub
  push bb3
  jumpi
  push 36
  calldatasize
  lt
  push bb3
  jumpi
  push 1
  not
  push 4
  calldataload
  sgt
  iszero
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```

### `signedLower`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0xc2b80224
  sub
  push bb3
  jumpi
  push 36
  calldatasize
  lt
  push bb3
  jumpi
  push 1
  push 4
  calldataload
  sgt
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```

### `boolExtend`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0xadfa22a9
  sub
  push bb3
  jumpi
  push 36
  calldatasize
  lt
  push bb3
  jumpi
  push 4
  calldataload
  iszero
  iszero
  push 1
  eq
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```

### `wrappedAdd`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0xdc42c9d0
  sub
  push bb3
  jumpi
  push 36
  calldatasize
  lt
  push bb3
  jumpi
  push 6
  not
  push 4
  calldataload
  add
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```

### `byteMask`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0x7aeb5711
  sub
  push bb3
  jumpi
  push 36
  calldatasize
  lt
  push bb3
  jumpi
  push 4
  calldataload
  push 31
  byte
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```

### `selectTrue`

```text
@module Probe_runtime
bb0:
  callvalue
  push bb3
  jumpi
  push 0
  calldataload
  push 224
  shr
  dup 1
  push 0x43b35dc8
  sub
  push bb3
  jumpi
  push 68
  calldatasize
  lt
  push bb3
  jumpi
  push 4
  calldataload
  push 1
  lt
  push bb3
  jumpi
  push 36
  calldataload
  push 1
  lt
  push bb3
  jumpi
  push 1
  push 36
  calldataload
  sub
  push 4
  calldataload
  mul
  push 1
  add
  iszero
  iszero
  push 128
  mstore
  push 32
  push 128
  return
bb3 [cold]:
  push 0
  push 0
  revert
```
