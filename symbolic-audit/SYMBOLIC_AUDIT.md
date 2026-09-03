# Symbolic differential audit

Bounded solc-vs-solar symbolic differential run with `fuzz/bin/solsymdiff`
(Foundry symbolic executor, z3) over every external `pure`, `view`, and
`nonpayable` function in the codegen UI tests (`tests/ui/codegen/**`) and the
solc semantic tests (`testdata/solidity/test/libsolidity/semanticTests/**`).
Both compilers receive the same Standard JSON input: `evmVersion=osaka`,
optimizer on with 200 runs, `viaIR=true`. solc's via-IR backend is the
reference throughout; where solc's legacy backend behaves differently, the
via-IR behavior is the one to match. Storage is zero-initialized and
constructors do not run. Files that observe their own address, create
contracts, make external calls, or use value transfer were skipped.

Each entry below was confirmed by concrete replay of the counterexample
calldata against both runtimes. Reproduction files live in `symbolic-audit/`.

| Date | solc | solar | forge |
|------|------|-------|-------|
| 2026-09-01 | 0.8.36+commit.8a079791 | 840bedd89 (`dani/rewrite-codegen-lowering`) | 1.8.1-nightly a998dbf |

## Checklist

All 19 items are fixed in code and re-verified against `e6db78e6b` on
2026-09-02 with the repros below (symbolic where the executor can model the
input, concrete otherwise). Each item names the commit that fixed it.
CODEGEN-003, 004, and 005 in `docs/SOLC_DIVERGENCE.md` predate those fixes
and describe behavior that no longer differs.

- [x] 1. Integer literal expressions lose arbitrary-precision semantics during lowering
      (`symbolic-audit/literal_addmod_fold.sol`; fixed in `52c20ab3f`)
- [x] 2. Public array getter out-of-bounds reverts with `Panic(0x32)` instead of empty data
      (`symbolic-audit/getter_out_of_bounds.sol`; fixed in `b20583ae2`)
- [x] 3. Assembly-assigned calldata pointer past `calldatasize` reverts instead of reading zeros
      (`symbolic-audit/assembly_calldata_pointer_encode.sol`; fixed in `361e596e0`)
- [x] 4. Oversized memory array allocation loses the `Panic(0x41)` check
      (`symbolic-audit/memory_array_too_large.sol`; fixed in `ba539e44a`)
- [x] 5. Storage-to-memory copy in tuple assignment happens before later lvalues
      (`symbolic-audit/storage_to_memory_tuple_order.sol`; fixed in `343f64eda`)
- [x] 6. Underflowed assembly calldata slice reverts with empty data instead of `Panic(0x41)`
      (`symbolic-audit/assembly_calldata_slice_underflow.sol`; fixed in `277169cb0`)
- [x] 7. `-Onone`: value crossing a branch is replaced by its derived address at the store
      (`symbolic-audit/stack_rematerialization_unoptimized.sol`; fixed in `2673f5128`)
- [x] 8. Unused attached library function as a statement lowers to `INVALID`
      (`symbolic-audit/unused_bound_library_function.sol`; fixed in `c58ffb535`)
- [x] 9. Public `bytesN` constant from a hex literal returns the masked memory pointer
      (`symbolic-audit/hex_literal_fixed_bytes_constant.sol`; fixed in `d0ac5763a`)
- [x] 10. Dirty user-defined value type passed as a function argument is treated as clean in the callee
      (`symbolic-audit/udvt_dirty_param.sol`; fixed in `8e87d88bd, 5c2753827, 1738b4454`)
- [x] 11. Static calldata array validation is eager in solar, lazy per element in solc
      (`symbolic-audit/calldata_static_array_validation.sol`; fixed in `ac68e1e40`)
- [x] 12. Dirty narrow value used directly as a `new` length or wider mapping key is not masked
      (`symbolic-audit/implicit_widen_alloc_mapping.sol`; fixed in `fdf00323b`)
- [x] 13. solc quirk to document: storage array index from a narrow type is not cleaned by solc
      (see "solc-side observations"; fixed in `b76edd223`)
- [x] 14. Static calldata array passed to an internal function is still validated eagerly (leftover of 11)
      (`symbolic-audit/calldata_static_array_validation.sol`, `passToInternal`; fixed in `12f20df40`)
- [x] 15. Optimizer loses the second element of a `bytes[2] memory` when a helper indexes it
      (`symbolic-audit/bytes_array_element_index_optimized.sol`; fixed in `f563ff5d9`)
- [x] 16. Assembly calldata pointer past `calldatasize`: static array and `bytes` copies must revert like via-IR solc; solar zero-fills (leftover of 3)
      (`symbolic-audit/assembly_calldata_pointer_encode.sol`, `arrayCopy`, `bytesCopy`, `bytesHash`; fixed in `9bc465922`)
- [x] 17. Named arguments are evaluated in declaration order instead of source order
      (`symbolic-audit/named_argument_order.sol`; fixed in `113ca4f22`)
- [x] 18. `0 ** 0` with literal operands evaluates to 0 (regression on `9bc465922`)
      (`symbolic-audit/exp_zero_zero.sol`; fixed in `2ae8f6612`)
- [x] 19. Memory-typed dynamic parameter with an oversized ABI length reverts empty instead of `Panic(0x41)`
      (`symbolic-audit/abi_decode_memory_oversized_length.sol`; fixed in `1bfb1c7ae`)

## Findings

### 1. Integer literal expressions lose arbitrary-precision semantics during lowering

File: `symbolic-audit/literal_addmod_fold.sol`
Source: `testdata/solidity/test/libsolidity/semanticTests/arithmetics/addmod_mulmod.sol`

```solidity
function fold() external pure returns (uint256) {
    return (2**255 + 2**255) % 7;
}
```

`2**255 + 2**255` is part of an integer literal expression. Solidity keeps
literal-only expression trees at arbitrary precision until conversion to a
non-literal type, so the result is `2**256 % 7 == 2`.

Solar's type checker also evaluates these trees with `BigInt` and records the
result as an `IntLiteral`. Normal function lowering does not use that value:
it recursively lowers the leaves as `U256` and each operator as an EVM
instruction. An oversized intermediate therefore wraps as an EVM word or,
where the lowering gives it a checked integer type, traps with `Panic(0x11)`.
The later `% 7` is only the smallest reproducer; the same problem applies to
any literal-only tree whose oversized intermediate is later reduced, divided,
compared, shifted, or otherwise brought back into range.

This fixture takes the checked-arithmetic path and compiles the whole function
to a `Panic(0x11)` revert:

```
fn @fold() [selector=0xe684f46a, pure] {
  bb0:
    tail_call @revert_stub
}
```

| Call | solc | solar |
|------|------|-------|
| `fold()` | returns `2` | reverts `Panic(0x11)` |
| `test()` | returns `0` | reverts `Panic(0x11)` |

Status: intentional divergence. Function-body lowering deliberately retains
EVM-width arithmetic for the individual operators instead of materializing the
type checker's result for the complete literal expression.

### 2. Public array getter out-of-bounds revert data

File: `symbolic-audit/getter_out_of_bounds.sol`
Sources (13 functions across 11 files), for example
`semanticTests/storage/chop_sign_bits.sol`,
`semanticTests/getters/mapping_of_string.sol`,
`semanticTests/storage/accessors_mapping_for_array.sol`,
`semanticTests/array/copying/arrays_from_and_to_storage.sol`.

```solidity
contract GetterOutOfBounds {
    uint256[] public dynamicArray;
    uint256[2] public fixedArray;
}
```

| Call | solc | solar |
|------|------|-------|
| `dynamicArray(0)` on empty array | reverts, empty returndata | reverts `Panic(0x32)` |
| `fixedArray(2)` | reverts, empty returndata | reverts `Panic(0x32)` |

Solc's generated getters use a plain `revert(0, 0)` for an out-of-bounds
index, while ordinary index expressions panic with `0x32`. Solar panics in
both. This is the largest class of mismatches in the run. It affects every
public array or mapping-of-array state variable. It is documented in
`docs/SOLC_DIVERGENCE.md`.

Status: intentional divergence. Getter lowering shares ordinary array-index
lowering, so callers that decode reverts see `Panic(0x32)` rather than solc's
empty revert data.

### 3. Assembly-assigned calldata pointer past `calldatasize`

File: `symbolic-audit/assembly_calldata_pointer_encode.sol`
Source: `tests/ui/codegen/lowering/run-call/abi_calldata_static_struct_validation.sol`

```solidity
function encodeStruct() external pure returns (bytes memory) {
    return abi.encode(makeStruct());
}
function makeStruct() internal pure returns (Pair calldata value) {
    assembly { value := 4 }
}
```

Called with only the 4-byte selector (`0x01e941a3`), so the struct lies
past the end of calldata.

| Call | solc | solar |
|------|------|-------|
| `encodeStruct()` | returns `abi.encode(Pair(0, 0))` | reverts, empty returndata |

Solc reads the struct with `calldataload`, which yields zeros past the end.
Solar emits a calldata bounds check and reverts. The existing UI test only
exercises this entry point with 64 extra calldata bytes, so it passes.

Severity: low. The pointer is assigned in assembly and points outside
calldata, which is outside Solidity's guarantees. Recorded because it is a
concrete behavioral divergence.

Re-check on `1738b4454` (after `361e596e0`): the struct cases agree with
both solc backends. For a static array or `bytes calldata` copied to memory,
including `keccak256(b)`, solc's two backends disagree with each other.
solar currently matches the legacy one. The reference for this audit is the
via-IR backend, so these three are open (item 16):

| Call, calldata = selector only | solc via-IR | solc legacy | solar |
|------|------|------|------|
| `arrayCopy()` returns `uint256[2] calldata` at offset 4 | reverts, empty | `[0, 0]` | `[0, 0]` |
| `bytesCopy()` returns `bytes calldata` at offset 4, length 64 | reverts, empty | 64 zero bytes | 64 zero bytes |
| `bytesHash()` | reverts, empty | hash of zeros | hash of zeros |
| `bytesLength()`, `arrayIndex()`, struct cases | agree | agree | agree |

Checked with solc 0.8.36 under all four combinations of `viaIR` and the
optimizer; the split is by backend, not by optimizer setting. via-IR reverts
because its calldata-to-memory copy for arrays and `bytes` checks
`offset + length <= calldatasize`, while the struct path reads through
`calldataload`.

Second instance: `tests/ui/codegen/lowering/run-call/calldata_struct_return.sol`
`decode(bytes)` with a 1-byte `bytes` argument. The library assigns a struct
pointer with dynamic members to `input.offset`; the member offsets are read
past the end of calldata. solc returns `(true, <struct with zero offsets>)`,
solar reverts with empty returndata.

### 4. Oversized memory array allocation loses the `Panic(0x41)` check

File: `symbolic-audit/memory_array_too_large.sol`
Source: `testdata/solidity/test/libsolidity/semanticTests/array/create_memory_array_too_large.sol`

```solidity
uint256 l = 2**256 / 32;
uint256[] memory x = new uint256[](l);
x[1] = 42;
```

| Call | solc | solar |
|------|------|-------|
| `f()` | reverts `Panic(0x41)` | reverts `Panic(0x32)` |

The MIR shows the folded allocation size is `32` (header only) and the stored
length is `0`: `l * 32` wrapped to zero during constant folding, so the
allocator's size check passes and the later index panics instead:

```
v24 = add v16, 32
...
mstore v16, 0
mstore 0, 0x4e487b71...
mstore 4, 50
```

Severity: miscompile of the panic code. The byte-size computation for a
constant array length must be checked for overflow before folding.

Re-check on `1738b4454` (after `ba539e44a`): fixed. `f()`, `g()` from the
same solc test, `new bytes(huge)`, and runtime lengths from calldata all
agree, for `uint8[]`, `uint256[]`, `uint256[][]`, struct arrays, `bytes`, and
`string`, as long as the length reaches `new` through a local or a constant.
An inline literal length such as `new uint256[](2**256 / 32)` still differs,
but that is CODEGEN-003 (the literal `2**256` wraps to 0 in function
lowering, so solar allocates an empty array where solc panics with 0x41),
not the allocation check. The repro keeps `nested()`, `structs()`, and
`str()` as examples of that overlap.

### 5. Storage-to-memory copy in tuple assignment happens before later lvalues

File: `symbolic-audit/storage_to_memory_tuple_order.sol`
Source: `tests/ui/codegen/lowering/run-call/storage_tuple_aliasing.sol`

```solidity
items[0].value = 10;
(copy, targets[mutateSource()]) = (items[0], 1); // mutateSource sets items[0].value = 99
return (copy.value, items[0].value);
```

| Call | solc | solar |
|------|------|-------|
| `memorySnapshot()` | `(99, 99)` | `(10, 99)` |

Solc evaluates every lvalue before performing any copy, so `copy` sees the
mutated storage. Solar copies storage to memory eagerly. The existing UI test
`storage_tuple_aliasing.sol` asserts solar's `(10, 99)`, so the expectation
itself encodes the divergence.

Severity: observable semantic divergence in evaluation order. Solidity leaves
expression evaluation order unspecified, so this is a compatibility issue
rather than a spec violation.

### 6. Underflowed assembly calldata slice: panic code differs

File: `symbolic-audit/assembly_calldata_slice_underflow.sol`
Source: `tests/ui/codegen/lowering/run-call/forwarded_calldata_slice_return.sol`
(`delegate`, `single`, `nested`)

```solidity
assembly {
    data.offset := add(executionData.offset, 20)
    data.length := sub(executionData.length, 20)
}
```

Called with an empty `bytes`, so `data.length` is `2**256 - 20`. Returning
`data` as `bytes memory` copies it.

| Call | solc | solar |
|------|------|-------|
| `delegate("")` | reverts `Panic(0x41)` | reverts, empty returndata |

Severity: low. The slice is invalid by construction, but solc still produces a
defined panic while solar produces a bare revert.

### 7. `-Onone` returns the derived address instead of the stored value

File: `symbolic-audit/stack_rematerialization_unoptimized.sol`
Source: `tests/ui/codegen/lowering/stack_only_rematerialization.sol`
Settings: Standard JSON `optimizer.enabled=false`, which maps to `-Onone`.
Found only by the no-optimize variant pass; the file's UI test runs with
`-Ogas` only.

```solidity
function first(bool selectFirst) external pure returns (uint256) {
    return storeAtDerivedAddress(0x100, selectFirst);
}
function storeAtDerivedAddress(uint256 base, bool selectFirst) internal pure returns (uint256 result) {
    uint256 address_;
    assembly { address_ := add(base, 32) }
    if (selectFirst) { result = 7; } else { result = 8; }
    assembly { mstore(address_, result) result := mload(address_) }
}
```

| Call | solc | solar |
|------|------|-------|
| `first(true)` | `7` | `0x120` |
| `first(false)` | `8` | `0x120` |

The unoptimized MIR is correct:

```
fn @storeAtDerivedAddress(arg0: u256, arg1: bool) -> u256 [pure] {
  bb0:
    v0 = add arg0, 32
    jumpi arg1, bb1, bb2
  ...
  bb3:
    v1 = phi [bb1: 7], [bb2: 8]
    mstore v0, v1
    v2 = mload v0
    ret v2
}
```

so the defect is in MIR-to-EVM lowering: `v0` crosses the branch and the
store or the load ends up using it as the value. The returned `0x120` is
`base + 32`. With the optimizer on, the module agrees with solc within bounds.

Severity: miscompile (wrong return value) at `-Onone`.

### 8. Unused attached library function as a statement lowers to `INVALID`

File: `symbolic-audit/unused_bound_library_function.sol`
Sources: `testdata/solidity/test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol`,
`testdata/solidity/test/libsolidity/syntaxTests/nameAndTypeResolution/253_using_for_function_exists.sol`

```solidity
library D {
    function double(uint256 self) public pure returns (uint256) { return 2 * self; }
}
contract C {
    using D for uint256;
    function f(uint256 a) external pure {
        a.double;
    }
}
```

| Call | solc | solar |
|------|------|-------|
| `f(0)` | returns normally | `INVALID` (0xFE) |

The bare expression statement references the bound library function without
calling it. solc compiles it to a no-op. Solar's MIR for `f` is:

```
fn @f() [selector=0xb3de648b, pure] {
  bb0:
    v0 = calldatasize
    v1 = lt v0, 36
    jumpi v1, bb1, bb2
  bb2:
    invalid
  bb1:
    revert 0, 0
}
```

Severity: miscompile. A valid program that solc accepts and runs traps at
runtime. Likely the lowering of a bound-library-function value with no call
site falls into an "unsupported expression" path that emits `invalid`.

### 9. Public `bytesN` constant from a hex literal returns the masked memory pointer

File: `symbolic-audit/hex_literal_fixed_bytes_constant.sol`
Sources: `testdata/solidity/test/libsolidity/smtCheckerTests/typecast/string_literal_to_fixed_bytes_constant_initialization_1.sol`
and `_2.sol`

```solidity
bytes4 public constant constantHex = hex"01";
```

| Call | solc | solar |
|------|------|-------|
| `constantHex()` | `0x01000000` | `0x00000000` |

Only the public constant getter is wrong. A local `bytes4 value = hex"01"`,
a string-literal constant `bytes4 public constant c = "a"`, and an explicit
`bytes4(hex"0102")` all agree with solc. The getter MIR allocates a `bytes
memory` object for the literal, then masks the pointer instead of the
contents:

```
mstore v0, 1
mstore v1, 0x0100...00
v2 = and v0, 0xffffffff00...00
mstore 128, v2
returndata 128, 32
```

Severity: miscompile (wrong value). The constant getter lowers the hex
literal with its default `bytes memory` type rather than converting it to the
declared fixed-bytes type.

### 10. Dirty user-defined value type passed as a function argument is treated as clean in the callee

File: `symbolic-audit/udvt_dirty_param.sol`
Found by the value-cleanup probes (assembly-assigned dirty words pushed through
each cleanup boundary in `docs/SOLC_VALUE_CLEANUP.md`).

```solidity
type Small is uint8;
using {eqSmall as ==} for Small global;
function eqSmall(Small a, Small b) pure returns (bool) {
    return Small.unwrap(a) == Small.unwrap(b);
}
function inject(uint256 raw) internal pure returns (Small x) { assembly { x := raw } }
function viaOperator(uint256 raw, uint256 raw2) external pure returns (bool) {
    return inject(raw) == inject(raw2);
}
```

| Call | solc | solar |
|------|------|-------|
| `viaOperator(0x100, 0)` | `true` | `false` |
| `viaCall(0x101, 1)` | `true` | `false` |
| `viaWiden(0x101)` | `1` | `0x101` |
| `viaWidenSigned(0x100)` | `0` | `256` |
| `plainParam(0x100, 0)` with `uint8` parameters | `true` | `true` |
| `noCall(0x100, 0)` with no call | `true` | `true` |

Inside a callee whose parameter has a user-defined value type, solar treats
`Small.unwrap(param)` as an already-clean `uint8`. The inlined MIR for the
operator case is a raw word compare:

```
v5 = eq arg0, arg1
```

while the inline version masks both operands with `and arg, 255`. Plain
integer and `bool` parameters are cleaned correctly. solc never assumes an
internal argument is clean; it cleans at the comparison or widening.

Severity: miscompile. Libraries that build user-defined value types in
assembly from packed words and rely on later cleanup get wrong comparison and
widening results.

### 11. Static calldata array validation is eager in solar, lazy per element in solc

File: `symbolic-audit/calldata_static_array_validation.sol`
Found by the value-cleanup probes with hand-built non-canonical calldata.

```solidity
function readSecond(uint8[2] calldata a) external pure returns (uint8) {
    return a[1];
}
```

Calldata words `[0x101, 0x01]`, so element 0 is not a canonical `uint8`.

| Call | solc | solar |
|------|------|-------|
| `readSecond([0x101, 1])` | returns `1` | reverts, empty returndata |
| `bools([2, 1])` | returns `true` | reverts, empty returndata |
| `readFirst([1, 0x101])` | returns `1` | reverts, empty returndata |
| `unused([0x101, 1])` | returns `2` | returns `2` |
| `copyToMemory([0x101, 1])` | reverts | reverts |

solc validates a static calldata array element when high-level code reads
that element (`docs/SOLC_VALUE_CLEANUP.md`, "Validation of a calldata
composite can be lazy"). solar validates every element of the array on the
first read of any element. Dynamic calldata arrays and calldata structs
already agree with solc.

Severity: low. Only non-canonical ABI input observes it, but solar rejects
calls that solc accepts.

Re-check on `1738b4454` (after `ac68e1e40` "defer static calldata checks"):
direct element reads, nested static arrays, static arrays of structs, static
arrays inside dynamic arrays, `bytes4`, `address`, enum, loops, copies to
memory, and `abi.encode` all agree with solc. One shape is still eager
(item 14): passing the array to an internal function.

```solidity
function passToInternal(uint8[2] calldata a) external pure returns (uint8) { return second(a); }
function second(uint8[2] calldata a) internal pure returns (uint8) { return a[1]; }
```

| Call | solc | solar |
|------|------|-------|
| `passToInternal([0x101, 1])` | `1` | reverts, empty returndata |

### 12. Dirty narrow value used directly as a `new` length or wider mapping key is not masked

File: `symbolic-audit/implicit_widen_alloc_mapping.sol`
Found by the value-cleanup probes. The symbolic executor cannot model a
symbolic allocation size, so this one rests on concrete replays.

```solidity
function newLength(uint256 raw) external pure returns (uint256) {
    uint256[] memory a = new uint256[](inject(raw)); // inject: assembly `x := raw` into uint8
    return a.length;
}
function mappingKey(uint256 raw) external returns (uint256) {
    m[1] = 7;
    return m[inject(raw)]; // mapping(uint256 => uint256)
}
```

| Call | solc | solar |
|------|------|-------|
| `newLength(0x101)` | `1` | `0x101` |
| `newBytesLength(0x101)` | `1` | `0x101` |
| `mappingKey(0x101)` | `7` | `0` |
| `signedMappingKey(0x1ff)` with `mapping(int256 => uint256)` | `7` (key `-1`) | `0` |
| `newLengthViaLocal(0x101)`, widening through a `uint256` local | `1` | `1` |
| `assign`, return, call argument, arithmetic, comparison, `abi.encode` | agree | agree |

solc's IR wraps every implicit `uint8` to `uint256` conversion in
`convert_t_uint8_to_t_uint256`, which masks. solar masks the conversion in
most contexts but not when the narrow expression is consumed directly as the
`new` length (arrays, `bytes`, `string`, nested arrays) or as the key of a
mapping with a wider key type, including nested mappings, mappings in storage
structs, writes, and `delete`. The MIR for `mappingKey` stores `arg0` raw
into scratch before hashing.

Severity: miscompile. A dirty `uint8` from assembly selects a different
mapping slot or allocates a far larger array than solc.

### 15. Optimizer loses the second element of a `bytes[2] memory` when a helper indexes it

File: `symbolic-audit/bytes_array_element_index_optimized.sol`
Source: `testdata/solidity/test/libsolidity/semanticTests/array/copying/calldata_2d_bytes_to_memory_2.sol`
Regression: that test agreed in every earlier pass and mismatches on
`1738b4454`. Under `-Onone` every variant agrees, so an optimization pass is
responsible.

```solidity
function build() internal pure returns (bytes[2] memory m) {
    m[0] = hex"6162";
    m[1] = hex"6162";
}
function sumSecond() external pure returns (uint256) {
    return sumElement1(build());
}
function sumElement1(bytes[2] memory m) internal pure returns (uint256) {
    return uint8(m[1][0]) + uint8(m[1][1]);
}
```

| Call | solc | solar `-Ogas` | solar `-Onone` |
|------|------|---------------|----------------|
| `sumSecond()` | `0xc3` | reverts `Panic(0x32)` | `0xc3` |
| `twoAsserts()` | `1` | reverts `Panic(0x32)` | `1` |
| `sumFirst()` (same reads on element 0) | `0xc3` | `0xc3` | `0xc3` |
| `oneAssert()` (one comparison) | `1` | `1` | `1` |

No calldata is involved; the array is built in memory. The helper's own MIR
is correct and reads element 1's pointer from `m + 32`, then falls into the
"pointer is zero, allocate empty bytes" path and panics on the byte index, so
the pointer store from `build()` is lost before the call. The same shape with
`bytes[]`, `string[2]`, a `bytes[2] calldata` argument copied to memory, or
two comparisons at the same index also fails; two separate `bytes memory`
parameters do not.

Localization (with `target/symaudit/bisect_run.py`, which compiles solar
with explicit `-Z` flags and replays against solc):

| Pipeline | Result |
|----------|--------|
| default `-Ogas` | mismatch |
| `-Onone` | agree |
| `-Zevm-ir-pipeline=none` | mismatch |
| lowering passes only (the `-Onone` MIR list) | agree |
| lowering passes + `cse` after `lower-memory-objects` | mismatch |
| lowering passes + any other single optimization group | agree |

The `cse` diff is semantically neutral: it reuses `v26 = add arg0, 32`
from the block before the "pointer is zero" branch instead of recomputing it
in the branch, and drops `add x, 0`. The final MIR is correct. In the EVM IR
(`-Zdump=evm-ir-runtime -Zevm-ir-pipeline=none`) the failing helper's later
blocks reload that value with `push 352 mload mload`, but no block in the
function ever stores to frame slot 352, so the second `mload v26` reads a
stale slot and the element pointer comes back wrong. The passing helper
(element 0) keeps `arg0` on the stack and re-reads it with `dup`. The defect
is in MIR-to-EVM lowering: a value made live across a branch by `cse` gets a
frame-slot reload without a matching spill store. Same area as finding 7,
reachable at `-Ogas`.

Severity: miscompile (wrong panic on valid code) at the default optimization
level.

### 17. Named arguments are evaluated in declaration order instead of source order

File: `symbolic-audit/named_argument_order.sol`
Found by an evaluation-order probe set (side-effecting `tick(n)` calls that
append to a decimal log) on `9bc465922`.

```solidity
s = S({y: tick(1), x: tick(2)});          // struct literal, storage or memory
uint256 r = take({b: tick(1), a: tick(2)}); // internal call
this.recv({b: tick(1), a: tick(2)});        // external call
```

| Call | solc | solar |
|------|------|-------|
| `structNamed()` | `1221` (log `12`: `y` then `x`) | `2121` (log `21`: `x` then `y`) |
| `memoryStructNamed()` | `1221` | `2121` |
| `callNamed()` | `1221` | `2121` |
| `externalNamed()` | `recv(2, 1)` after log `12` | log `21` |
| `structPositional()`, `callPositional()` | agree | agree |

solc evaluates named arguments in the order they are written and then
permutes them into parameter order; solar evaluates them in parameter order.
Solidity leaves argument evaluation order unspecified, so this is a
compatibility divergence like finding 5. Every other shape in the probe set
agreed: positional arguments, binary operators, short-circuit, index and
compound assignments, tuples, array literals, modifiers, loops, `try`, and
`new` lengths.

Severity: observable semantic divergence when named arguments have side
effects.

### 18. `0 ** 0` with literal operands evaluates to 0

File: `symbolic-audit/exp_zero_zero.sol`
Source: `testdata/solidity/test/libsolidity/semanticTests/expressions/exp_zero_literal.sol`
Regression: agreed on every pass up to `1738b4454`, mismatches on
`9bc465922`.

```solidity
function literal() external pure returns (uint256) { return 0 ** 0; }
```

| Call | solc | solar |
|------|------|-------|
| `literal()` | `1` | `0` |
| `typedBase()` (`uint256 b = 0; b ** 0`) | `1` | `1` |
| `typedExp()` (`uint256 e = 0; 0 ** e`) | `1` | `1` |
| `runtime(0, 0)`, `zeroBase(0)`, `zeroExp(0)`, `2 ** 0`, `1 ** e` | agree | agree |

Only the all-literal `0 ** 0` is wrong, and it is already `ret 0` at
`-Onone`, so the value comes from literal constant evaluation, not from a
pass. `0 ** 0` is `1` in Solidity and in the EVM `EXP` opcode.

Cause: `52c20ab3f` "fold literal expressions" now lowers evaluable literal
binary expressions through `try_eval_const`, and `checked_pow` in
`crates/sema/src/eval.rs` returns the base unchanged when it is zero without
checking for a zero exponent. Before that commit the expression reached the
EVM `EXP` opcode, which gives 1. Fixed in the working tree after
`cef3dd41b` (uncommitted change in `crates/codegen/src/lower/function.rs`); `literal()`
agrees again.

Severity: miscompile (wrong constant).

### 19. Memory-typed dynamic parameter with an oversized ABI length reverts empty instead of `Panic(0x41)`

File: `symbolic-audit/abi_decode_memory_oversized_length.sol`
Found by hand-built malformed ABI encodings on `cef3dd41b`.

```solidity
function u256Array(uint256[] memory a) external pure returns (uint256) { return a.length; }
```

Calldata after the selector: `[0x20, 0xffffffffffffffff]`, or any length at
or above `2**64`, or a head offset such as `0x1f` that makes the length word
read as garbage.

| Parameter type | solc | solar |
|------|------|-------|
| `uint256[] memory`, `uint8[] memory`, `string memory`, `bytes memory`, `uint256[][] memory`, struct with `uint256[]` member | `Panic(0x41)` | reverts, empty returndata |
| `uint256[] calldata` and other calldata-typed parameters | reverts, empty | reverts, empty |

solc's decoder for memory-typed parameters allocates the array and hits the
allocation-size check first; solar rejects the encoding before allocating.
Both reject the call, so only the revert data differs. Well-formed and
otherwise malformed encodings (bad offsets within range, overlapping tails,
short calldata, trailing data) agreed for calldata parameters, `abi.decode`
from `bytes`, `try`/`catch` decoding of malformed revert and return data,
modifier argument order, and internal and external function pointers.

Severity: low, revert-data only.

## solc-side observations

Divergences where solc, not solar, departs from the rule in
`docs/SOLC_VALUE_CLEANUP.md`. They still change observable behavior between
the compilers, so they belong in `docs/SOLC_DIVERGENCE.md` rather than in a
fix.

### 13. solc does not clean a narrow storage array index

Probe: `target/symaudit/cleanup/dirty_widen.sol` `storageIndex`,
`storageFixedIndex`, and `dirty_misc.sol` `popToDirtyIndex`.

```solidity
uint256[] sarr;
function storageIndex(uint256 raw) external returns (uint256) {
    sarr.push(1); sarr.push(2); sarr.push(3);
    return sarr[inject(raw)]; // uint8 index
}
```

| Call | solc | solar |
|------|------|-------|
| `storageIndex(0x101)` | reverts `Panic(0x32)` | `2` |
| `storageIndex(0x100)` | reverts `Panic(0x32)` | `1` |
| `memIndex(0x101)` on a memory array | `9` | `9` |

solc's IR calls `storage_array_index_access(slot, index)` with the `uint8`
value directly, with no `convert_t_uint8_to_t_uint256`, so the bounds check
sees the dirty word. For memory arrays solc does emit the conversion. solar
masks the index in both cases. The "widening cleans according to the source
width" rule in the cleanup notes does not hold for solc storage indexing.

Known upstream: argotorg/solidity#15142 "Unsafe type conversions with the IR
codegen" (open, labeled bug, reported May 2024 with the same `uint40` index
shape) and #15519 "viaIR pipeline does not clean uints before indexing
storage arrays" (closed as a duplicate of #15142 in October 2024). Not fixed
in 0.8.36: `IRGeneratorForStatements::endVisit(IndexAccess)` passes the raw
index variable for storage arrays while memory and calldata arrays go through
`expressionAsType(..., uint256)`.

### Third session (2026-09-02, later)

Regression and discovery on `1738b4454`, `9bc465922`, `cef3dd41b`, and
`e6db78e6b` as fixes landed. Findings 14 to 19 came from this session and
are all fixed. Angles covered, each under `-Ogas` and `-Onone` symbolically
and with concrete edge values:

| Angle | Probe file(s) in `symbolic-audit/probes/` | Result |
|-------|------|--------|
| Evaluation order of side effects (arguments, operators, short-circuit, indices, tuples, literals, modifiers, loops, `try`) | `eval_order.sol` | finding 17 (fixed) |
| Storage `bytes`/`string` across the 31/32/33 short-long boundary | `storage_bytes.sol` | agree |
| Packed and standard ABI encoding of narrow arrays, nested arrays, structs | `packed_encoding.sol` | agree |
| Malformed ABI input through calldata parameters and `abi.decode` | `abi_malformed.sol` | finding 19 (fixed) |
| `try`/`catch` decoding of malformed `Error`, `Panic`, custom, and return data | `try_catch_decode.sol` | agree |
| Modifier argument order, internal and external function pointers | `modifier_fnptr.sol` | agree |
| Literal-only expression folding (32 integer shapes) | `literal_folds.sol` | finding 18 (fixed) |
| Inline assembly opcode semantics and Yul control flow | `yul_ops.sol` | agree (one probe clobbered the free-memory pointer itself) |
| Environment builtins, string literal escapes, `type(...)`, overload resolution | `env_literals_overloads.sol` | agree |
| Transient storage, including narrow types and assembly `tload`/`tstore` | `transient_storage.sol` | agree |
| Stack pressure: 24 generated functions with 8 to 22 live values across branches and loops | `stack_pressure.sol` | agree under gas, none, and size |
| Storage and memory aliasing, reference stability across push/pop, parameter references | `aliasing.sol` | agree |
| Dispatcher on short, odd-length, and unknown-selector calldata, with and without fallback/receive | `dispatch.sol` | agree |

The full corpus under `-Ogas` and `-Onone` on `e6db78e6b` shows only the
reviewed non-bugs (self-address, free-memory-pointer clobbering). solar still
rejects rational literals such as `0.5 * 4` at compile time; that is a
support gap, not a codegen divergence.

The scratch tooling used for these runs is copied to `symbolic-audit/tools/`
(campaign runner, concrete and flag-driven replay, prefix-task generator,
triage, and the repro recheck script). It expects to live under
`target/symaudit/` and is kept here only for reference.

## Value-cleanup probe set

`symbolic-audit/probes/` holds the probe contracts behind findings 10 to 13.
Each function takes `uint256 raw`, injects it into a narrow type with
`assembly { x := raw }` (or writes it raw into storage, memory, or return
data), and pushes it through one cleanup boundary from
`docs/SOLC_VALUE_CLEANUP.md`: ABI encode and decode, comparisons, checked and
unchecked arithmetic, shifts, conversions, memory and storage round trips,
mapping keys, enums, user-defined value types, external function values,
function parameters, allocation lengths, and ABI validation of external inputs
and return data. About 330 functions, each run symbolically under `-Ogas`,
`-Onone`, and `-Osize`, and concretely with 10 to 18 dirty words. Everything
not listed above agreed with solc.

## Reviewed and not bugs

These mismatches are real output differences but come from
implementation-defined memory layout or memory-unsafe assembly, where the
compilers are allowed to differ.

- `tests/ui/codegen/lowering/multi_return_scratch.sol` `assign(uint256,uint256)`
  and `tests/ui/codegen/lowering/mir_alloc_ops.sol` `rawAssembly()` return
  the raw free-memory pointer. Solc starts at `0x80`; solar's differs.
- `tests/ui/codegen/lowering/multi_return_fmp_clobber.sol` `run(uint256)`
  writes `not(0)` into the free-memory pointer from assembly. Solc then runs
  out of gas on the next allocation; solar never reads the pointer again and
  returns normally.
- `semanticTests/functionTypes/selector_expression_side_effect.sol` `h()`
  returns `this`. The two runtimes live at different addresses. Same for
  external function pointers (`abi_encode_call_is_consistent_v2.sol`,
  `function_array_cross_calls.sol`, `external_function_pointer_nested_array.sol`),
  which embed the contract address.
- `tests/ui/codegen/lowering/creation_code.sol`, `runtime_code.sol`, and
  `program-data/codesize_data.sol` return or measure the contract's own code.
- `tests/ui/codegen/lowering/run-call/proxy_clobbered_local.sol` `keepsLocal`
  runs `calldatacopy(0, 0, calldatasize())` over the free-memory pointer. solc
  then runs out of memory gas encoding the return; solar returns normally.
- `tests/ui/codegen/lowering/run-call/external_call_returndata_size.sol`
  returns the free-memory-pointer delta of an external call.
- `semanticTests/various/create_random.sol` returns addresses derived from
  `address(this)` through `create` and `create2`.
- Value-cleanup probes: `f.address := raw` in assembly followed by an assembly
  read of `f.address` shows the raw word in solc and a masked word in solar.
  Assembly reads of dirty locals are implementation-defined. A probe that
  OR-ed a symbolic word into `f.address` before `f == g` differed only because
  the two deployment addresses have different low bits.

## Campaign statistics

Three passes over the same corpus, each with different compiler settings.
The `incomplete` column covers solver and path limits, forge timeouts,
abstract contracts with no runtime, unresolved libraries, and function inputs
the symbolic executor does not model.

| Pass | Settings | Checked | Agreement | Incomplete | Mismatch |
|------|----------|---------|-----------|------------|----------|
| osaka | `evmVersion=osaka`, optimizer on | 3877 | 3327 | 507 | 43 |
| paris | `evmVersion=paris`, optimizer on | 3943 | 3412 | 489 | 42 |
| noopt | `evmVersion=osaka`, optimizer off | 4035 | 3475 | 517 | 43 |

44 unique functions mismatched across all passes. By class: 27 are finding 2,
2 are finding 1, 1 is finding 3, 2 are finding 4, 1 is finding 5, 3 are
finding 6, 1 is finding 7, and 4 are the reviewed non-bugs above. Finding 7
appears only in the `noopt` pass.

Raw per-function results are in `target/symaudit/results*.jsonl`; the
campaign and replay scripts are in `target/symaudit/`.

### Second session (2026-09-02)

New corpora and settings, about 29,500 further function checks. Findings 8
and 9 came from this session; everything else was a repeat of findings 1 to 7
or a reviewed non-bug.

| Lane | Corpus | Settings | Checked | Mismatch (unique) |
|------|--------|----------|---------|-------------------|
| size | UI codegen + solc semantic tests | optimizer runs=1 (`-Osize`) | 4035 | 31 |
| extra | `tests/ui` (non-codegen), `tests/foundry`, fuzz corpora | default | 752 | 32 |
| solsmith, solsmith2 | 1200 SolSmith programs, with and without `setup` prefix | default | 6000 | 0 |
| fandango | 56 grammar-generated runtime harnesses | default | 280 | 0 |
| wide | solc semantic tests skipped earlier (self-calls, `new`, payable) | default | 1760 | 13 |
| syntax | solc syntax, cmdline, and gas tests | default | 1151 | 3 |
| smt | solc SMT checker and AST JSON tests | default | 1537 | 5 |
| multisrc | 257 multi-source solc tests, split into projects | default | 206 | 0 |
| noopt2, size2 | syntax, SMT, cmdline, `tests/ui`, `tests/foundry` | `-Onone`, `-Osize` | 6706 | 80 |
| prefix lanes | solc semantic-test call sequences as concrete prefixes, symbolic tail | default, `-Onone`, paris, `-Osize` | 6939 | 168 |
| retry | earlier incompletes with 8192 paths and 60 s solver timeout | default | 198 | 0 |

The prefix lanes replay each semantic test's expectation sequence (integer,
bool, string, and hex literal arguments; calls expected to `FAILURE` are
dropped since they leave no state) and then symbolically compare the next
function. Every prefix mismatch was a public array getter called out of
bounds (finding 2) or an address derived from `address(this)`.

## Reproducing

```bash
cargo build -p solar-compiler --bin solar
fuzz/bin/solsymdiff --source symbolic-audit/literal_addmod_fold.sol \
  --contract LiteralAddmodFold --signature 'fold()'
fuzz/bin/solsymdiff --source symbolic-audit/getter_out_of_bounds.sol \
  --contract GetterOutOfBounds --signature 'dynamicArray(uint256)' --include-view
fuzz/bin/solsymdiff --source symbolic-audit/assembly_calldata_pointer_encode.sol \
  --contract AssemblyCalldataPointerEncode --signature 'encodeStruct()'
fuzz/bin/solsymdiff --source symbolic-audit/memory_array_too_large.sol \
  --contract MemoryArrayTooLarge --signature 'f()'
fuzz/bin/solsymdiff --source symbolic-audit/storage_to_memory_tuple_order.sol \
  --contract StorageToMemoryTupleOrder --signature 'memorySnapshot()' --include-stateful
fuzz/bin/solsymdiff --source symbolic-audit/assembly_calldata_slice_underflow.sol \
  --contract AssemblyCalldataSliceUnderflow --signature 'delegate(bytes)'
fuzz/bin/solsymdiff --source symbolic-audit/stack_rematerialization_unoptimized.sol \
  --contract StackRematerializationUnoptimized --signature 'first(bool)' --no-optimize
fuzz/bin/solsymdiff --source symbolic-audit/unused_bound_library_function.sol \
  --contract UnusedBoundLibraryFunction --signature 'f(uint256)'
fuzz/bin/solsymdiff --source symbolic-audit/hex_literal_fixed_bytes_constant.sol \
  --contract HexLiteralFixedBytesConstant --signature 'constantHex()' --include-view
fuzz/bin/solsymdiff --source symbolic-audit/udvt_dirty_param.sol \
  --contract UdvtDirtyParam --signature 'viaOperator(uint256,uint256)'
# Finding 11 needs non-canonical calldata; the symbolic harness only builds
# canonical typed inputs. Call `readSecond` with words `[0x101, 1]` directly.
# Finding 12 needs a concrete call: `newLength(0x101)` and `mappingKey(0x101)`.
fuzz/bin/solsymdiff --source symbolic-audit/bytes_array_element_index_optimized.sol \
  --contract BytesArrayElementIndexOptimized --signature 'sumSecond()'
fuzz/bin/solsymdiff --source symbolic-audit/named_argument_order.sol \
  --contract NamedArgumentOrder --signature 'structNamed()' --include-stateful
# Finding 19 needs malformed calldata: call `u256Array` with words `[0x20, 0xffffffffffffffff]`.
```

Exit status 1 with `"status": "mismatch"` and the counterexample calldata in
the printed JSON.
