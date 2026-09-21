# ISLE rules on PR 1523

This change builds on [PR 1523](https://github.com/paradigmxyz/solar/pull/1523),
commit `669a2e2e3c29be42b753f393204157e66581050a`. The
[original audit](ISLE_RULE_AUDIT.md), [canonicalization review](ISLE_RULE_CANONICALIZATION.md),
[baseline evidence](ISLE_RULE_EVIDENCE.md), and [clause inventory](ISLE_RULE_INVENTORY.md)
retain their PR 1514 pins and observations. They are historical evidence, not
claims about this implementation's output.

The initial expansion added 259 net rule clauses across the three MIR rule files: 78 in
`egraph/`, 116 in `word/`, and 65 in `word_sequence/`. They cover
local scalar identities, bounded expression recipes, typed boolean
cleanup, and unused memory-pointer normalization. The tables below account for
every proposed family. “Implemented” means a rule is available under its guards;
it does not mean every source expression reaches that shape or that extraction
always selects it. Profitability still depends on target costs, sharing,
placement, and stack traffic. Some proposed extensions require analyses or IR
support beyond local ISLE matching; those limits remain explicit.

## Placement and safety

Each rule set has [modules grouped by root operation](../crates/codegen/isle/mir/README.md).
The split preserves rule forms, guards, priorities, and the order of alternatives
for each operation. The build lists modules explicitly; proof CI checks every
module in each rule-set directory.

Single-node alternatives live in [word/](../crates/codegen/isle/mir/word/).
Value substitutions, typed boolean rules, and effect-preserving operand cleanup
live in [egraph/](../crates/codegen/isle/mir/egraph/).
Bounded multi-node alternatives live in
[word_sequence/](../crates/codegen/isle/mir/word_sequence/), after e-graph
extraction. The recipe cost includes only producers that can be deleted, and
charges retained/shared inputs. Matching does not insert speculative MIR.

All arithmetic identities use wrapping 256-bit words, EVM zero-divisor behavior,
and saturating shifts. Rules preserve checked-source panics and effects. Shift
introductions require fork support. Select factoring preserves integer widths;
pointer, aggregate, and wider-than-word arithmetic selects remain excluded.
Narrow ordered comparisons are not introduced: current MIR ordered operations
require word operands. EQ/NE accept only i1, i160, and i256, so a general i8
comparison cleanup would also need an IR legality change.

Empty CALL input/output, LOG data, and REVERT memory starts become zero only
when their own size is zero. Calls, logs, reverts, other operands, and effects
remain. The e-graph now visits rewriteable instructions without results, so LOG
rules run. REVERT is a terminator and uses the existing terminator rewrite path.
This does not justify dropping RETURNDATACOPY bounds checks for empty copies.

## Main audit families

The named Solidity probes below live in
[rule_opportunities.sol](../tests/ui/codegen/mir/egraph/rule_opportunities.sol),
unless marked as MIR. The fixture includes exact runtime assertions under the
standard codegen matrix; its MIR snapshot records the source lowering shape.
[audit_rules.mir](../tests/ui/codegen/mir/egraph/audit_rules.mir) adds direct pass
snapshots for positive and rejection cases under both gas and size objectives.

| ID | Implementation and limits | Probe |
| --- | --- | --- |
| R01 | Combine wrapping constant MULs; extraction prices the new immediate and retained producer. | `scale` |
| R02 | Combine constant OR/XOR chains. | `accumulatedFlags`, `toggleFlags` |
| R03 | Move ADD/SUB constants through EQ/NE, including wraparound. | `affineConstant` |
| R04 | Cancel shared ADD/SUB/negation operands through EQ/NE. | `sameFee` |
| R05 | Nested subtraction and negation identities. | `nestedDifference`, `signedDelta` |
| R06 | `MAX-x => ~x`. | `invert` |
| R07 | Reuse repeated AND/OR operands in either order. | `nestedAnd`, `nestedOr` |
| R08 | Divide by dynamic `SHL(n,1)` using SHR, including overshifts. | `page`, MIR `dynamic_power_divisor` |
| R09 | Zero-base EXP becomes a typed zero-exponent predicate. | `emptyPower`, MIR `zero_power` |
| R10 | Neutral ADDMOD/MULMOD operands become MOD. Covers constant-left neutral operands even when both operands are constants. | `reduceAdd`, `reduceMul`; follow-up `neutralLeft` |
| R11 | Power-of-two modular arithmetic becomes wrapping arithmetic plus a mask when the recipe wins. | `ringAdd`, `ringMul`; MIR `ring_add`, `ring_mul` |
| R12 | Combine constant unsigned divisors with a nonoverflowing product; overflowing product gives zero. | `nestedQuotient`; follow-up `overflowingQuotient`, `zeroDivisor` |
| R13 | Constant nonzero quotient-zero tests become unsigned bounds. Dynamic zero divisors are not assumed nonzero. | `belowUnit`, rejection control `belowBucket` |
| R14 | Unsigned subset/superset bounds fold. | `subset`, `superset` |
| R15 | Unsigned SHR cannot exceed its input. | `shiftedOrder` |
| R16 | A word cannot equal its complement. | `fixedComplement` |
| R17 | Cancel complements around SAR. | `complementSar` |
| R18 | SAR of all ones remains all ones. | `sarOnes` |
| R19 | SAR preserves signedness for every count. | `signedNegative` |
| R20 | Constant SAR overshifts use count 255. | `highSign` |
| R21 | Remove SIGNEXTEND under a mask that ignores sign-fill bits. | `signMask` |
| R22 | Aligned high-field SIGNEXTEND/SHR becomes SAR. | `signedTopByte` |
| R23 | Equal opposite shifts become masks only with a local single-use producer and no larger push cost. | `realign`, `clearLow`; existing `lossless_shifts.mir`, `bit_slices.mir` |
| R24 | SDIV by signed MIN becomes equality with MIN. | `signedMinQuotient` |
| R25 | Cancel the same odd constant factor from both sides of EQ/NE. Even factors remain. The additional constant-RHS modular-inverse form is not implemented. | `oddScale`, follow-up `evenScale`; MIR `odd_scale`, `even_scale` |
| R26 | Cancel a common XOR tag, all operand orders. | `tagDifference` |
| R27 | Absorb repeated operands across nested AND/OR. | `repeatedPermission` |
| R28 | Remove redundant masked bits under a complementary fill. | `fillOutsideMask` |
| R29 | Negation preserves the low parity bit. | `negatedParity` |
| R30 | Negated high-bit extraction becomes SAR255. | `signFromBit` |
| R31 | Factor same-count SHLs through subtraction. | `shiftedDistance` |
| R32 | Fuse shifted masked extraction, including saturated counts. | `extractMasked` |
| R33 | Positive power-of-two signed-remainder zero tests use low-bit masks; the remainder itself is not replaced. | `signedAligned` |
| R34 | `(x|C)-C => x&~C`, as a costed alternative. | `removeSetMask` |
| R35 | Factor a common multiplier across ADD/SUB with producer deletion and sharing costs. | `combinedFee`; MIR `factored_mul`, `factored_shared` |
| R36 | Signed endpoint and impossible-bound comparisons. | `minimumOnly` |
| R37 | EXP of all ones uses exponent parity when cheaper. | `alternatingSign` |
| R38 | Costed self-division and unit-dividend remainder recipes, preserving divisor zero. | `normalized`, `firstRingIndex`; recipe proofs |

## Follow-on families

Follow-up source probes live in
[canonicalization_opportunities.sol](../tests/ui/codegen/mir/egraph/canonicalization_opportunities.sol).
The scope below deliberately distinguishes contractions from neutral motion.

| ID | Implemented scope | Remaining scope or constraint |
| --- | --- | --- |
| F01 | Combine two constant-bearing children for ADD/MUL/AND/OR/XOR; `associateScale`. | No free reassociation that saves nothing. |
| F02 | Bounded subtraction recipes that expose contraction, including outer ADD/SUB constants and `(A-x)-y` forms. | No unrestricted motion of constants through subtraction. |
| F03 | Unequal opposite constant shifts use a residual shift and exact shifted mask; `oppositeShifts`. | Equal counts use R23's stricter placement/push guards. |
| F04 | Move a mask through SHL/SHR when an outer AND consumes it. | Standalone neutral mask motion stays absent. |
| F05 | Drop a child whose masked bits cannot reach the consumer; `disjointField`. | No general distribution that grows the expression. |
| F06 | Drop SIGNEXTEND when SHL discards all sign-fill bits; `signDiscard`. | No recipe that merely relocates a surviving extension. |
| F07 | Complement/XOR contractions, including direct `x&(~x^y)` and operand orders; `xorIntersection`. | Shared producers still count in extraction. |
| F08 | Syntactic conflicting-bit comparisons and repeated-mask cleanup. | General known-zero/known-one propagation and signed phi facts need a Rust analysis. |
| F09 | Fold zero/nonzero consumers of OR with a nonzero constant; retain existing branch cleanup. | No replacement of general word users by a boolean. |
| F10 | Local low-bit/byte consumers ignore unused sign-fill bits. | Whole-user demanded-bit propagation is not implemented. |
| F11 | SDIV by a positive power of two uses SHR for a proven nonnegative word, or SAR for syntactically proven exact divisibility. | No general biasing expansion or new path-sensitive range analysis. Negative inexact division is a rejection control. |
| F12 | Constant-factor MUL/DIV cancellation with a nonzero divisor and no-overflow bound; `unsignedProduct`. | Dynamic-factor range reasoning and new environment facts are not implemented. |

## Cleanup and canonicalization

| ID | Implementation and limits | Evidence |
| --- | --- | --- |
| C01 | Compare widened i1 values with one without retaining the widening. | `widenedTrue`, `widenedFalse`, MIR boolean-extension cases |
| C02 | All four boolean select forms use typed boolean combinations. | `selectTrue`, `selectFalse`, MIR boolean-select cases |
| C03 | Reuse inverted boolean select conditions and swap arms. Existing boolean zero-test cleanup strips redundant tests where types permit. | `invertSelect` |
| C04 | Compare distinct constant select tags directly with the typed condition. | `selectTag` |
| C05 | Equality-based select arm substitution and zero/subtraction contraction. | `selectDistinct`, `zeroSubtract` |
| C06 | Factor shared NOT, negation, and compatible ZEXT/SEXT/TRUNC operations from select arms. Preserve the source type of the temporary select. | `factoredNot`, `selectSigned`, `selectBool`; MIR matching/mixed extension controls |
| C07 | Fold arithmetic/bitwise operations into constant select arms. | `constantArms`, MIR `constant_select` |
| C08 | Fuse nonzero consumers of widened boolean AND/OR/XOR, plus zero consumers of AND/OR. XOR-zero already composes through existing equality rules. | `boolAnd`, `boolOr`, `booleanZeroFlags`; MIR boolean combinations |
| C09 | Combine same-operand strict comparisons with EQ/NE through AND/OR/XOR, including reversed operands and EQ/NE complements. Signed and unsigned ordering stay separate. | `comparisonFlags`, `comparisonComplements`, `mixedSigns`; source-rule proofs |
| C10 | Narrow zero tests of extended i160 values and sign-extended i1 values, retaining correctly typed zero immediates. | Other narrow widths and ordered comparisons still need legality work; MIR i8 and mixed-sign controls remain unchanged. |
| C11 | Invert constant bounds only away from signed/unsigned endpoints and only when the replacement push does not grow. | `upperBound`, `lowerBound`, `signedUpper`, `signedLower`; MIR compact/shifted bound controls |
| C12 | Reverse complemented order and move constant NOT/XOR through EQ/NE. | `complementOrder`, `signedComplementOrder`, `invertedEqual`, `xorEqual` |
| C13 | Offer costly negative ADD constants as positive SUB constants; target extraction chooses. Keep ADD by MAX for cheaper commutative decrement scheduling. | `constantSub`, `wrappedAdd` |
| C14 | Fuse BYTE31 with a low mask. | `byteNibble` |
| C15 | Combine repeated complements, including swapped outer operands. | `clearFields`; exact recipe proofs |
| C16 | Existing consumer contraction already handles `~(x+7)+7=>~x`. | `complementConsumer`; carry-safe logic/add motion without a reducing consumer remains deferred. |
| C17 | Replace saturated sign-mask zero tests with signed predicates. | `zeroSign`, `highSign` |
| C18 | Zero unused memory starts in CALL/LOG/REVERT while retaining effects. | `emptyCall`, `emptyLog`, `emptyRevert`; MIR nonempty controls and proof guard mutations |
| D01 | Fix the select rule's false immediate to i1. | MIR `boolean_select_inverse`, full verifier-enabled UI suite |
| D02 | Remove remaining constant-left LT/boolean-NE mirrors. PR 1523 already removed the audited word-file mirrors. Also remove newly duplicated widened-boolean zero tests. | Canonical operand handling, constant folding, existing zero-test fixtures |
| D03 | Remove the duplicate repeated SIGNEXTEND alternative. | Existing signed-extension fixtures and rule proofs |

## Proof boundary

The offline checker verifies the actual source rules, with source, helper, and
query hashes. CALL/LOG cleanup compares the same effect and every meaningful
operand, normalizing a memory start only under its corresponding zero size.
Removing the size guard yields counterexamples in the checker tests. It does
not model gas, stack scheduling, memory contents, or arbitrary effect motion.

Hard select proofs split the full zero/nonzero condition. Odd-factor EQ/NE
proofs cover all 256 least differing bits with 772 checked obligations, without
an assumed modular inverse. Mixed opposite shifts use complete output-bit or
index partitions. Applicability witnesses establish satisfiable guards only;
all equivalence inputs remain symbolic. Missing partitions, timeouts, and
unsupported semantics do not count as proofs. See the
[proof guide](../scripts/evm-rules/README.md) for replay and trust limits.

## Initial validation and measurements

The figures in this section describe the initial expansion, before the simple-rule follow-up below. Benchmark artifacts stay under `target/codegen-bench/`;
proof reports, UI logs, and the raw UI bytecode sweep stay under
`target/isle-audit/`. The baseline binary is `target/isle-audit/solar-pr1523`.

The benchmark workload includes real repository contracts and compilation-only
projects. The solc v0.8.37 v4-core stack-too-deep failure occurs in both baseline
and candidate reference rows; it is not a new compiler failure. Runtime
comparisons exclude compilation-only projects and failed reference rows.

The final guards follow observed codegen costs. Equal opposite shifts could
extend a producer across blocks or replace a compact shift count with a large
mask. Bound inversion could turn compact shifted thresholds into PUSH32s and
prevent lossless-shift cancellation. We restrict those alternatives and pin
both cases with MIR snapshots. Algebraic equivalence alone does not establish
an improvement.

The two source fixtures contain 111 functions and 193 exact runtime assertions.
The direct pass fixture adds 38 MIR functions, including even-factor rejection,
shared producers, wrapping products, inexact signed division, nonempty effects,
mismatched cast widths, compact bounds, and pointer decrements. The full codegen
UI run covers 3,358 revisions; the codegen unit/schema suite covers 335 tests.
The offline proof-tool suite covers 117 tests, including missing size guards,
noncanonical select conditions, rejected specializations, incomplete partitions,
and applicability witnesses that must not specialize equivalence.

Rust formatting and `cargo clippy --workspace --all-targets` pass. Clippy reports
existing warnings in unchanged modules. The `cargo cl` alias enables all features
and fails on this stable toolchain's existing `smallvec` specialization feature;
we did not switch toolchains. Python formatting, Ruff lint, and `ty` checks pass.

The proof reports cover 707 rules: 308 e-graph rules, 227 word alternatives, and
172 word-sequence/backend rules. The final word report is
`target/isle-audit/1523-delivery-word-proofs.json`; the final e-graph and backend reports are
`target/isle-audit/1523-verified-proofs/egraph-*/proofs.json` and
`target/isle-audit/1523-verified-proofs/other-0/proofs.json`. This count is source rules, not
individual solver queries or runtime test cases.

The bytecode sweep compiles 581 standard-matrix UI source files with both
binaries under `-Ogas` and `-Osize`, using `--emit=bin-runtime --allow=2264`.
One deliberate recursive-creation error fails identically in both binaries and
both modes. The 580 successful source files contain 777 comparable contracts
per objective. This sweep measures runtime bytecode length, not runtime gas.
Raw rows and bytecode hashes are in `target/isle-audit/1523-ui-size-delivery.json`.

| UI bytecode comparison | Smaller | Unchanged | Larger | Net bytes |
| --- | ---: | ---: | ---: | ---: |
| `-Ogas`, 777 contracts | 59 | 700 | 18 | -815 |
| `-Osize`, 777 contracts | 59 | 713 | 5 | -907 |

The largest individual increase is 16 bytes under `-Ogas` and 4 bytes under
`-Osize`. These are retained tradeoffs, not hidden by the total.

| New source fixture | Objective | PR 1523 bytes | Candidate bytes | Delta |
| --- | --- | ---: | ---: | ---: |
| `rule_opportunities.sol` | gas | 1,968 | 1,703 | -265 |
| `rule_opportunities.sol` | size | 1,766 | 1,441 | -325 |
| `canonicalization_opportunities.sol` | gas | 2,523 | 2,417 | -106 |
| `canonicalization_opportunities.sol` | size | 2,284 | 2,153 | -131 |

For the runtime/project benchmark, both runs use the same explicit 32 test IDs,
debug compiler profile, hot gas workload, optimization settings, and five
compilation samples (the harness stops repeats for long compiles by default).
Reference solc results come from the baseline. There are 23 executable contract
benchmarks and nine compilation-only projects. Timings overlap other work on
this shared machine, so they do not isolate compiler-time effects.

| Runtime/project metric | Candidate versus PR 1523 |
| --- | ---: |
| Runtime gas | -0.02% |
| Runtime bytes | -0.07% |
| Creation bytes | -0.06% |
| Deployment gas | -0.04% |
| Compile time, shared-machine measurement | +4.16% |
| Peak RSS, shared-machine measurement | -0.30% |

These are geometric means of comparable positive case ratios, not a percentage
of summed gas. The harness excludes 22 LibString memory-brutalizer calls from
the gas comparison because their work depends on gas and contract bytecode;
they still execute and retain their raw measurements in the report. All 32
candidate compiler runs succeed. The only failed compiler row is the unchanged
solc v4-core reference failure described above.

Measured gas totals fall by 1,493 for Solady Algorithms, 320 for verified words,
24 for Aave's encoder, and 18 for comparable LibString calls. Two mixed-sort
calls regress by 5 and 6 gas. LibString grows by 9 runtime bytes. Those tradeoffs
remain; the decimal-formatting regression is gone. This is not a claim that
every function gets smaller or cheaper.

The baseline is `target/codegen-bench/isle-1523-baseline/results.json`; the final
candidate is `target/codegen-bench/isle-1523-delivery/results.json`. The
[full comparison](../target/codegen-bench/isle-1523-delivery-comparison.md)
contains per-case and per-call deltas, excluded gas measurements, compilation
failures, and artifact changes. Matching JSON and MIR/EVM/disassembly/bytecode
diffs sit beside that report. The successful proof reports contain 8,036 saved
queries; they establish the modeled identities, not the cost results.

## Simple-rule follow-up

This batch adds 116 clauses on top of the initial implementation: 62 e-graph
rules, 24 word alternatives, and 30 bounded recipes. Operand-order variants
account for much of that count; these are four groups of local identities,
not 116 distinct algorithms. The combined change adds 375 net clauses over
PR 1523.

- **Boolean zero consumers:** move AND/OR of widened i1 operands back to i1
  before testing zero. XOR-zero already folds through XOR equality followed by
  equality of extensions, so it gets a regression case without a duplicate rule.
- **Comparison truth sets:** complete the local combinations of strict order
  with EQ/NE under AND, OR, and XOR. Cover reversed outer operands, reversed
  equality operands, opposite comparisons spelled with the same opcode, and
  EQ/NE complements. Reuse a surviving predicate where possible. Do not combine
  unsigned order with signed order.
- **Typed zero checks:** compare an extended i160 source with an i160 zero,
  and a sign-extended i1 source with false. General i8 comparisons remain word
  comparisons because the current MIR legality rules do not permit i8 EQ/NE.
  No type or instruction legality changes were made.
- **Bounded constant contraction:** combine an outer ADD/SUB constant through
  `(A-x)-y`, `x-(y+A)`, and `x-(A-y)`. These recipes remove an operation; they
  do not introduce neutral reassociation merely to change the expression's shape.

The existing pass fixture checks the resulting MIR under gas and size settings.
The source fixture adds comparison-flag packing, empty/address validation,
remaining-capacity arithmetic, and reservation/header arithmetic. It executes
zero, equal, unequal, signed-extreme, and wrapping cases. Negative controls
retain mixed signedness and unsupported narrow comparison widths.

The baseline binary for this batch is `target/isle-audit/solar-before-simple`.
Both benchmark runs use the full unfiltered runtime/project corpus, one compile
sample, and the hot gas workload. They retain the existing Uniswap V2 parser
failure for a bare `chainid` builtin; it occurs on both sides. Single-sample
compiler timing is not treated as a performance result.

The final rules pass 823 source-rule proof obligations: 370 e-graph, 251 word,
and 202 sequence/backend rules. These establish equivalence under the checker's
modeled contracts, not end-to-end compiler correctness. The checker passes 118
tests, including six typed extension/zero obligations. The codegen suite passes
335 unit/schema tests and 3,358 UI revisions. Formatting, Python lint/type checks,
and workspace Clippy pass; Clippy retains existing warnings.

This batch adds 26 functions to the existing MIR fixture and ten functions with
24 runtime assertions to the existing source fixture. Together, the two audit
source fixtures now contain 121 functions and 217 runtime assertions.

The UI bytecode sweep compares 777 contracts per optimization mode across 580
successful standard-matrix sources. The deliberate recursive-creation error is
unchanged on both sides. Each mode has 771 unchanged contracts, three smaller
contracts, and three larger contracts. Total runtime bytes fall by 25 under
`-Ogas` and 14 under `-Osize`. The changed contracts are:

| Contract | Gas-mode byte delta | Size-mode byte delta |
| --- | ---: | ---: |
| CanonicalizationOpportunities | -29 | -20 |
| YulParamReassignment | -2 | -2 |
| MemoryStructAssemblyList | -1 | -1 |
| ForwardingHarness | +1 | +1 |
| Decompression-loop Harness | +1 | +1 |
| TryCreation | +5 | +7 |

The raw sweep is `target/isle-audit/simple-ui-size-final.json`. Most of the
savings come from the new regression fixture; these totals do not establish a
broad real-world size improvement.

The final full-corpus run passes all runtime checks and gas transactions for
successfully compiled cases: 32 compiler runs succeed and the unchanged Uniswap
parser case fails. Comparable runtime gas is unchanged. LibString falls from
60,710 to 60,706 runtime bytes and from 13,198,012 to 13,197,196 deployment gas
(-816). The other comparable size and gas rows are unchanged. The harness
excludes 22 bytecode/gas-dependent LibString memory-brutalizer calls from the
gas comparison while retaining their raw results.

The final candidate is `target/codegen-bench/isle-simple-final-isolated`;
[the comparison](../target/codegen-bench/isle-simple-final-isolated-comparison.md)
links the measurements to the saved artifacts. An earlier candidate run suffered
RPC failures and deployment timeouts on the default local port. Its partial
results are retained in `target/codegen-bench/isle-simple-final` but are not used
for these conclusions. The complete rerun used a separate local RPC port.
