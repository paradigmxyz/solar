# ISLE audit: executable examples and evidence

This is the pinned PR 1514 review. See the [PR 1523 implementation report](ISLE_RULE_IMPLEMENTATION.md) for current changes, coverage, measurements, and remaining limits.

[Main audit](ISLE_RULE_AUDIT.md) · [All existing clauses](ISLE_RULE_INVENTORY.md)

Compiler source: PR1514 `ac257d1a0955f139747b627300b37ec6cb481670`. Working checkout: `6a9fe7447e706fd64ee79e6bc84def1c962c05c3`. The compiler was built from a `git archive` of the PR head; an enclosing checkout's Git version string is not evidence of the archive revision. The current branch was not switched.

The [canonicalization follow-up](ISLE_RULE_CANONICALIZATION.md) records a
separate 40-function fixture, 78 runtime assertions, and its PR-head evidence.

## Runtime fixture

[rule_opportunities.sol](../tests/ui/codegen/mir/egraph/rule_opportunities.sol) contains **50 functions and 82 run-call/run-call-fail assertions**. The `none`, `gas`, `size`, and `mir` UI revisions passed on the PR build and on the working checkout. The fixture uses the existing UI EVM runner. These are concrete runtime expectations, not a solver proof for arbitrary inputs or a differential run against solc.

The [checked-in standard MIR snapshot](../tests/ui/codegen/mir/egraph/rule_opportunities.mir.stdout) belongs to the working checkout and tests source lowering. It deliberately does not require unimplemented optimizations. The optimized excerpts below come from the exact PR-head binary and retain the missed expressions.

## Reproduction

Use the pinned PR checkout with the new fixture copied into its existing UI test directory. Build and run:

```sh
cargo build -p solar-compiler --bin solar
ISLE_AUDIT_UI_BINARY=$(cargo nextest list --package=solar-compiler --test=tests --message-format json | jq -r '."rust-suites"."solar-compiler::tests"."binary-path"')
TESTER_MODE=ui "$ISLE_AUDIT_UI_BINARY" rule_opportunities
target/debug/solar tests/ui/codegen/mir/egraph/rule_opportunities.sol -Ogas -Zdump=mir
target/debug/solar tests/ui/codegen/mir/egraph/rule_opportunities.sol -Osize -Zdump=mir
```

The custom UI harness accepts fixture filters directly. Passing the fixture name to `cargo uitest` instead filters the single nextest harness test and runs no tests. The command above obtains the built harness path and passes the filter to that harness. Initial snapshot creation used `cargo uibless rule_opportunities`; the final direct harness run passed without blessing. The standard MIR snapshot may differ between the current branch and PR1514; compare it only with the matching source revision. No optimizer candidate was enabled for the runtime run.

## Optimized PR-head observations

Excerpts omit ABI-length checks, branches, result zero-extension and return-buffer boilerplate, but retain the final `mstore` operand so constant-folded results remain visible. `argN` is the corresponding source parameter. All operations listed are observed output, not proposed output.

### scale

```solidity
function scale(uint x) external pure returns (uint r) { unchecked { return (x * 3) * 5; } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = mul arg0, 3
v1 = mul v0, 5
mstore 128, v1
```

### sameFee

```solidity
function sameFee(uint x, uint y, uint fee) external pure returns (bool) { unchecked { return x + fee == y + fee; } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = add arg0, arg2
v1 = add arg1, arg2
v2 = eq v0, v1
mstore 128, v3
```

### oddScale

```solidity
function oddScale(uint x, uint y) external pure returns (bool) { unchecked { return x * 3 == y * 3; } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = mul arg0, 3
v1 = mul arg1, 3
v2 = eq v0, v1
mstore 128, v3
```

### nestedDifference

```solidity
function nestedDifference(uint balance, uint amount) external pure returns (uint) { unchecked { return balance - (balance - amount); } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = sub arg0, arg1
v1 = sub arg0, v0
mstore 128, v1
```

### invert

```solidity
function invert(uint x) external pure returns (uint r) { assembly { r := sub(not(0), x) } }
```

Both `-Ogas` and `-Osize`:

```text
v1 = sub 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, arg0
mstore 128, v1
```

### nestedAnd

```solidity
function nestedAnd(uint flags, uint mask) external pure returns (uint) { return flags & (flags & mask); }
```

Both `-Ogas` and `-Osize`:

```text
v0 = and arg0, arg1
v1 = and arg0, v0
mstore 128, v1
```

### nestedOr

```solidity
function nestedOr(uint flags, uint mask) external pure returns (uint) { return flags | (flags | mask); }
```

Both `-Ogas` and `-Osize`:

```text
v0 = or arg0, arg1
v1 = or arg0, v0
mstore 128, v1
```

### accumulatedFlags

```solidity
function accumulatedFlags(uint flags) external pure returns (uint) { return (flags | 0x100) | 0x20; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = or arg0, 256
v1 = or v0, 32
mstore 128, v1
```

### toggleFlags

```solidity
function toggleFlags(uint flags) external pure returns (uint) { return (flags ^ 0x100) ^ 0x120; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = xor arg0, 256
v1 = xor v0, 288
mstore 128, v1
```

### page

```solidity
function page(uint offset, uint bits) external pure returns (uint r) { assembly { r := div(offset, shl(bits, 1)) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = shl arg1, 1
v1 = div arg0, v0
mstore 128, v1
```

### normalized

```solidity
function normalized(uint amount) external pure returns (uint r) { assembly { r := div(amount, amount) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = div arg0, arg0
mstore 128, v0
```

### emptyPower

```solidity
function emptyPower(uint count) external pure returns (uint r) { assembly { r := exp(0, count) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = exp 0, arg0
mstore 128, v0
```

### negate

```solidity
function negate(int value) external pure returns (int r) { assembly { r := sdiv(value, not(0)) } }
```

Both `-Ogas` and `-Osize`:

```text
v1 = sub 0, arg0
mstore 128, v1
```

### ringAdd

```solidity
function ringAdd(uint x, uint y) external pure returns (uint) { return addmod(x, y, 256); }
```

Both `-Ogas` and `-Osize`:

```text
v2 = addmod arg0, arg1, 256
mstore 128, v2
```

### ringMul

```solidity
function ringMul(uint x, uint y) external pure returns (uint) { return mulmod(x, y, 256); }
```

Both `-Ogas` and `-Osize`:

```text
v2 = mulmod arg0, arg1, 256
mstore 128, v2
```

### reduceAdd

```solidity
function reduceAdd(uint x, uint modulus) external pure returns (uint) { return addmod(x, 0, modulus); }
```

Both `-Ogas` and `-Osize`:

```text
v1 = ne arg1, 0
v2 = addmod arg0, 0, arg1
mstore 128, v2
```

### reduceMul

```solidity
function reduceMul(uint x, uint modulus) external pure returns (uint) { return mulmod(x, 1, modulus); }
```

Both `-Ogas` and `-Osize`:

```text
v1 = ne arg1, 0
v2 = mulmod arg0, 1, arg1
mstore 128, v2
```

### belowBucket

```solidity
function belowBucket(uint amount, uint unit) external pure returns (bool r) { assembly { r := iszero(div(amount, unit)) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = div arg0, arg1
v1 = eq v0, 0
mstore 128, v2
```

### nestedQuotient

```solidity
function nestedQuotient(uint amount) external pure returns (uint r) { assembly { r := div(div(amount, 7), 3) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = div arg0, 7
v1 = div v0, 3
mstore 128, v1
```

### subset

```solidity
function subset(uint flags, uint mask) external pure returns (bool) { return (flags & mask) > flags; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = and arg0, arg1
v1 = gt v0, arg0
mstore 128, v2
```

### superset

```solidity
function superset(uint flags, uint mask) external pure returns (bool) { return (flags | mask) < flags; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = or arg0, arg1
v1 = lt v0, arg0
mstore 128, v2
```

### shiftComplement

```solidity
function shiftComplement(int value, uint bits) external pure returns (int) { return (~value) >> bits; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = not arg0
v1 = sar arg1, v0
mstore 128, v1
```

### signedNegative

```solidity
function signedNegative(int value) external pure returns (bool) { return (value >> 12) < 0; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = sar 12, arg0
v1 = slt v0, 0
mstore 128, v2
```

### highSign

```solidity
function highSign(int value) external pure returns (int) { return value >> 300; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = sar 300, arg0
mstore 128, v0
```

### signMask

```solidity
function signMask(uint value) external pure returns (uint r) { assembly { r := and(signextend(0, value), 255) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = signextend 0, arg0
v1 = and v0, 255
mstore 128, v1
```

### highByte

```solidity
function highByte(uint value) external pure returns (uint r) { assembly { r := shr(248, signextend(0, value)) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = signextend 0, arg0
v1 = shr 248, v0
mstore 128, v1
```

### realign

```solidity
function realign(uint value) external pure returns (uint) { return (value << 8) >> 8; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = shl 8, arg0
v1 = shr 8, v0
mstore 128, v1
```

### clearLow

```solidity
function clearLow(uint value) external pure returns (uint) { return (value >> 8) << 8; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = shr 8, arg0
v1 = shl 8, v0
mstore 128, v1
```

### affineConstant

```solidity
function affineConstant(uint x) external pure returns (bool) { unchecked { return x + 7 == 31; } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = add arg0, 7
v1 = eq v0, 31
mstore 128, v2
```

### booleanMask

```solidity
function booleanMask(uint flags, uint mask) external pure returns (bool) { return ((flags & mask) | mask) == mask; }
```

Both `-Ogas` and `-Osize`:

```text
mstore 128, 1
```

### complementSar

```solidity
function complementSar(int value, uint bits) external pure returns (int) { return ~((~value) >> bits); }
```

Both `-Ogas` and `-Osize`:

```text
v0 = not arg0
v1 = sar arg1, v0
v2 = not v1
mstore 128, v2
```

### sarOnes

```solidity
function sarOnes(uint bits) external pure returns (int) { return int256(-1) >> bits; }
```

Both `-Ogas` and `-Osize`:

```text
v1 = sar arg0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
mstore 128, v1
```

### signedMinQuotient

```solidity
function signedMinQuotient(int value) external pure returns (int r) { assembly { r := sdiv(value, shl(255, 1)) } }
```

Both `-Ogas` and `-Osize`:

```text
v1 = sdiv arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
mstore 128, v1
```

### fixedComplement

```solidity
function fixedComplement(uint value) external pure returns (bool) { return value == ~value; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = not arg0
v1 = eq arg0, v0
mstore 128, v2
```

### shiftedOrder

```solidity
function shiftedOrder(uint value, uint bits) external pure returns (bool) { return (value >> bits) > value; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = shr arg1, arg0
v1 = gt v0, arg0
mstore 128, v2
```

### tagDifference

```solidity
function tagDifference(uint tag, uint x, uint y) external pure returns (uint) { return (tag ^ x) ^ (tag ^ y); }
```

Both `-Ogas` and `-Osize`:

```text
v0 = xor arg0, arg1
v1 = xor arg0, arg2
v2 = xor v0, v1
mstore 128, v2
```

### repeatedPermission

```solidity
function repeatedPermission(uint flags, uint allowed, uint extra) external pure returns (uint) { return (flags & allowed) & (flags | extra); }
```

Both `-Ogas` and `-Osize`:

```text
v0 = and arg0, arg1
v1 = or arg0, arg2
v2 = and v0, v1
mstore 128, v2
```

### fillOutsideMask

```solidity
function fillOutsideMask(uint value, uint mask) external pure returns (uint) { return (value & mask) | ~mask; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = and arg0, arg1
v1 = not arg1
v2 = or v0, v1
mstore 128, v2
```

### negatedParity

```solidity
function negatedParity(uint value) external pure returns (uint) { unchecked { return (0 - value) & 1; } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = sub 0, arg0
v1 = and v0, 1
mstore 128, v1
```

### signFromBit

```solidity
function signFromBit(uint value) external pure returns (uint) { unchecked { return 0 - (value >> 255); } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = shr 255, arg0
v1 = sub 0, v0
mstore 128, v1
```

### shiftedDistance

```solidity
function shiftedDistance(uint x, uint y, uint bits) external pure returns (uint) { unchecked { return (x << bits) - (y << bits); } }
```

Both `-Ogas` and `-Osize`:

```text
v1 = shl arg2, arg1
v0 = shl arg2, arg0
v2 = sub v0, v1
mstore 128, v2
```

### extractMasked

```solidity
function extractMasked(uint value, uint mask, uint bits) external pure returns (uint) { return ((value << bits) & mask) >> bits; }
```

Both `-Ogas` and `-Osize`:

```text
v0 = shl arg2, arg0
v1 = and v0, arg1
v2 = shr arg2, v1
mstore 128, v2
```

### signedAligned

```solidity
function signedAligned(int value) external pure returns (bool r) { assembly { r := iszero(smod(value, 256)) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = smod arg0, 256
v1 = eq v0, 0
mstore 128, v2
```

### removeSetMask

```solidity
function removeSetMask(uint value) external pure returns (uint) { unchecked { return (value | 255) - 255; } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = or arg0, 255
v1 = sub v0, 255
mstore 128, v1
```

### combinedFee

```solidity
function combinedFee(uint rate, uint a, uint b) external pure returns (uint) { unchecked { return rate * a + rate * b; } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = mul arg0, arg1
v1 = mul arg0, arg2
v2 = add v0, v1
mstore 128, v2
```

### signedDelta

```solidity
function signedDelta(uint a, uint b) external pure returns (uint) { unchecked { return a + (0 - b); } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = sub 0, arg1
v1 = add arg0, v0
mstore 128, v1
```

### belowUnit

```solidity
function belowUnit(uint value) external pure returns (bool) { return value / 1000 == 0; }
```

Both `-Ogas` and `-Osize`:

```text
v4 = div arg0, 0x3e8
v1 = eq v4, 0
mstore 128, v2
```

### minimumOnly

```solidity
function minimumOnly(int value) external pure returns (bool) { return value < type(int256).min + 1; }
```

Both `-Ogas` and `-Osize`:

```text
v1 = slt arg0, 0x8000000000000000000000000000000000000000000000000000000000000001
mstore 128, v2
```

### alternatingSign

```solidity
function alternatingSign(uint exponent) external pure returns (uint r) { assembly { r := exp(not(0), exponent) } }
```

Both `-Ogas` and `-Osize`:

```text
v1 = exp 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, arg0
mstore 128, v1
```

### signedTopByte

```solidity
function signedTopByte(uint value) external pure returns (int r) { assembly { r := signextend(0, shr(248, value)) } }
```

Both `-Ogas` and `-Osize`:

```text
v0 = shr 248, arg0
v1 = signextend 0, v0
mstore 128, v1
```

## Existing-rule checker run

From the PR snapshot:

```sh
uv run scripts/evm-rules/test.py StackProofTests LateWordProofTests CallEffectTests RuleTests.test_actual_integer_and_pointer_cast_rules RuleTests.test_shift_cancellation_requires_a_lossless_input RuleTests.test_shifted_comparison_requires_constant_alignment RuleTests.test_byte_index_guard_prevents_wrapping_into_word
```

**14 tests passed in 53.403 seconds.** They include all seven compiled stack
clauses, both late-window clauses, all four classic-call operand rewrites,
26 cast clauses, and selected guard mutations. This is not a complete fresh
460-clause proof run. Temporary files created by the test runner were cleaned
by its context managers.

## Candidate word-proof experiment

The following sketches express 18 candidate formulas in the existing checker's
input language. They are not registered compiler rules. In particular, they
omit registration, declaration merging, opcode-availability guards, priorities,
cost gating, shared/cross-block cases, and an ISLE build/type check. The EXP
sketch explicitly converts its comparison result back to a word. Word-model
success does not establish compiler integration or justify erasing effects.

Save this block as `target/isle-audit/candidates.isle` in the pinned PR checkout:

```lisp
;; Additive cancellation in fee/offset comparisons.
(rule (rewrite (Op.Eq (add x z) (add y z))) (Op.Eq x y))
(rule (rewrite (Op.Ne (sub x z) (sub y z))) (Op.Ne x y))
;; Remaining balance minus remaining-after-payment recovers payment.
(rule (rewrite (Op.Sub x (sub x y))) (Op.Add y (imm (u256 0))))
;; Inverted masks.
(rule (rewrite (Op.Sub (all_ones) x)) (Op.Not x))
;; Repeated filters and flag setting.
(rule (rewrite (Op.And x (band x y))) (Op.And x y))
(rule (rewrite (Op.Or x (bor x y))) (Op.Or x y))
;; Variable page size; the zero divisor agrees with saturating SHR.
(rule (rewrite (Op.Div x (shl n (one)))) (Op.Shr n x))
;; Zero-base powers include 0**0 = 1.
(rule (sequence_rewrite (Op.Exp (zero) n))
      (if-let zero (imm (u256 0)))
      (if-let equal (make (Op.Eq n zero))) (sequence (Op.Zext equal)))
;; Subset/superset ordering.
(rule (simplify (Op.Gt (band x y) x)) (imm_bool false))
(rule (simplify (Op.Lt (bor x y) x)) (imm_bool false))
;; Sign test is invariant under saturating arithmetic shifts.
(rule (rewrite (Op.SLt (sar n x) (zero))) (Op.SLt x (imm (u256 0))))
;; A low-byte consumer ignores sign extension.
(rule (rewrite (Op.And (signextend (zero) x) (iconst m)))
      (if-let true (u256_eq m 255)) (Op.And x (imm m)))
;; Moving arithmetic offsets to a comparison constant.
(rule (rewrite (Op.Eq (add x (iconst a)) (iconst b)))
      (Op.Eq x (imm (u256_sub b a))))
;; Modular neutral operands retain EVM modulus-zero behavior.
(rule (rewrite (Op.AddMod x (zero) m)) (Op.Mod x m))
(rule (rewrite (Op.MulMod x (one) m)) (Op.Mod x m))
;; Sign extension complement commutes with arithmetic shift.
(rule (sequence_rewrite (Op.Sar n (bnot x)))
      (if-let shifted (make (Op.Sar n x))) (sequence (Op.Not shifted)))
;; A power-of-two modulus divides the wrapping word modulus.
(rule (sequence_rewrite (Op.AddMod x y (iconst m)))
      (if-let true (u256_eq m 256))
      (if-let sum (make (Op.Add x y)))
      (if-let mask (imm (u256 255))) (sequence (Op.And sum mask)))
(rule (sequence_rewrite (Op.MulMod x y (iconst m)))
      (if-let true (u256_eq m 256))
      (if-let product (make (Op.Mul x y)))
      (if-let mask (imm (u256 255))) (sequence (Op.And product mask)))
```

Run from that PR checkout, so the checker uses the matching operation schema,
extractors and instruction-selection mapping:

```sh
uv run scripts/evm-rules/verify.py verify target/isle-audit/candidates.isle --timeout-ms 5000 --output target/isle-audit/candidate-proofs.json
```

The observed result was **16 proved, 2 unknown (timeout)**; the command correctly
exited nonzero. Both unknowns are the ADDMOD/MULMOD neutral-operand formulas
R10. They remain unproved by this run, despite the elementary modular argument.
No timeout was accepted as a proof, and no cvc5 fallback or replay was run for
these proposals. The rule checker first checks satisfiable applicability, then
word equality; its ordinary trusted type/semantic contracts still apply.

| Rule source line | Result | Trusted contracts reported |
|---|---|---|
| 2 | proved | None |
| 3 | proved | Op.Ne: trusted MIR word inequality or bit-preserving scalar cast |
| 5 | proved | None |
| 7 | proved | None |
| 9 | proved | None |
| 10 | proved | None |
| 12 | proved | None |
| 14 | proved | Op.Zext: trusted MIR word inequality or bit-preserving scalar cast |
| 18 | proved | None |
| 19 | proved | None |
| 21 | proved | None |
| 23 | proved | None |
| 26 | proved | None |
| 29 | unknown (timeout) | None |
| 30 | unknown (timeout) | None |
| 32 | proved | None |
| 35 | proved | None |
| 39 | proved | None |

Recorded inputs:

- `solver`: `4.16.0`.
- `word_bits`: `256`.
- `prelude_sha256`: `1f26349351c3f4ab1a8d36912e36cd73bb2db5fd0ef05188ddf7245e5fd91111`.
- `extractors_sha256`: `2fcb5238a7544d66fb4661b39c4feb3d455ff85de934cdee3076b8f8aab6634a`.
- `selection_sha256`: `9bd26f60e82f4a3b6c9da9b9d64fe1b7154f7501ac39afe2f1b1163bbbe60ba0`.
- `implementation_sha256`: `c158d0f4274f3e396051ee39dcc0591a930de61a59b473368446e60e89ff65d8`.
- Candidate source SHA-256: `4ae8b6728d4a6c25221100c3f7442adf3aa6ed59e9a4745a72246480375eeb51`.


## Checkout checks

`cargo fmt --all -- --check` passed. The stable formatter warned that the
repository's nightly-only formatting options were unavailable; no Rust source
changed. `cargo cl` could not compile the all-features dependency configuration:
`smallvec` specialization and `thread_local` require nightly. No toolchain switch
was made. `cargo clippy --workspace --all-targets` then passed with default
features and 12 existing boolean-simplification warnings in unchanged codegen
source (duplicated for the library test target).

The UI fixture's Solidity compiled and ran through the standard test runner on
both source revisions. Markdown local links, code fences, source counts and
trailing whitespace were checked. No benchmark, full workspace test suite,
full fresh proof gate, commit, push, or PR update was performed.
