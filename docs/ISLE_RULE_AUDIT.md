# ISLE rule audit after PR 1514

This is the pinned PR 1514 review. See the [PR 1523 implementation report](ISLE_RULE_IMPLEMENTATION.md) for current changes, coverage, measurements, and remaining limits.

This audit uses [PR 1514](https://github.com/paradigmxyz/solar/pull/1514) at
`ac257d1a0955f139747b627300b37ec6cb481670`, based on
`ce50334150023a4727c018317d39357186bf6b81`. It covers **all 566 explicit rule
clauses in eight handwritten ISLE files**, their Rust guards and consumers,
and the relevant scalar, EVM, and stack transformations in the reference
compilers below. The [clause inventory](ISLE_RULE_INVENTORY.md) lists every
clause, including operand permutations, with its pinned source location.

The [cleanup and canonicalization follow-up](ISLE_RULE_CANONICALIZATION.md)
adds a review of rule redundancy, typed boolean/select cleanup, normal forms,
and pass placement across all seven reference compilers, with 40 further
source examples and 78 runtime assertions.

There are useful gaps. The best initial work is basic constant reassociation,
comparison cancellation, repeated masks, and signed/unsigned bit facts. These
have broader uses than another large batch of two-input arithmetic identities.
The existing rules already cover most basic neutral-element, complement,
common-mask, same-shift, byte-extraction, and conditional-arithmetic families.

This is a review and candidate catalog, not an optimizer implementation or a
performance report. No compiler rules changed. The accompanying
[UI fixture](../tests/ui/codegen/mir/egraph/rule_opportunities.sol) has 50
functions and 82 runtime assertions; all four standard revisions passed on the
PR head. The [evidence appendix](ISLE_RULE_EVIDENCE.md) records the optimized
MIR under both `-Ogas` and `-Osize`, the source examples, and the proof experiment.
Examples retaining a pattern establish a missed MIR simplification; they do not
establish lower final gas or bytecode size for a proposed replacement.

The inventory is exhaustive for that pinned ISLE source. The comparison covers
the named reference files and relevant families, not every architecture-specific
LLVM combine, every possible integer identity, or every future upstream rule.
No clause-hit instrumentation was added: a test that covers a family is not
proof that every permutation fires.

## Sources and scope

All source comparisons use code, not descriptions of compiler capabilities.
The reference links below pin the exact versions inspected.

| Reference | Pin and inspected code | Scope |
|---|---|---|
| LLVM | [llvm-project, `0bd330675f9eb08126e467505a0800f167084473`](https://github.com/llvm/llvm-project/tree/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine) | `InstCombineCompares.cpp`, `InstCombineMulDivRem.cpp`, `InstCombineAddSub.cpp`, and `Analysis/InstructionSimplify.cpp`; integer algebra, compares, division, overflow premises. |
| Cranelift | [wasmtime, `5cde11750fe20a580e8846f080e47d44b3e811ae`](https://github.com/bytecodealliance/wasmtime/tree/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts) | `arithmetic.isle`, `bitops.isle`, `cprop.isle`, `icmp.isle`, `shifts.isle`, and constant-division helpers. |
| solx LLVM | [solx-llvm, `bdad05b2b0bccd5c5083135cdc4afebc409468a3`](https://github.com/NomicFoundation/solx-llvm/tree/bdad05b2b0bccd5c5083135cdc4afebc409468a3/llvm/lib) | Generic LLVM integer simplification in the EVM fork, checked against the official LLVM files above. |
| solx lowering | [solx, `3d4c9c56176296204814dca17d8eb6c6f2e2503a`](https://github.com/NomicFoundation/solx/blob/3d4c9c56176296204814dca17d8eb6c6f2e2503a/solx-codegen-evm/src/codegen/instructions/arithmetic.rs) | How EVM arithmetic enters LLVM, especially zero divisors and signed overflow. |
| solc | [tracked Solidity, `f401782df49be312ea4ef52a2d467cf5183b5906`](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h) | Complete scalar rule list, shifts, masks, reassociation and exponent rules. |
| Vyper Venom | [Vyper, `9b35e492b7e016f5bf9e1f907121391890ac8444`](https://github.com/vyperlang/vyper/tree/9b35e492b7e016f5bf9e1f907121391890ac8444/vyper/venom/passes) | Algebraic optimization plus range/overflow, affine, literal, memory, load/store and assertion passes. |
| Sonatina | [Sonatina, `54492147373b8bf45f381bcef39477f0fb194648`](https://github.com/fe-lang/sonatina/tree/54492147373b8bf45f381bcef39477f0fb194648/crates/codegen/src/optim) | `simplify_expr.rs`, `known_bits_simplify.rs`, facts and loop-strength-reduction boundaries. |
| plank | [plank, `1471137247f9d829bf81e879e8eaba223e648693`](https://github.com/plankevm/plank-monorepo/tree/1471137247f9d829bf81e879e8eaba223e648693/plankc/sir/crates/passes/src/optimizations) | Constant propagation/evaluation, switch peepholes and CFG passes; fewer nonconstant identities than the other references. |

Cranelift and solx use the cached pins above: cache refresh encountered a tag
conflict and a fetch failure respectively. Neither is presented as the latest
upstream revision. The Solidity submodule was not initialized here; its tracked
rule source was read at the gitlink revision through `gh`.

In the catalog, `CL` refers to files under Cranelift's `opts/`; `S` to solc's
`RuleList.h`; `V` to Venom's `algebraic_optimization.py`; `N` to Sonatina's
`simplify_expr.rs`. The source appendix below gives precise anchors. References
identify the origin or close analogue; formulas explicitly adapted to EVM are
our deductions, not claims that upstream uses identical semantics.

## Semantics and acceptance criteria

Unless stated otherwise, `x`, `y`, and `z` are 256-bit words, arithmetic wraps
modulo `2^256`, `MAX=2^256-1`, and `MIN=2^255` denotes the signed minimum's bit
pattern. `A`, `B`, `C`, `k`, and `m` called constants are compile-time values.
`>>` means SHR; `sar` and signed comparisons are explicit. Shifts saturate;
their counts are full words. DIV, SDIV, MOD and SMOD return zero for a zero
divisor. BYTE counts from the most significant byte. Comparisons produce 0/1.

Word equality alone does not authorize a typed MIR rewrite. A comparison has
an `i1` result: replacements for an `i256` operation such as EXP or SDIV need
an explicit zero extension or the sequence builder's checked conversion.
An identity cannot erase a checked-Solidity panic, memory access, call, or
other effect. Work on actual wrapping MIR instructions and retain their checks.

Use `simplify` when the answer is an existing value of the right type. Use an
e-graph alternative for one-operation replacements, and `word-sequence` for
small multi-operation recipes. Facts, demand analysis, code motion, and CFG
algorithms stay in Rust. Gate new opcodes on fork availability. Price constants,
retained producers, input lifetimes and stack preparation through `Target`.
Reassociation must be bounded and directed, not unbounded equality saturation.

## Confirmed source-shaped opportunities

Every function name in this section refers to the new UI fixture. Its actual
PR-head optimized MIR is in the evidence appendix. Both optimization objectives
retain the indicated opportunity unless noted. This is stronger evidence than
finding a missing spelling in ISLE, but still stops before a candidate gas bench.

Priority **A** means a small, broadly useful rule worth implementing first;
**B** means useful but more conditional or less common; **C** means keep only
as a priced alternative or when another simplification exposes a gain. These
are review priorities, not measured rankings. Symmetric operand arrangements
and EQ/NE duals belong to the same family, with separate tests when added.

| ID | Priority | Rule and required guard | Reference | Realistic case / observed fixture |
|---|---|---|---|---|
| R01 | A | `(x*A)*B => x*(A*B)` with wrapping constant product. | CL `cprop.isle:214`; S:451. | Inlined unit scaling, `scale`: two MULs remain for multipliers 3 and 5. Keep original when the combined immediate or shared producer costs more. |
| R02 | A | `(x\|A)\|B => x\|(A\|B)`; `(x^A)^B => x^(A^B)`. | CL `cprop.isle:214–225`; S:451. | Accumulated access flags and domain tags, `accumulatedFlags`, `toggleFlags`: two constant operations remain. ADD/SUB/AND constant combining is already present. |
| R03 | A | `(x+A)==B => x==(B-A)`; `(x-A)==B => x==(B+A)`; NE duals. No no-overflow guard for equality. | CL `cprop.isle:245–260`; LLVM compares. | Sentinel-offset checks, `affineConstant`: ADD then EQ remains. Do not transfer this to ordered comparisons. |
| R04 | A | `(x+z)==(y+z) => x==y`; shared subtraction/negation and NE duals. | CL `icmp.isle:531–535` is a related cancellation; modular bijection supplies the generalization. | Common fee/offset validation, `sameFee`: both ADDs remain. Covers wraparound. |
| R05 | A | `x-(x-y) => y`; `x+(-y)=>x-y`; `x-(-y)=>x+y`; double negation. | CL `arithmetic.isle:23–39`; S additive rules. | Recovering an amount from before/after balances, `nestedDifference`; subtracting a signed delta, `signedDelta`. Current common-base SUB cancellation does not cover all these shapes. |
| R06 | A | `MAX-x => ~x`. | S:148; V `_rule_additive`. | Inverted packed masks, `invert`: SUB and a full-word constant remain. |
| R07 | A | `(x&y)&x => x&y`; `(x\|y)\|x => x\|y`, all permutations, reuse inner value. | CL `bitops.isle:200–210`; S:260. | Reapplying permission filters or setting flags through inlined helpers, `nestedAnd`, `nestedOr`. Cross-operator absorption is already present. |
| R08 | A | `DIV(x,SHL(n,1)) => SHR(n,x)`, SHR available. Valid also for `n>=256`, since both sides are zero. | S:604. | Dynamic page/field width, `page`: SHL plus DIV remains. Counts 255, 256 and MAX execute in the fixture. |
| R09 | B | `EXP(0,n) => zext(n==0)`. | S:759; V `_rule_exp`. | A general exponent helper after specializing its base, `emptyPower`. `0**0` must remain 1. EXP(1,n), EXP(2,n), and exponents 0/1/2 are already covered. |
| R10 | B | `ADDMOD(x,0,m)=>MOD(x,m)`; `MULMOD(x,1,m)=>MOD(x,m)`, with commuted neutral operands. | EVM neutral-element deduction, adjacent to S modular rules. | Generic modular helpers after specialization, `reduceAdd`, `reduceMul`. Preserve Solidity's modulus-zero panic; raw Yul versions return zero. |
| R11 | C | `ADDMOD(x,y,2^k)=>(x+y)&(2^k-1)`; MULMOD analogue, `1<=k<=255`. | S:294/303 has the related power-of-two modulus restriction. | Ring-buffer arithmetic, `ringAdd`, `ringMul` at modulus 256. Wraparound loses only multiples of the modulus. MUL+AND may merely tie native MULMOD; measure, do not assume a gas win. |
| R12 | A | `DIV(DIV(x,A),B)=>DIV(x,A*B)` for positive constants with exact product below `2^256`. Product at/above `2^256` gives zero; a zero divisor gives zero. | LLVM nested division. | Decimal scale normalization, `nestedQuotient`: DIV by 7 followed by 3 remains. Compute the product with overflow detection, never a wrapped divisor. |
| R13 | A | `(DIV(x,C)==0)=>(x<C)` for constant `C!=0`. | LLVM division comparison. | Amount below one unit, `belowUnit`: DIV by 1000 plus EQ remains. `belowBucket` deliberately covers the unsafe dynamic-zero case; `unit=0` returns true. A nonzero range fact can enable the dynamic form. |
| R14 | A | `(x&y)>x=>false`; `(x\|y)<x=>false`; reversed equivalents. Unsigned only. | CL `bitops.isle:762–780`. | Generic bounds helpers after a mask or flag update, `subset`, `superset`: operation plus comparison remains. Test the top bit; signed ordering differs. |
| R15 | A | `(x>>s)>x=>false`, including `s>=256`. Unsigned SHR only. | CL `icmp.isle:488–492`. | Bounds checking after unit conversion, `shiftedOrder`. SAR is not interchangeable. |
| R16 | B | `x==~x=>false`; `x!=~x=>true`. | CL `icmp.isle:478–482`. | Complemented sentinel checks after inlining, `fixedComplement`. No concrete word equals its complement. |
| R17 | A | `~sar(s,~x)=>sar(s,x)`. | CL `bitops.isle:832–833`. | Signed packed-field normalization, `complementSar`: two NOTs remain around SAR. `shiftComplement` alone only commutes NOT and SAR and is not an independent cost win. |
| R18 | A | `sar(s,MAX)=>MAX`. | CL `shifts.isle:328–329`. | A constant negative sign mask shifted by a dynamic field width, `sarOnes`: SAR remains, including the 256-count test. |
| R19 | A | `slt(sar(s,x),0)=>slt(x,0)` and equivalent nonnegative tests, every full-word count. | CL `icmp.isle:537–543`. | Sign classification after fixed-point scaling, `signedNegative`: SAR then SLT remains. |
| R20 | B | `sar(C,x)=>sar(255,x)` for constant `C>=256`. | EVM saturation deduction; compare CL shift rules without its count masking. | Generic overshift after inlining, `highSign`: SAR 300 remains. Mainly a smaller immediate or follow-on canonicalization opportunity. |
| R21 | A | `SIGNEXTEND(b,x)&m => x&m` when constant mask touches only bits below `8*(b+1)`, for `b<31`; larger indices already imply identity. | S:350. | Reinterpreting a packed signed byte as its unsigned bits, `signMask`. SIGNEXTEND then AND 255 remains. |
| R22 | A | `SIGNEXTEND(a,SHR(b,x))=>SAR(b,x)` when `b<=248`, byte aligned, and `8*(a+1)=256-b`. | S:665. | Signed high-byte extraction, `signedTopByte`: SHR 248 plus SIGNEXTEND 0 remains. Require SAR support. |
| R23 | C | `(x<<k)>>k=>x&(MAX>>k)`; `(x>>k)<<k=>x&(MAX<<k)`, constant k; zero for `k>=256`. | CL `shifts.isle:24–45`; S:513/530. | Truncation/alignment, `realign`, `clearLow`. **Deliberate existing omission:** `bit_slices.mir` says masks may be more expensive. Add only a costed alternative, not unconditional canonicalization. |
| R24 | A | `SDIV(x,MIN)=>zext(x==MIN)`. | LLVM signed-minimum divisor. | Signed boundary normalization, `signedMinQuotient`: SDIV with MIN remains. Other negative inputs truncate to zero; MIN/MIN is 1. |
| R25 | B | `(x*C)==(y*C)=>x==y` for odd constant C; `(x*C)==D=>x==(D*inverse(C))` is a further constant form. | CL `arithmetic.isle:249–294` has a restricted exact-divisor form. | Unchecked hash/tag validation, `oddScale` at C=3. Even C loses information and cannot cancel. An inverse may need a costly PUSH32. |
| R26 | A | `(t^x)^(t^y)=>x^y`. | CL `bitops.isle:794–798`. | Removing a common tag from a checksum difference, `tagDifference`: three XORs remain. Shared-producer liveness still matters. |
| R27 | A | `(x&y)&(x\|z)=>x&y`; dual `(x&y)\|(x\|z)=>x\|z`. | CL `bitops.isle:803–821`. | Nested permission composition, `repeatedPermission`. Three operations remain; existing two-variable mined rules do not cover independent y,z. |
| R28 | B | `(x&m)\|~m=>x\|~m`; dual variants. | CL `bitops.isle:58–84`. | Fill bits outside a retained field, `fillOutsideMask`. Constants may have folded away their NOT producer, so constant forms need matching too. |
| R29 | B | `(-x)&1=>x&1`. | CL `bitops.isle:800–801`. | Signed oddness checks, `negatedParity`. Negation remains before the bit test. |
| R30 | A | `0-(x>>255)=>sar(255,x)`. | CL `shifts.isle:98–101`. | Branchless signed arithmetic masks, `signFromBit`. Only count 255 has this identity. |
| R31 | B | `(x<<s)-(y<<s)=>(x-y)<<s`, including `s>=256`. | CL `shifts.isle:315`. | Packed-offset distances, `shiftedDistance`: two SHLs plus SUB remain. Same-shift ADD factoring already exists. |
| R32 | A | `((x<<s)&m)>>s=>x&(m>>s)`, all word counts. | CL `shifts.isle:318–319`, adapted to EVM saturation. | Extract a variable-position field, `extractMasked`: SHL/AND/SHR remains. Covers lost high bits and overshifts without an extra range guard. |
| R33 | B | `SMOD(x,2^k)==0 => (x&(2^k-1))==0`, positive signed divisor, `1<=k<=254`. | LLVM signed-remainder zero test. | Signed fixed-point alignment, `signedAligned`: SMOD 256 plus EQ remains. Only the zero test transfers; the negative remainder itself is not the low-bit mask. |
| R34 | C | `(x\|C)-C=>x&~C`. | CL `arithmetic.isle:224–230`. | Clearing a field set by a prior helper, `removeSetMask`. A full-word complemented mask can erase the apparent benefit. |
| R35 | B | `r*a+r*b=>r*(a+b)`; subtraction analogue, wrapping MIR only. | LLVM distributive factoring. | Combine fee components, `combinedFee`: two MULs plus ADD remain. Sequence recipe, shared inputs and checks priced/preserved. |
| R36 | C | `slt(x,MIN+1)=>eq(x,MIN)`; signed upper edge duals and impossible extremes. | V `_optimize_comparator_instruction`. | Narrow/boundary checks, `minimumOnly`: SLT remains. Boundary equality may tie in cost; prioritize impossible predicates or a useful consumer. |
| R37 | B | `EXP(MAX,n)=>1-2*(n&1)` in wrapping arithmetic. | S:788. | Alternating-sign helper after specialization, `alternatingSign`: EXP remains. Construct with correctly typed values; price parity reuse and exponent's dynamic cost. |
| R38 | C | `DIV(x,x)=>zext(x!=0)`; `MOD(1,x)=>zext(x>1)`. | EVM adaptations of LLVM self-divide and unit-remainder rules. | `normalized` retains DIV. Two ISZEROs can cost more than native DIV: only pursue a useful boolean consumer or reuse. The MOD form includes x=0 correctly; example `assembly { r := mod(1, modulus) }` is a crafted specialization case, not a new executed fixture. |

R01–R38 group related formulas; 38 is not a claim of 38 independently measured
wins. `negate` is an explicit negative control: SDIV by -1 already becomes
`0-x` via `word.isle`. `booleanMask` is another: existing absorption removes
its whole predicate. `highByte` demonstrates the context where sign-extension
facts may help, but replacing it with a longer hand-built sign-mask expression
is not recommended in isolation.

## Further candidates that need a useful context

These were compared against the full rule inventory. The examples below are
crafted expressions that occur naturally after helper inlining or packed-field
lowering. They are not reported as executed UI cases or measured benchmark hits.
Their simpler subfamilies already have executed examples above. Keep them as
follow-on work, not as unconditional rules to add en masse.

| ID | Candidate, guard, and useful context | Source and validation case |
|---|---|---|
| F01 | Expose associative constants: `(x op C) op y => (x op y) op C` for ADD/MUL/AND/OR/XOR. Alone this saves nothing. Use a bounded recipe only when another constant or common subexpression then disappears. | S:462. Inlined scaling `unchecked { return (x*3)*(y*5); }` can become `(x*y)*15`; test shared `x*3` and large products as cost countercases. |
| F02 | Move constants through subtraction: `x-(y+C)=>(x-y)-C`, `(C-x)-y=>C-(x+y)`, `x-(C-y)=>(x+y)-C`. | S:692. Pointer span `sub(add(base,len),add(base,header))` is already common-base cancellation; harder trigger `sub(end,add(start,header))` needs later constant combination to justify motion. |
| F03 | Mixed opposite constant shifts use a shifted mask plus residual shift. For a,b<256, `SHR(b,SHL(a,x)) = shift(a-b,x) & SHR(b,SHL(a,MAX))`; `SHL(b,SHR(a,x)) = shift(b-a,x) & SHL(b,SHR(a,MAX))`. Here `shift(d,x)` is SHL for nonnegative d and SHR by -d otherwise; compute d as a signed mathematical difference. | S:513/530. `and(shr(16,shl(8,word)),255)` can expose one BYTE or a smaller mask. Test shifts 0,248,255,256,MAX separately; no signed-count arithmetic on full U256 inputs. |
| F04 | Move constant masks across SHL/SHR to expose an outer AND: `SHIFT(s,x&m)=>SHIFT(s,x)&SHIFT(s,m)`. May tie alone. | S:547. `and(shr(8,and(word,65535)),255)` can collapse masks or become BYTE. Existing rules only discard a mask when all cleared bits are ignored. |
| F05 | Distribute an outer mask when it annihilates a child: `((x&A)\|y)&B => y&B` if `A&B=0`. General distribution is larger and needs reuse. | S:572. Packed-field update followed by reading a different field, `and(or(and(old,0xff00),fresh),0xff)`. Test fresh values with dirty upper bits. |
| F06 | Move SHL through SIGNEXTEND only when the new extension becomes identity or reusable: `SHL(a,SIGNEXTEND(b,x))=>SIGNEXTEND(b+a/8,SHL(a,x))`, constant `a%8=0`, `a<=256`, `b<=32`, with nonwrapping computation of `b+a/8`. | S:659. `shl(248,signextend(0,x))=>shl(248,x)` is the useful two-to-one case. First prove indices/saturation, then charge any surviving outer extension. |
| F07 | Complement/XOR collapse: `~(~x^y)=>x^y`; `x&~(x^y)=>x&y`; `x&(~x^y)=>x&y`. | CL `bitops.isle:597,647–657`. `and(flags,not(xor(flags,allowed)))` appears in bit-equality masks. Test nonconstant permutations and retained XOR users. |
| F08 | General known-zero/known-one masks: remove OR bits already known one; fold conflicting-bit equality; drop redundant SIGNEXTEND for sign-filled negative values. | N:1074,1468–1499. `eq(or(x,0x100),and(y,0xff))=>false`, `or(or(x,0x100),0x100)=>or(x,0x100)`; a negative byte's phi followed by SIGNEXTEND should use a sound signed fact. R07 handles the simple syntactic form. General facts belong in Rust. |
| F09 | Demand-aware boolean users: a branch on `x\|nonzero_C` is always taken; truth-only consumers can omit double ISZERO. Do not replace a general returned word by 1 or by x. | V `_rule_or`, `_rewrite_iszero_uses`. `assembly { if or(flags,1) { mstore(0,7) } }`; existing CFG and JUMPI peepholes cover parts. Audit surviving users before extending local patterns. |
| F10 | Demand only a low byte/bit slice of a signed value. Avoid constructing unused sign-fill bits, or move demanded bits through a producer. | N `known_bits_simplify.rs:290`. `highByte` is a signed-byte sign test; `signMask` is the profitable existing probe. Whole-user demand propagation belongs in Rust, with local ISLE consumers. |
| F11 | Signed division by a positive `2^k`, `0<=k<=254`, only under a useful fact: nonnegative inputs permit SHR; signed inputs exactly divisible by the divisor permit SAR. Biasing a general negative input is correct but often more expensive than SDIV. | CL `arithmetic.isle:86–111`. `require(x>=0); return x/8;` can use SHR if the signed fact reaches the operation; `x=-3` is the mandatory rejection control. |
| F12 | Cancel multiplication/division only with no-overflow and divisor-nonzero facts; specialize EVM environment operands only to actual specified facts. | LLVM guarded combines; N facts. A `uint128` product divided by a proven nonzero factor may qualify if widened multiplication cannot wrap. Full-word `mul(x,2)/2` must retain lost-bit behavior. Do not add unguarded cancellation. |

## Existing coverage and benchmark relevance

The clause inventory links each file to the following coverage groups. These
are source-verified coverage locations, not inferred test hits. Paths are
relative to the PR head's `tests/ui/codegen/` unless stated otherwise.

| Group | Existing coverage | What it establishes / remaining limit |
|---|---|---|
| Basic scalar identities | `mir/egraph/{egraph,identities,known_bits,word_rules}.mir`, `word_rules_runtime.sol` | Basic arithmetic, bitwise and range examples; does not cover every new family or permutation. |
| Mined and seeded identities | `mir/egraph/{word_candidates,seeded}.mir`, `seeded_runtime.sol`; `mir/word-sequence/{seeded,mined_words}.mir`, `mined_words.sol` | Current mixed arithmetic/bitwise families already have focused shapes and runtime checks. |
| Cancellation and carry | `mir/egraph/{unit_carry,cancellation}.mir`, corresponding `_runtime.sol` | Current carry/borrow and common-base rewrites; R03–R05 extend different roots. |
| Shifts and packed fields | `mir/egraph/{bit_slices,byte_alignment,packed_word_slices,mined_masks,lossless_shifts}.mir`, corresponding runtime files where present | Includes lost-bit, saturation, byte-index and range guards. `bit_slices.mir` deliberately retains costly-mask alternatives. |
| Exponent/fork selection | `mir/egraph/exp2.mir`, modern/legacy snapshots; `lowering/run-call/power_of_two_mul.sol` | Existing power-of-two rules, fork gating and full pipeline; EXP zero/MAX base additions still need focused tests. |
| CLZ | `mir/egraph/clz.mir`, `clz_runtime.sol`; checker bit-scan and guard mutations | Existing CLZ identities and zero=256 semantics. Do not import LLVM poison-on-zero ctlz. |
| E-graph placement/cost | `mir/egraph/{cost_shared_inputs,alternative_numbering,paired_views,gvn,gvn_phi,loop_congruence}.mir`, runtime companions | Retained alternatives, sharing, dominance, loops; new rules need shared and cross-block controls here. |
| Bool and cast rules | `mir/egraph/scalar_simplification.mir`; checker cast and i1-extension tests | 26 integer/pointer-cast clauses depend on valid MIR and type checks. |
| Call address cleanup | `mir/egraph/scalar_simplification.mir`; checker `CallEffectTests` | Four classic call forms, shared/cross-block/EOF negative cases; no dedicated dirty-address runtime fixture identified. |
| Address/balance | `mir/egraph/balance_masks.mir`, `balance_masks.sol`, `self_balance_evm_version.mir` | Dirty upper address bits, funded self and fork boundary. |
| Memory projections | `mir/egraph/{memory_object_identity_projections,memory_object_slice_projections,memory_object_gvn}.mir`, `memory_object_slice_fields.sol` | Three address identities and baseline/optimized slicing; not a general alias or allocation proof. |
| Sequence rules | `mir/word-sequence/{recipes,constraints,mined_masks,mined_words,seeded,select}.mir`, runtime companions; `lowering/run-call/constant_select.sol` | De Morgan, factored masks/shifts, power-of-two comparisons and PR1514 select factoring; includes noncanonical conditions and pointer exclusion. |
| Stack-resident selection | `lowering/stack_expression_selection.sol`, `stack_word_selection.sol` | Resident differences/XOR/bitwise terms, loop and shared cases. |
| EVM peepholes | `evm-ir/peephole/patterns/*.evmir`, `streaming.evmir`, `debug_info.evmir` | Constant, memory, branch, shuffle, extended-stack and metadata cases; general peepholes are outside the selected SMT files. |
| Late masks | `evm-ir/late-word/low_mask.evmir`; `mir/word-sequence/mined_masks_runtime.sol` | Gas/size/Paris shape checks plus runtime shift boundaries; not per-late-rule execution-hit evidence. |
| Opcode selection | Rust `backend/evm/codegen/select.rs::opcode_selection_matches_schema`, `select.snap` | 71 mapping clauses checked against schema stack shape, not a whole-backend equivalence proof. |

Pre-existing pass tests contain some exact proposed expressions outside the
e-graph test group. For example, translated comparisons occur in
`mir/loop-exit-remat/induction.mir`, `mir/loop-idioms/zero_count.mir`, and
lowering checks. Their selected pipelines test those passes; they do not prove
that an e-graph rule already exists or that the default pipeline removes them.

A source-form and MIR-definition screen found no exact broad-workload matches for several proposed nested DIV, MUL/OR/XOR constant chains, SDIV(MIN), or complemented-SAR shapes. That is not proof that inlining cannot produce them. The common-factor expression in `tests/foundry/unifap-v2/src/test/UnifapV2Pair.t.sol` occurs in test assertions, not deployed pool code. The new cases supply direct coverage where existing exact cases were not found.

The runtime corpus has relevant workloads in `testdata/runtime/VerifiedWords.sol`,
`SeededWords.sol`, `WordRecipes.sol`, `CompilerOptimizations.sol`, and
`Algorithms.sol`. The first three deliberately exercise existing rule families.
`CompilerOptimizations` covers packed state and joined checks; `Algorithms`
wraps Solady string, sort and Base64 work. The project archives broaden this to
real protocols and libraries. Their presence is **workload relevance**, not
proof that a particular new rule fires. No per-rule benchmark hit count or
baseline/candidate performance run was produced by this documentation task.

For R01–R38, use the new fixture to establish the exact missing source shape.
For packed masks/bytes, also use existing packed-storage and Base64 workloads;
for signed arithmetic, use signed decimal/fixed-point workloads; for equality
and scaling, use protocol arithmetic. Reject additions that only speed up
compilation. Before acceptance, compare matching runtime corpus IDs under gas
and size objectives, retain full artifacts, and investigate individual losses
rather than relying on an aggregate.

## Compiler-specific conclusions

**LLVM and solx.** Generic integer combining supplies the most useful missing
arithmetic and comparison families. solx explicitly lowers DIV with a zero
arm and SDIV with both zero and MIN/-1 handling. LLVM can use poison and
no-overflow facts inside that guarded representation; our raw EVM operations
cannot inherit those facts. Native EVM DIV, MOD and MUL also change the economics
of CPU strength reduction. See [solx arithmetic lowering](https://github.com/NomicFoundation/solx/blob/3d4c9c56176296204814dca17d8eb6c6f2e2503a/solx-codegen-evm/src/codegen/instructions/arithmetic.rs#L73).

**Cranelift.** Its generic ISLE files give direct structural patterns for
repeated masks, algebraic cancellation, signed shifts and comparisons. The
important semantic mismatch is masked CPU shift counts. Its native division,
rotate, SIMD and multiply-high strategy is not an EVM optimization plan.

**solc.** Most neutral/absorbing and two-input boolean identities are already
present here. The gaps are variable-power-of-two DIV, zero/MAX exponent bases,
constant MUL/OR/XOR reassociation, general sign-extension consumers, and
cost-sensitive shift/mask motion. The last group should remain alternatives,
since our stack traffic and push-width model can reject a profitable-looking
expression rewrite.

**Venom.** Scalar algebra overlaps substantially. Its larger lessons are
range facts, truth-only use rewriting, affine dataflow, memory-copy merging,
load/store elimination and assertion compatibility. These do not become safe
by spelling their final replacement in ISLE.

**Sonatina.** The most useful addition is general known-bit and demanded-bit
facts shared between passes. The current `max_bits` guard is a leading-bit
bound, not a full known-zero/known-one lattice. Sonatina's narrow integer,
saturating, undefined-value and typed rules need separate semantic checks.

**plank.** Its inspected constant-propagation pass mostly evaluates fully
constant EVM operations and propagates them through CFG edges. That is already
our evaluator/SCCP layer, not a new catalog of nonconstant ISLE identities.
Its SDIV, BYTE, SIGNEXTEND and overshift tests are useful evaluator boundary
references. Its nonzero environment assumptions must not transfer.

## Transformations that belong outside local ISLE

| Reference family | Sensible home and proof obligation |
|---|---|
| Venom `affine_folding.py` | Rust dataflow for base+offset relations across chains/phis; ISLE can consume a known relation, but repeated local rewrites do not replace analysis. |
| Venom `overflow_elimination.py` | Range facts may prove `max(x)+max(y)<=MAX` or `min(x)>=max(y)`; preserve panic branches until those facts hold on every path. Existing check-elim is the right layer. |
| Venom `memmerging.py` | Copy/zero-region coalescing with alias, overflow, memory expansion and observer checks. For example contiguous calldata copies can merge only when the same source bytes and observable behavior remain. Overlapping MCOPY sequences cannot merge merely because destinations are contiguous. |
| Venom `memory_copy_elision.py`, `load_elimination.py`, `dead_store_elimination.py` | Memory/storage/transient dataflow with call, reentrancy, unknown-alias and address-space invalidation. Existing copy-elision, memory-DSE, storage-load-CSE and CSE passes are the homes. |
| Venom `assert_combiner.py` | CFG changes require compatible revert data and safe intervening operations. Existing `iszero(x)&iszero(y)=>iszero(x\|y)` is only the pure boolean part. |
| Sonatina known/demanded bits | A Rust lattice and use-demand analysis; final small rules should query facts. Distinguish a producer's guaranteed bits from a consumer's ignored bits. |
| LLVM/Cranelift/Sonatina loop, induction, vector and code-motion transforms | CFG/loop passes with placement, alias and target support. PR1514's unswitching, closed forms, constant hashing and code sinking are already Rust passes; do not duplicate them as patterns. |
| Venom literal materialization; plank switch and block peepholes | Target constant construction, physical stack planning, EVM-IR CFG layout and tail merging. Keep assembly emission primitive. |

## Rejected transfers and required negative tests

| Tempting rewrite | Why it is wrong or not justified here |
|---|---|
| Remove `&255` from a shift count, or add shift counts modulo 256. | `SHL(256,1)=0`, while `SHL(256&255,1)=1`. Cranelift's count normalization models different semantics. |
| `x/x=>1`. | EVM gives 0 at x=0. Use R38 only when its resulting code is useful. |
| LLVM `1%x => x!=1`. | At x=0 the RHS is 1 and EVM remainder is 0. EVM's exact predicate is x>1. |
| `SDIV(x,2^k)=>SAR(k,x)` without facts. | SDIV(-3,2)=-1; SAR(1,-3)=-2. Rounding differs. |
| Combine arbitrary signed nested divisions. | `SDIV(SDIV(MIN,-1),2)` is negative; `SDIV(MIN,-2)` is positive. The intermediate wrapping overflow matters. |
| `(x*C)/C=>x`, cancel arbitrary equal left shifts, or cancel an even multiplier in EQ. | Overflow/dropped bits lose information. At x=MIN and C=2 the product is 0. |
| `(x/y)*y=>x-(x%y)` with no guard. | At y=0 and nonzero x, the two sides are 0 and x. |
| `(x+y)%m=>ADDMOD(x,y,m)` or product analogue for arbitrary m. | ADDMOD/MULMOD retain wide intermediates; raw ADD/MUL wrap first. For x=MAX,y=1,m=3, wrapping addition gives 0 while ADDMOD gives 1. |
| Turn checked source `(x+y)-y` into x while dropping overflow behavior. | Solidity can panic before the subtraction. Raw word equality is not source-program equivalence. |
| Assume ADDRESS, ORIGIN, CALLER, TIMESTAMP, GASLIMIT or CHAINID cannot be zero. | These are not generic EVM nonzero axioms. Plank's `constant_propagation.rs` contains such assumptions; they are not a rule source to copy. |
| `EXTCODESIZE(ADDRESS())=>CODESIZE`. | During construction, external deployed code is absent while current code is initcode. Venom explicitly recognizes this hazard. |
| CPU magic division or `x*(2^k±1)=>shift±x` by default. | EVM has cheap native word DIV/MUL, no native 256-bit multiply-high, and extra stack/immediate costs. Need a measured exceptional case. |
| Force every opposite shift pair into an AND mask. | Large PUSH constants and shared inputs can cost more; existing tests deliberately keep these pairs. |
| Replace a truth-tested word globally by a boolean. | Other users may return or store its noncanonical bits. Restrict to truth-demanding users. |
| Import LLVM poison/undef, native pointer provenance, vector, floating-point, or masked-shift rules. | These semantics/opcodes are not those of the raw EVM word layer. Typed MIR needs explicit separate legality. |

## Review of the existing rules and proof boundary

No confirmed executable miscompile emerged from this review. This is not a
full-compiler correctness proof. The following limits matter when extending it:

* The e-graph checks operand/result types before retaining or merging a rewrite.
  The 26 integer/pointer cast clauses rely on those checks and valid MIR.
* `mask_covers`, `below_const`, `sign_clear`, `shifted_out`, and related guards
  trust the Rust bit bound, including its depth-limited phi traversal. SMT proves
  the guarded identity, not the implementation of that analysis.
* `single_use` and `in_current_block` protect cost/liveness. Range and alignment
  guards protect semantics. Do not remove one because a proof ignores the other.
* Memory-object proofs show equal address words, not safe allocation, aliasing,
  length, contents or bounds. Balance uses one environment snapshot; classic-call
  rewrites preserve effective operands, not arbitrary callee execution.
* Sequence construction trusts placement, effect boundaries, typed conversion,
  metadata inheritance and dead-code credit. Resident stack selection trusts
  definition order and the current block/iteration.
* The seven selected stack rules and two late-window rules do not prove all
  physical peepholes. General legacy facets do not uniformly reject custom
  effects/protected boundaries; that is an integration review target, not a
  demonstrated reachable bug from this audit.
* Debug metadata must remain bytecode-neutral. Existing metadata snapshots and
  bytecode-neutrality tests are separate from word proofs.
* A second solver replay checks the same formula. It is not an independent EVM
  semantics implementation or a whole-compiler proof certificate.

The PR CI runner selects **460 clauses in six files** for the proof lane. The
71 opcode selectors are trusted/checked mappings; the 35 general peephole
clauses include a dispatcher and are not selected. Do not describe this as all
566 transformations proved. The README's older statements about a five-file
CLI default and range-free new word rules need that distinction: current CI
also selects egraph, and lossless word rules use `mask_covers`.

## Validation and next implementation step

The exact PR build compiled the new fixture with both objectives and passed
`none`, `gas`, `size`, and `mir` UI revisions. The 82 assertions include zero,
MAX, signed MIN, divisor-zero panic behavior, and 255/256/MAX shift counts.
The checked-in MIR snapshot records the checkout's standard unoptimized MIR;
the evidence appendix separately preserves optimized **PR-head** observations.
The fixture intentionally does not assert that unimplemented rules already fire.

A selected checker run passed **14 tests** covering all seven physical-stack
rules, both late-word rules, four classic-call operand rewrites, 26 cast clauses,
and guard-removal counterexamples. A separate 18-formula experiment proved 16
word equalities and timed out on two under the recorded five-second limit;
those timeouts remain unproved, not passed. Details and reproducible commands
are in the evidence appendix. This was not a fresh full 460-clause CI proof run.

For the first implementation, choose R01–R08 plus the direct sign/mask reductions
R17–R22/R24, one small group at a time. Add exact optimized MIR checks for the
selected rules and their shared/cross-block negative controls. Expand full-width
source-based proofs and runtime boundaries before comparing gas and size on the
existing corpus. Keep only candidates whose final scheduled code earns its cost.

## Precise upstream anchors

Cranelift links below resolve the `CL` file/line references in the catalog.

| Families | Pinned source |
|---|---|
| Constant reassociation, translated equality | [cprop.isle:214](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/cprop.isle#L214), [245](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/cprop.isle#L245). |
| Nested idempotence; mask/order and XOR facts | [bitops.isle:200](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/bitops.isle#L200), [762](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/bitops.isle#L762). |
| Complement absorption/XOR | [bitops.isle:58](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/bitops.isle#L58), [597](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/bitops.isle#L597), [647](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/bitops.isle#L647). |
| Equality, shift order, sign tests | [icmp.isle:478](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/icmp.isle#L478), [531](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/icmp.isle#L531). |
| Negation, set-mask subtraction, odd multiplier | [arithmetic.isle:23](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/arithmetic.isle#L23), [224](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/arithmetic.isle#L224), [249](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/arithmetic.isle#L249). |
| Opposite shifts, sign mask, shift factoring | [shifts.isle:24](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/shifts.isle#L24), [98](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/shifts.isle#L98), [315](https://github.com/bytecodealliance/wasmtime/blob/5cde11750fe20a580e8846f080e47d44b3e811ae/cranelift/codegen/src/opts/shifts.isle#L315). |
| solc scalar groups | [RuleList.h:148](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h#L148), [260](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h#L260), [451](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h#L451). |
| solc shifts, sign extension, exponentiation | [RuleList.h:513](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h#L513), [604](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h#L604), [659](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h#L659), [759](https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/RuleList.h#L759). |
| Venom algebra and analysis | [algebraic_optimization.py](https://github.com/vyperlang/vyper/blob/9b35e492b7e016f5bf9e1f907121391890ac8444/vyper/venom/passes/algebraic_optimization.py), [memmerging.py:184](https://github.com/vyperlang/vyper/blob/9b35e492b7e016f5bf9e1f907121391890ac8444/vyper/venom/passes/memmerging.py#L184), [assert_combiner.py:87](https://github.com/vyperlang/vyper/blob/9b35e492b7e016f5bf9e1f907121391890ac8444/vyper/venom/passes/assert_combiner.py#L87). |
| Sonatina facts | [simplify_expr.rs:1074](https://github.com/fe-lang/sonatina/blob/54492147373b8bf45f381bcef39477f0fb194648/crates/codegen/src/optim/simplify_expr.rs#L1074), [1468](https://github.com/fe-lang/sonatina/blob/54492147373b8bf45f381bcef39477f0fb194648/crates/codegen/src/optim/simplify_expr.rs#L1468), [known_bits_simplify.rs:290](https://github.com/fe-lang/sonatina/blob/54492147373b8bf45f381bcef39477f0fb194648/crates/codegen/src/optim/known_bits_simplify.rs#L290). |
| plank evaluator/environment assumptions | [constant_propagation.rs:194](https://github.com/plankevm/plank-monorepo/blob/1471137247f9d829bf81e879e8eaba223e648693/plankc/sir/crates/passes/src/optimizations/constant_propagation.rs#L194), [333](https://github.com/plankevm/plank-monorepo/blob/1471137247f9d829bf81e879e8eaba223e648693/plankc/sir/crates/passes/src/optimizations/constant_propagation.rs#L333). |

Official LLVM source anchors (not just the solx fork):

| Family | Official LLVM source and symbol | Result |
|---|---|---|
| Generic bounded reassociation | [InstructionSimplify.cpp:248-281](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Analysis/InstructionSimplify.cpp#L248), `simplifyAssociativeBinOp` at :250 | Same structural simplifier as fork. Reassociate only if the inner and outer expressions simplify or an existing value can be returned. Calls include MUL :939, AND :2150, OR :2456, XOR :2603. Cranelift provides the direct constant-specific rules cited in R01/R02. |
| Translate equality through ADD constant | [InstCombineCompares.cpp:3760-3766](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineCompares.cpp#L3760), `foldICmpBinOpEqualityWithConstant` at :3736 | Confirmed `(A+C2)==C -> A==(C-C2)`, NE too, retaining the one-use restriction. Safe modulo 2^256; target-priced constants and lifetimes still matter. |
| Signed power-of-two remainder zero-test | [InstCombineCompares.cpp:3748-3756](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineCompares.cpp#L3748), same symbol | Confirmed SREM->UREM for comparison to zero, constant >1 power of two, one use. EVM adapts positive constants first. |
| Division equality to range check | [InstCombineCompares.cpp:2912-2950](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineCompares.cpp#L2912), `foldICmpDivConstant` at :2857 | Confirmed explicit `X/u5==0` example at :2933, nonzero divisor exclusion at :2914, overflow-aware interval construction. Start with nonzero-constant zero-test, not wholesale signed interval machinery. |
| Nested constant division | [InstCombineMulDivRem.cpp:1371-1377](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineMulDivRem.cpp#L1371), `commonIDivTransforms` at :1358 | Confirmed multiply-overflow guard. EVM unsigned version needs exact product or explicit zero result for product >=2^256. Signed version requires extra intermediate MIN/-1 reasoning. |
| Signed division by MIN | [InstCombineMulDivRem.cpp:1909-1911](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineMulDivRem.cpp#L1909), `visitSDiv` at :1888 | Confirmed equality with MIN. Safe EVM identity without extra runtime guards. |
| MUL distributive factoring | [InstCombineAddSub.cpp:1619-1621](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineAddSub.cpp#L1619), `visitAdd` at :1604; subtraction at [2508-2510](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineAddSub.cpp#L2508), `visitSub` at :2413 | Confirmed both call `foldUsingDistributiveLaws`. Wrapping arithmetic identity transfers; checked source panic behavior must remain. |
| Unsafe X/X->1 | [InstructionSimplify.cpp:1080-1091](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Analysis/InstructionSimplify.cpp#L1080) | Confirmed LLVM returns 1 and treats zero divisor as poison. EVM requires X!=0 instead. |
| Unsafe 1%X->X!=1 | [InstCombineMulDivRem.cpp:2568-2572](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineMulDivRem.cpp#L2568), `visitURem` at :2543 | Confirmed. X=0 invalidates direct EVM transfer; unsigned X>1 is the EVM-specific alternative. |
| Division-product remainder identity needs nonzero divisor | [InstCombineMulDivRem.cpp:457-477](https://github.com/llvm/llvm-project/blob/0bd330675f9eb08126e467505a0800f167084473/llvm/lib/Transforms/InstCombine/InstCombineMulDivRem.cpp#L457) | Confirmed `(X/Y)*Y -> X-(X%Y)`. Fails at EVM Y=0 for X!=0. |
