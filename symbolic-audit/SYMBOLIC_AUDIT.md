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

Items 1 to 22 are fixed in code; 1 to 19 were re-verified against `e6db78e6b`
on 2026-09-02, 20 against `e9ca037d5`, 21 against `e1693d1ba`, and 22
against `96551139a` on 2026-09-03, with the repros below (symbolic where the executor can model the
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
- [x] 20. Library functions with storage parameters or non-view mutability appear in the ABI, and a mapping in that parameter type is an ICE
      (`symbolic-audit/library_storage_param_abi.sol`; fixed in `89d91da6a, 190467599`)
- [x] 21. `payable` library functions are accepted; solc rejects them with error 7708
      (`symbolic-audit/payable_library_function.sol`; fixed in `3312f3346`)
- [x] 22. Five more declaration checks solc applies to libraries and `payable` are missing
      (`symbolic-audit/library_declaration_checks.sol`; fixed in `96551139a`)
- [x] 23. Checked arithmetic in a base-constructor argument is lowered unchecked
      (`symbolic-audit/base_constructor_arg_overflow.sol`; fixed in `fadddd133`, hardened in `9c9bfbcc0`)
- [x] 24. Storage-reference argument to a base constructor lowers to `invalid` or is rejected
      (`symbolic-audit/base_constructor_storage_arg.sol`; fixed in `5967d1929`)
- [x] 25. Five valid programs from the solc semantic tests are rejected by the type checker
      (see the item; the repros are the upstream test files; fixed in `b8b912c89`, `d37f90e4f`, `32a1778b9`, `b36cba5e5`, `af388943c`, and `40d523c80` for the transient gap)
- [x] 26. Indexing a storage `bytes` loads the whole array into memory, making indexed loops quadratic in gas
      (`symbolic-audit/storage_bytes_index_gas.sol`; fixed in `92a20464d`)
- [x] 27. Pre-byzantium external calls with return values have no `extcodesize` guard, so a code-less callee returns zeros
      (`symbolic-audit/external_call_prebyzantium.sol`; fixed in `49cf1b51d`, tests in `46b9d61f8`)
- [x] 28. `push`, `push()`, and `pop` on a storage `bytes` rewrite the whole value, so loops of them are quadratic in gas
      (`symbolic-audit/storage_bytes_push_gas.sol`; fixed in `8f2e55ba`, review fix `3d5d21f8`)
- [x] 29. At homestead every external call forwards `gas()`, which a pre-EIP-150 EVM rejects, so every external call runs out of gas
      (`symbolic-audit/external_call_gas_prebyzantium.sol`; fixed in `5aebf672`, review fix `f675c760`)
- [x] 30. `try`/`catch` with a bare `catch { }` is rejected before byzantium because the catch path emits `RETURNDATACOPY`
      (`symbolic-audit/try_catch_prebyzantium.sol`; fixed in `af297b6b`, review fix `813b6632`)
- [x] 31. External library calls with return values are rejected before byzantium instead of using a static output buffer
      (`symbolic-audit/library_call_prebyzantium.sol`; fixed in `0e722706`, tests in `a5b57877`)
- [ ] 32. Loop-carried values round-trip through memory frame slots on every iteration, so tight loops cost 1.25x to 1.7x solc
      (`symbolic-audit/loop_carried_frame_slots.sol`)
- [x] 33. `this.f()` in an internal function reached from the constructor skips the `extcodesize` guard, so deployment succeeds where solc reverts
      (`symbolic-audit/this_call_from_constructor_helper.sol`; fixed in `e638b191`, tests in `ffb4d4bf`)
- [ ] 34. Any recursive internal function is rejected before constantinople by the post-legalization stack verifier
      (`symbolic-audit/recursion_preconstantinople.sol`)
- [ ] 35. `virtual` free functions and non-`external` interface functions are accepted; solc rejects them with errors 4493 and 1560
      (`symbolic-audit/interface_free_function_checks.sol`)
- [x] 36. `-Ogas` regression: a loop assigning a tuple after a branch returns a wrong value (`stack_pressure.sol` `f2`, `f12`)
      (`symbolic-audit/loop_tuple_assign_miscompile.sol`; fixed in `daaf2d05`, tests in `ae814158`)
- [ ] 37. `a.push()` used as a value on a non-bytes storage array returns 0 instead of the appended element
      (`symbolic-audit/storage_array_push_rvalue.sol`)
- [ ] 38. At `-Ogas` the backend puts a jump between homestead's `sub(gas(), 50)` and a shared CALL block, so the reserve is short and the call throws
      (`symbolic-audit/call_gas_reserve_split.sol`)
- [ ] 39. A pre-byzantium external call whose dynamic return value is unused is rejected; solc compiles it
      (`symbolic-audit/dynamic_return_unused_prebyzantium.sol`)

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

### 20. Library functions with storage parameters appear in the ABI, and a mapping in that parameter type is an ICE

File: `symbolic-audit/library_storage_param_abi.sol`
Found on `decbf7fcf` by the fourth-session library probe
(`symbolic-audit/probes/libs_fnptr.sol`): the campaign compiles every file
through Standard JSON with `abi` in the output selection, and solar crashed
on the whole file.

```solidity
library L {
    struct Set { mapping(uint256 => uint256) idx; }
    struct Plain { uint256 a; }
    function withMapping(Set storage s) external view returns (uint256) { return s.idx[0]; }
    function plainStruct(Plain storage p) external view returns (uint256) { return p.a; }
    function mappingParam(mapping(uint256 => uint256) storage m) external view returns (uint256) { return m[0]; }
    function arrParam(uint256[] storage a) external view returns (uint256) { return a.length; }
}
```

| Output | solc | solar |
|------|------|-------|
| `--abi` for the library above | `[]` | ICE: `printing unsupported type as ABI: Mapping(...)` at `crates/sema/src/ty/print.rs:152` |
| `--abi` with only `plainStruct` and `arrParam` | `[]` | lists both, `plainStruct` as a `tuple` parameter |
| `--hashes` | four signatures with `storage` | agree |
| `--abi` for a library with `pure`, `view`, `nonpayable`, and `public` functions | only the `pure` and `view` ones | all four |

solc leaves every library function with a storage-reference parameter or
return, and every library function whose mutability is above `view`, out of
the JSON ABI (`libsolidity/interface/ABI.cpp`, the `isLibrary()` check in
`ABI::generate`), while keeping them all in the selector list. solar's ABI writer includes them and
prints the parameter as if it were a memory value; when the storage type
contains a mapping there is no ABI spelling and the printer asserts.
Not a codegen divergence: the selectors and the generated code agree, only
the emitted ABI JSON differs, and the crash aborts the whole compilation.

Severity: ICE on valid code plus an ABI JSON divergence.

Re-check on `e9ca037d5` (after `89d91da6a` and `190467599`): `--emit abi`
prints `[]` for the repro and lists only the `pure` and `view` functions of
the mixed-mutability library, matching solc; `--emit hashes` is unchanged.
The review of the fix turned up item 21.

### 21. `payable` library functions are accepted

File: `symbolic-audit/payable_library_function.sol`
Found while reviewing the fix for 20.

```solidity
library P {
    function f() external payable {}
    function g() public payable returns (uint256) { return msg.value; }
}
```

| Input | solc | solar |
|------|------|-------|
| the library above | error 7708 `Library functions cannot be payable.` on each function | compiles |

solc rejects the declaration in `TypeChecker::visit(FunctionDefinition)`.
Libraries are only reached by `DELEGATECALL`, so `msg.value` refers to the
caller's value and the modifier is meaningless. Not a codegen divergence:
solar accepts a program solc refuses.

Severity: missing diagnostic.

Re-check on `e1693d1ba` (after `3312f3346`): both library functions get
error 7708 and contract `C` still compiles. The review turned up item 22.

### 22. Five more declaration checks solc applies to libraries and `payable` are missing

File: `symbolic-audit/library_declaration_checks.sol`
Found while reviewing the fix for 21.

```solidity
library L {
    constructor() {}
    fallback() external {}
    function v() public virtual {}
}
contract C {
    function p() internal payable {}
    function q() private payable {}
}
function free() payable {}
```

| Declaration | solc | solar |
|------|------|-------|
| constructor in a library | error 7634 `Constructor cannot be defined in libraries.` | compiles |
| `fallback` in a library | error 5982 `Libraries cannot have fallback functions.` | compiles |
| `virtual` library function | error 7801 `Library functions cannot be "virtual".` | compiles |
| `internal` or `private` `payable` function | error 5587 `"internal" and "private" functions cannot be payable.` | compiles |
| `payable` free function | error 9559 `Free functions cannot be payable.` | compiles |

All five live next to the 7708 check in `TypeChecker::visit(FunctionDefinition)`
and `TypeChecker::visit(ContractDefinition)` in solc. Not codegen divergences:
solar accepts programs solc refuses.

Severity: missing diagnostics.

Re-check on `96551139a`: the repro reports the six errors solc reports, with
5587 twice, and the review found the payable and `virtual` else-if chains
match solc on every function form. The review listed further diagnostic gaps
that are outside this audit's codegen scope and are not tracked as items:
the "libraries cannot have receive ether functions" message lacks solc's
code 4549; 7775 (only pure free functions define operators), 7001
(`constructor() virtual`, a parse error here), 4493 (`virtual` free
function), 1560 (interface function visibility), and warning 5815 are not
emitted; and the `Contract::functions()` doc comment in
`crates/sema/src/hir/mod.rs` says it excludes the constructor and fallback
while the lowering pushes every function into `items`.

### 23. Checked arithmetic in a base-constructor argument is lowered unchecked

File: `symbolic-audit/base_constructor_arg_overflow.sol`
Found on `023e758e4` by the new stateful Foundry differential
(`symbolic-audit/tools/statediff.py`), which deploys both compilers'
creation code with the same constructor arguments; the symbolic tool never
runs constructors, so this path was unexplored.

```solidity
contract Base { uint256 public x; constructor(uint256 v) { x = v; } }
contract BaseArgAdd is Base { constructor(uint256 v) Base(v + 1) {} }
```

| Deployment | solc | solar |
|------|------|-------|
| `BaseArgAdd(2**256 - 1)` | reverts `Panic(0x11)` | deploys, `x() == 0` |
| `BaseArgMul(2**255)` with `Base(v * 2)` | reverts `Panic(0x11)` | deploys, `x() == 0` |
| `Base(bump(v))` with the add inside `bump` | reverts | reverts |
| `x = v + 1` in the constructor body | reverts | reverts |
| modifier argument `m(v + 1)` on a constructor | reverts | reverts |

The constructor MIR shows the argument as a plain `add arg0, 1` followed by
the `sstore`, with none of the overflow check that the same expression gets
in a function body. Only expressions written directly in the inheritance
specifier's argument list are affected.

Severity: miscompile. A checked operation silently wraps.

Cause and fix (`fadddd133`): the type checker validated base-constructor
arguments with the inner routine that computes a type without registering
it, so the outermost argument expression had no entry in the expression
type table. Codegen's binary and unary lowering reads that table to choose
the checked-arithmetic kind and, on a missing entry, emitted the raw opcode.
The fix routes the arguments through the registering entry point. The
review (`9c9bfbcc0`) adds an assertion in codegen that every binary and
unary operand has a registered type, so a missing type is an ICE rather
than a silent unchecked operation; it fires nowhere across the UI suite, the
Foundry projects, the solc test modes, or the sweeps. The review also found
and fixed three adjacent defects in the same checker routine: named
base-constructor arguments were paired positionally (`b824ac478`), arguments
to a base with no constructor were accepted (`f028879d3`), and the new UI
test gained subtraction, exponentiation, and an unchecked-shift case
(`f8c4c1a6e`). Re-verified on `f028879d3` with the stateful harness: both
overflow deployments revert on both compilers and the in-range deployments
agree.

### 24. Storage-reference argument to a base constructor lowers to `invalid` or is rejected

File: `symbolic-audit/base_constructor_storage_arg.sol`
Sources: `semanticTests/types/struct_mapping_abstract_constructor_param.sol`,
`semanticTests/types/array_mapping_abstract_constructor_param.sol`
Found by the stateful Foundry differential sweep over the solc semantic
tests on `023e758e4`.

```solidity
struct S { mapping(uint256 => uint256) m; }
abstract contract A { constructor(S storage s) { s.m[5] = 16; } }
contract C is A {
    mapping(uint256 => S) m;
    constructor() A(m[1]) {}
}
```

| Deployment | solc | solar |
|------|------|-------|
| `C()` then `getM(1, 5)` | deploys, returns `16` | deploys revert: the constructor MIR is `callvalue` check then `invalid` |
| `D()` with `mapping(...)[] storage` parameter and `m.push()` | deploys, `m(1, 0, 1)` returns `2` | before `fadddd133`: compiles, deployment reverts; after: compile error `codegen rewrite does not support this storage access yet` |

solc passes a storage pointer to a base constructor as its slot number, the
same way it passes storage pointers to internal functions. solar's lowering
of the inheritance-specifier argument does not handle reference-typed
storage arguments: the struct case falls into the "unsupported expression"
path that emits `invalid` at runtime without a diagnostic (the same class as
finding 8), and the array case is now rejected at compile time.

Severity: miscompile (silent `INVALID` on valid code) plus a support gap.

Cause and fix (`5967d1929`): the inlined base constructor bound every
parameter as a plain value, while a function's own storage parameters are
bound as storage references, so the base body could not resolve `s` or `m`
as storage and the whole function lowering bailed. For `C` the bail was
silent and the contract lowering substituted an `invalid` body; for `D` the
`push` path reported. The argument side also loaded storage arguments as
values (a memory copy for `uint256[] storage` and `bytes storage`) instead
of passing the slot. Both now go through the path internal calls and
modifier arguments use. The contract lowering also no longer replaces a
failed function with `invalid` unless a diagnostic was emitted, closing the
silent-`INVALID` class of finding 8 for good. Re-verified on `5967d1929`
with the stateful harness: `C` and `D` deploy and `getM(1, 5)` returns 16
on both compilers. The review (`4e946a961`) probed name collisions between
base parameters and derived state, diamonds, forwarding, runtime-indexed
and ternary storage arguments, and argument evaluation order against solc,
all agreeing, and stopped the new bail-out diagnostic from cascading after
an unrelated type error.

### 25. Five valid programs from the solc semantic tests are rejected by the type checker

Found by the stateful sweep over `semanticTests/`, which compiles every file
with both compilers: solc accepts these, solar reports an error. They are
front-end acceptance gaps, not codegen divergences, and are grouped here for
triage.

| Upstream test | Construct | solar error |
|------|------|------|
| `array/copying/nested_array_storage_to_memory.sol` | `a3.push([1, 2])` on `uint256[2][] storage` | `no matching member push found on type uint256[2][] storage` |
| `array/slices/array_slice_calldata_to_memory.sol` | `[b[start:end]][0][0]`, an array literal of calldata slices | `cannot infer nameable array element type` |
| `constantEvaluator/rounding.sol` | `uint[c] memory x` with an `int` constant `c` as the length | `mismatched types: expected uint256, found int256` |
| `errors/named_parameters_shadowing_types.sol` | `error E2(EnumType StructType, StructType EnumType)` | `name has to refer to a valid user-defined type` |
| `functionTypes/stack_height_check_on_adding_gas_variable_to_function.sol` | `this.g{gas: 42}.address` | `call options must be part of a call expression` |

The reverse direction also appears once: solar accepts `bytes transient`
and `string transient` state variables and lowers their `.length` and
`t[i]` against regular storage, while solc rejects the declaration with
"Transient data location is only supported for value types" (found by the
finding 26 review).

`cargo tq solc-solidity` does not catch these because it compares only parser
errors.

Severity: valid programs rejected.

Fixes, one commit each: `push` of an array literal into a storage array
uses solc's relaxed storage-copy conversion and nested literals receive the
expected element type (`b8b912c89`); calldata slices mobilize to memory
arrays in array literals (`d37f90e4f`); array lengths accept any integer
constant and leave the value checks to the evaluator (`32a1778b9`); error,
event, and bodiless-function parameter names no longer shadow the types of
sibling parameters, matching solc's `isVisibleAsUnqualifiedName`
(`b36cba5e5`); call options before a member access are a HIR node whose
options are evaluated for side effects and discarded (`af388943c`); and
reference types in transient storage are rejected with solc's error 1834
(`40d523c80`). All five upstream tests compile on `40d523c80`, and the
transient probe reports error 1834. The review ran about 70 accept/reject
probes against solc and fixed four more gaps it turned up: slicing a
static calldata array was accepted (`5a249c59b`), the name-hiding change
had made bodiless-modifier and `try` clause parameters invisible
(`bcb1d1247`), an implemented function's parameter name did not shadow its
own type as it does in solc (`8ee9a58c0`), and `this.g{gas: 5};` as a bare
statement was rejected (`cb8683cf5`). It documented the one remaining
divergence in this area, nested array literals assigned to a wider memory
array, as `TYPECK-003` in `docs/SOLC_DIVERGENCE.md`.

### 26. Indexing a storage `bytes` loads the whole array into memory

File: `symbolic-audit/storage_bytes_index_gas.sol`
Source: `semanticTests/array/copying/bytes_storage_to_storage.sol`, which
under `-Onone` ran out of the harness's 20M gas in solar for `f(914)` while
solc used 4M; with a 500M cap both agree, so the divergence is gas, not
values. Measured at `-Ogas` after storing 914 bytes:

| Call | solc gas | solar gas | ratio |
|------|------|------|------|
| `readOne(5)`, one `a[i]` | 25 945 | 87 547 | 3.4 |
| `readAll()`, `a[i]` in a loop over 914 | 703 959 | 17 264 688 | 24.5 |
| `writeAll()`, `a[i] = ...` in a loop | 998 031 | 23 045 244 | 23.1 |
| `fill(914)`, memory to storage copy | 832 070 | 788 481 | 0.9 |

The MIR for `readOne` shows why: `a[i]` is lowered as
`internal_call @load_storage_bytes` (copy the whole `bytes` to memory),
`mload` the length for the bounds check, then `mload`/`byte` on the copy.
solc reads the length slot, checks the bound, and loads the single data
slot `keccak(slot) + i / 32`. Every index of a long storage `bytes` therefore
costs a full copy (29 cold `sload`s for 914 bytes), and loops over the array
are quadratic. Writes behave the same way. `uint8[]` and `uint256[]`
storage arrays are indexed directly and are not affected.

Severity: gas. Correct results, but 3x to 25x the gas of solc on storage
`bytes` element access, and callers with tight gas limits see reverts solc
does not produce.

Fix (`92a20464d`): element access on storage `bytes` and `string` now
resolves to one storage word the way solc does (decode the header slot,
bounds-check, then `keccak(slot) + i / 32` for long values or the header
slot itself for short ones) and reuses the packed-storage read and
read-modify-write machinery; `.length` reads the header instead of copying.
Re-measured on `92a20464d` after storing 914 bytes:

| Call | solc gas | solar gas | ratio |
|------|------|------|------|
| `readOne(5)` | 25 945 | 25 808 | 0.99 |
| `readAll()` | 703 959 | 623 467 | 0.89 |
| `writeAll()` | 998 031 | 977 873 | 0.98 |

The UI codegen corpus shrinks 1.0% at `-Ogas` and 1.7% at `-Osize` in
runtime size; the probe set `storage_bytes.sol` still agrees with solc on
random multi-call sequences. The review (`ca8fe0247`) ran about 1100
differential calls over encoding edges, dirty headers (`Panic(0x22)` before
`Panic(0x32)`, as in solc), tuple assignments, side-effecting indices,
storage-pointer parameters, and all three optimization levels, all
agreeing, and pinned the dirty-header cases in a UI test. It noted that the
inlined header decode costs bytecode size on copy-heavy contracts
(`storage_struct_dynamic_copy`, `storage_copy_recursive_struct`) even
though the corpus total shrinks; outlining the decode is a possible
follow-up.

### 27. Pre-byzantium external calls with return values have no `extcodesize` guard

File: `symbolic-audit/external_call_prebyzantium.sol`
Source: `semanticTests/array/array_function_pointers.sol` (`g(811, 1)` calls a
zero-initialized external function pointer),
`abicoder/calldataDecoding/array/calldata_array_function_types_v2.sol`,
`abicoder/calldataDecoding/struct/member_external_function_v2.sol`.
Found by the stateful sweep at `--evm-version homestead`; the same three
tests agree from byzantium on.

```solidity
interface I { function f() external returns (uint256); function g() external; }
contract R {
    function callRet(address a) external returns (uint256) { return I(a).f(); }
    function callNoRet(address a) external { I(a).g(); }
}
```

| Call, target has no code | solc (homestead, tangerineWhistle, spuriousDragon) | solar |
|------|------|------|
| `callRet(address(0))`, `callView`, `callPtr` (return values) | reverts | succeeds, returns 0 |
| `callNoRet(address(0))`, `callPtrNoRet` (no return values) | reverts | reverts |
| all of the above from byzantium on | agree | agree |

Before byzantium there is no `RETURNDATASIZE`, so solc guards every external
call with `if iszero(extcodesize(target)) { revert }` (visible in
`--ir-optimized` at homestead); from byzantium on it drops the guard for
calls with return values because the return-data length check subsumes it.
solar's homestead lowering of a call with return values emits neither: the
MIR is `call ...; revert 0, 0` on failure and `mload` of the untouched output
buffer on success, so a missing contract yields zeros instead of a revert.
Calls without return values keep the `extcodesize` check on both compilers.

Severity: miscompile for the three pre-byzantium targets. A call to a
non-existent contract that returns a value silently succeeds.

Cause and fix (`49cf1b51d`): the call lowering emitted the `extcodesize`
guard only when the call had no return values, at every EVM version,
while solc's condition is `encodedHeadSize == 0 || !supportsReturndata()`
(`checkExtcodesize` in `IRGeneratorForStatements.cpp`). The fix adds
`needs_code_check(returns) = returns == 0 || !supports_returndata()` at the
four call sites (direct call, external function pointer, library
delegatecall, `try`). The same commit also fixes a second pre-byzantium
defect it exposed: the short-return-data check compared the expected size
against a constant zero `RETURNDATASIZE`, so any call returning a static
aggregate reverted unconditionally at homestead; the comparison is now
skipped where `RETURNDATASIZE` does not exist and the returned words are
validated at every version. Re-verified with the stateful harness: the
repro agrees at homestead, tangerineWhistle, spuriousDragon, byzantium, and
osaka, also with `--no-optimize`; before the fix the same command reported
`callRet(address(0))` as success on our side and failure on solc's. The
review (`46b9d61f8`) diffed guard presence against solc over ten call
shapes at ten EVM versions with every cell agreeing, probed live callees
returning one, two, aggregate, struct, view, and dirty-word results at
homestead and osaka, and pinned the guard with FileCheck lines in
`external_view_call_evm_version.sol` and a `tangerineWhistle` revision of
the new run-call test. It left one pre-byzantium difference alone: when a
live callee returns fewer words than declared, both compilers read stale
bytes from the call's output area, and the bytes differ because the memory
layouts differ (solc reuses the input buffer, we use scratch); that is
undefined on both sides and unobservable from byzantium on. It also found
finding 33.

### 28. `push`, `push()`, and `pop` on a storage `bytes` rewrite the whole value

File: `symbolic-audit/storage_bytes_push_gas.sol`
Source: `semanticTests/array/push/push_no_args_bytes.sol` (`g(811)` ran out
of the harness's 20M gas in solar at byzantium while solc used 9.6M) and
`events/event_indexed_string.sol`. Found by the per-EVM-version stateful
sweeps; the divergence is gas only, both agree with a 500M cap.

Measured at `-Ogas`, osaka, against solc 0.8.36 via-IR:

| Call | solc gas | solar gas | ratio |
|------|------|------|------|
| `pushArg(40)`, `a.push(bytes1(i))` 40 times from empty | 113 422 | 150 450 | 1.33 |
| `pushArg(300)` | 534 369 | 1 599 173 | 2.99 |
| `pushNoArg(300)`, `a.push()` | 274 901 | 1 223 125 | 4.45 |
| `pushNoArgAssign(300)`, `a.push() = bytes1(i)` | 575 879 | 1 434 125 | 2.49 |
| `popAll()` over 300 bytes | 325 635 | 1 542 326 | 4.74 |
| `pushU8(300)` on `uint8[]` | 448 313 | 444 650 | 0.99 |

The MIR for `pushArg` calls `internal_call @store_storage_bytes` per
iteration: each `push` loads the current value, appends in memory, and
stores the whole value back, so a loop of n pushes moves O(n^2) storage
words. solc appends in place: it reads the header, and for a long value
writes only the last data word and the header, with the short-to-long
transition handled once at 32 bytes. `pop` behaves the same way. This is
the sibling of finding 26, whose fix deliberately left `push` and `pop`
alone.

Severity: gas. Correct results, but 2.5x to 4.7x the gas of solc on
`bytes` growth and shrink loops.

Cause and fix (`8f2e55ba`): the three operations were lowered as a
whole-value round trip through `load_storage_bytes` and
`store_storage_bytes`. They now follow solc's `array_push`,
`array_push_zero`, and `byte_array_pop`: decode the header, and for a long
value write only the affected data word and the header, with the
short-to-long transition at 31 to 32 bytes on push and the long-to-short
transition only when popping from 32 bytes, popped bytes cleared,
`Panic(0x31)` on an empty pop and `Panic(0x41)` at the 2^64 length limit.
The `StorageBytePush` lvalue variant is gone: `a.push() = x` is an
ordinary packed-storage read-modify-write like `a[i]`. Re-measured on
`8f2e55ba` at `-Ogas`, osaka:

| Call | solc gas | solar gas | ratio |
|------|------|------|------|
| `pushArg(40)` | 113 957 | 110 047 | 0.97 |
| `pushArg(300)` | 554 634 | 484 170 | 0.87 |
| `pushNoArg(300)` | 275 553 | 177 460 | 0.64 |
| `pushNoArgAssign(300)` | 576 619 | 511 326 | 0.89 |
| `popAll()` over 300 bytes | 291 550 | 302 777 | 1.04 |
| `pushU8(300)` on `uint8[]` | 449 171 | 445 338 | 0.99 |

The repro and `probes/storage_bytes.sol` agree on random sequences, and
`push_no_args_bytes.sol` `g(1000)` at byzantium, which ran out of the
harness's 20M gas before, now agrees at ratio 0.999. The UI codegen
corpus shrinks 0.6% at `-Ogas` and 1.1% at `-Osize`. The review compared
every branch against solc's Yul helpers, ran the harness at three
optimization levels and at byzantium and homestead, and found one defect
in the commit: `a.push()` used as a value returned a constant zero instead
of reading the appended byte, visible when assembly had dirtied the slot
(`3d5d21f8`, with dirty-header, 2^64 limit, and `bytes(string)`
coverage added). It confirmed three non-issues: pop costs about 37 gas
more than solc because solc outlines `byte_array_pop` and we inline it, a
bare `a.push();` keeps a dead phi at `-Onone`, and non-bytes arrays check
the push length with `Panic(0x11)` where solc uses `Panic(0x41)` at 2^64
(reachable only through an assembly-dirtied length slot). It also found
finding 37.

### 29. At homestead every external call forwards `gas()`, which a pre-EIP-150 EVM rejects

File: `symbolic-audit/external_call_gas_prebyzantium.sol`
Found by the agent fixing finding 27, which ran the new UI test on a
homestead EVM; the stateful harness cannot see it because pre-byzantium
targets run on an osaka EVM there. Reproduced with
`target/symaudit/prebyz_gas.py`, which compiles the caller with both
compilers at homestead and runs it on a forge EVM at homestead from a test
contract that uses only assembly calls with a static output buffer.

```solidity
contract Callee { function f() external returns (uint256) { return 42; } }
contract R { function callRet(address a) external returns (uint256) { return I(a).f(); } }
```

| Call on a homestead EVM, `Callee` deployed | solc | solar |
|------|------|------|
| `callRet(callee)` with 100 000 gas | returns `42`, 23 765 gas | fails, all 100 000 gas consumed |
| `callRet(callee)` with 1 000 000 gas | returns `42`, 23 755 gas | fails, all 1 000 000 gas consumed |
| `callNoRet(callee)`, no return values | returns `1`, 23 562 gas | fails, all gas consumed |
| `callValue(callee)`, `{value: 0}` on a payable target | returns `1`, 23 763 gas | fails, all gas consumed |
| the same three at tangerineWhistle | agree | agree |

Before EIP-150 (tangerineWhistle) a `CALL` whose gas argument exceeds the
gas remaining is an exception, not a capped forward. solc therefore emits
`sub(gas(), 40 + 10 [+ 9000 with value] [+ 25000 without the extcodesize
check])` as the call gas whenever `canOverchargeGasForCall()` is false, and
`gas()` from tangerineWhistle on (`IRGeneratorForStatements.cpp`,
`appendExternalFunctionCall`; the homestead runtime shows `PUSH1 0x31 NOT
GAS ADD CALL`). It also touches the end of the output area before the
call so the memory expansion is not charged inside the gas computation.
Our lowering emits `GAS` directly as the call gas at every version, so on a
homestead EVM every external call without an explicit `{gas: ...}` fails.
Only homestead is affected; the stateful harness's osaka EVM applies the
63/64 rule and hides it.

Severity: miscompile for the homestead target. Every external call fails.

Cause and fix (`5aebf672`): `lower_call_options` materialized `GAS` as the
call's gas operand at every EVM version, and did so before the argument
encoding, so the value also included the encoding's own cost. At
homestead the operand is now `sub(gas(), 50)`, `sub(gas(), 9050)` with a
`value`, for direct calls, external function pointers, library
delegatecalls, and `try`; `sub(gas(), 25050)` and `sub(gas(), 34050)`
for bare `.call`/`.delegatecall`, mirroring solc's `appendBareCall`
which always adds the new-account cost; `transfer`/`send` keep 2300/0
and precompiles keep their reserve. The `GAS` is evaluated just before
the call only where the reserve applies, so tangerineWhistle and later
are byte-identical to before (716 UI codegen files compiled at four EVM
versions and three optimization levels: no diffs). The output area is
touched (`mstore(offset + size - 32, 0)`) before `GAS` where the call
owns its buffer, and multi-word returns get their own buffer instead of
reusing the input buffer, because a pre-EIP-150 `CALL` charges the
output expansion out of the gas left before checking the forwarded
amount. Re-verified on a homestead forge EVM: `callRet`, `callNoRet`,
`callValue` all succeed with the same return values at 100 000 and
1 000 000 gas, using 23 429 / 23 418 / 23 448 gas against solc's
23 765 / 23 562 / 23 763; tangerineWhistle unchanged. The review
(`f675c760`) checked every shape's expression against solc's source
(table in the commit), found the precompile reserve one step off (25 100
where solc reserves 25 050) and fixed it, added optimized homestead
revisions and eight-word and aggregate return cases to the new run-call
test since every existing homestead test ran at `-Onone`, and probed
reverting, gas-hungry, dynamic-return, looping, value-carrying, bare,
`transfer`/`send`, function-pointer, and constructor-code calls on a
homestead EVM with 0 mismatches. Residual: at homestead a loop of
multi-word-return calls allocates a fresh buffer per call where solc
reuses one, and the `try` reserve is unreachable until finding 30 is
fixed.

### 30. `try`/`catch` with a bare `catch { }` is rejected before byzantium

File: `symbolic-audit/try_catch_prebyzantium.sol`
Found while fixing finding 27.

```solidity
try I(a).f() returns (uint256 v) { r = v; } catch { r = 7; }
```

| Input at homestead, tangerineWhistle, spuriousDragon | solc | solar |
|------|------|------|
| bare `catch { }` after a call with return values | compiles | `EVM IR verification failed: block 0: opcode `returndatacopy` is unavailable for `homestead` EVM` (twice) |
| bare `catch { }` after a call without return values | compiles | same error |
| `catch Error(string memory)`, `catch Panic(uint256)`, `catch (bytes memory)` | error `This catch clause type cannot be used on the selected EVM version (homestead). You need at least a Byzantium-compatible EVM or use `catch { ... }`.` | error `typed catch clause requires Byzantium-compatible EVM` plus a codegen error `cannot bind try/catch returndata before Byzantium` |
| the same at byzantium | compiles | compiles |

solc's type checker only rejects the typed catch clauses before byzantium
(`TypeChecker.cpp`, `visit(TryStatement)`), and its codegen for a bare
`catch` needs no return data. Our `try` lowering always emits the
`RETURNDATACOPY` that fills the catch clause's low-level data, even when no
clause binds it, and the backend's EVM IR verifier then rejects the
function. The diagnostic has no source location and reports an internal
verification failure for a valid program.

Severity: valid program rejected, with an internal error as the message.

Cause and fix (`af297b6b`): `lower_try` passed an empty output area to
the call, decoded the return values from return data on success, and
materialized the return data on failure to match `Error`/`Panic` clauses
even when only a bare `catch` existed. Where the EVM version has no return
data the call now plans a static output area through the shared
`plan_return_buffer`/`finish_external_call` helpers (so the `extcodesize`
guard of finding 27, the gas reserve and output-area touch of finding 29,
and the returned-word validation all apply), reads the static return
values back from it, and carries no catch data: the bare clause runs
unconditionally and the forwarding revert is `revert(0, 0)`, as in solc's
`revert_forward_0`. Typed clauses pre-byzantium bail without a second
diagnostic, and the type checker's error carries solc's codes 1812 and
9908. Byzantium and later are unchanged (all 719 UI codegen files
compiled at four versions and three optimization levels: no diffs).
Verified with the harness on the repro at homestead, tangerineWhistle,
spuriousDragon, byzantium, and osaka, on a live-callee probe with 15
shapes (one, two, aggregate, and no return values, view, revert, revert
with string, panic, gas hog, code-less callee, function pointer, side
effects, `try new`) at all five versions, and on a homestead forge EVM
where `try` around a returning, a two-word, a no-return, and a
gas-capped reverting callee gives solc's values with gas within 300. The
review (`813b6632`) read solc's `visit(TryStatement)`, `handleCatch`,
and forwarding-revert helpers, probed `try` in loops, nested, in a
modifier, in the constructor, and with the binding used after the block
on a real homestead EVM, and fixed the catch-clause name check: solc
reports 3542 for a name other than `Error` or `Panic` at every version,
and we had accepted `catch Foo(uint256)` from byzantium on. Confirmed by
both: a code-less callee reverts the whole function instead of running
the catch pre-byzantium and, with return values, from byzantium on,
because solc guards `try` calls too. Residual: a pre-byzantium call whose
dynamic return value is unused is still rejected (finding 39).

### 31. External library calls with return values are rejected before byzantium

File: `symbolic-audit/library_call_prebyzantium.sol`
Found while fixing finding 27.

```solidity
library L { function dbl(uint256 x) external pure returns (uint256) { return 2 * x; } }
contract C { function viaLib(uint256 x) external pure returns (uint256) { return L.dbl(x); } }
```

| Input at homestead | solc | solar |
|------|------|------|
| `L.dbl(x)` returning `uint256` | compiles | error `codegen cannot decode linked library returndata before Byzantium` |
| `L.pair(x)` returning `(uint256, uint256)` | compiles | same error |
| `L.noret(x)`, no return values | compiles | compiles |
| the same at byzantium | compiles | compiles |

solc's homestead IR for the call is `delegatecall(add(gas(), not(49)),
addr, in, 36, out, 32)` for one return word and `..., out, 64)` for two:
the static return size is passed as the output size and the words are read
back from that buffer; there is no `RETURNDATASIZE` involved. Our
`lower_library_call` always decodes return data and bails with a diagnostic
when the EVM version has none, so any contract that calls a linked library
function with a return value cannot be compiled for the three pre-byzantium
targets.

Severity: valid program rejected.

Cause and fix (`0e722706`): `lower_library_call` hardcoded a return plan
with an empty output area and return-data decoding, so
`finish_external_call` bailed wherever the version has no return data.
Pre-byzantium the library delegatecall now plans a static output area
through the shared helpers of findings 29 and 30 (gas reserve of 50, the
output-area touch, the `extcodesize` guard, and word validation), passes
it as the delegatecall's output operands, and reads the return values or
`abi_decode`s a static aggregate back from it; from byzantium on the plan
is unchanged (720 UI codegen files at byzantium and osaka, three
optimization levels: no diffs). Dynamically encoded return values keep
the diagnostic where solc rejects their use (finding 39 covers the unused
case). Verified with a linked-library forge project (both compilers'
library and caller through Standard JSON `libraries`, 13 calls: one and
two words, `uint256[2]`, a static struct, a storage-pointer parameter, a
reverting callee, no return, a code-less library) at five EVM versions
and three optimization levels, 195 comparisons with 0 mismatches. The
review (`a5b57877`) extended that to 49 calls with dirty-word libraries
(solc reverts pre-byzantium on a dirty returned `bool`, `uint8`,
`address`, `bytes4`, enum, or struct `bool` member and accepts dirty
`uint256`/`bytes32`; we match all seven), `try`, loops, `using for`,
storage and mapping parameters, and a gas-hungry callee, 588 comparisons
with 0 mismatches, and pinned `bool` and struct returns, an attached call
with a storage receiver, and a tangerineWhistle revision. Residual: from
byzantium on library calls still decode return data while direct calls
use the static area, a deliberate leftover to keep byzantium bytes
identical.

### 32. Loop-carried values round-trip through memory frame slots on every iteration

File: `symbolic-audit/loop_carried_frame_slots.sol`
Source: `semanticTests/inlineAssembly/inline_assembly_for.sol`, whose
`f(304385)` (a Yul factorial loop) ran out of the stateful harness's 20M
gas on our side at osaka while solc used 15.5M. Found by the rerun of the
osaka stateful sweep on `49cf1b51d`; with a 500M cap both agree, so the
divergence is gas only.

```solidity
function solidityLoop(uint256 a) external pure returns (uint256 b) {
    b = 1;
    unchecked { for (uint256 i = a; i > 0; i--) { b *= i; } }
}
```

Measured at `-Ogas`, osaka, 100 000 iterations:

| Call | solc gas | solar gas | ratio |
|------|------|------|------|
| `solidityLoop(100000)`, the loop above | 5 122 122 | 6 422 025 | 1.25 |
| `solidityCall(100000)`, the same loop in an internal function | 5 122 103 | 8 822 055 | 1.72 |
| `yulTopLevel(100000)`, the loop in inline assembly | 5 122 027 | 7 621 989 | 1.49 |
| `yulFunction(100000)`, the loop in a Yul function | 5 122 057 | 7 722 044 | 1.51 |
| `yulFunctionNoLoop(12345)`, a Yul function without a loop | 22 138 | 22 127 | 1.00 |

The optimized MIR is ideal, two `phi`s in the loop header:

```
bb1:
  v0 = phi [bb5: 1], [bb3: v4]
  v1 = phi [bb5: arg0], [bb3: v5]
  jumpi v1, bb3, bb2
bb3:
  v4 = mul v0, v1
  v5 = sub v1, 1
  jump bb1
```

but the runtime EVM IR materializes both phis in memory frame slots: the
header block stores them (`push 160 mstore`, `push 192 mstore`) and the
body reloads them (`push 192 mload`, `push 160 mload`) on every iteration,
about 30 gas per iteration on top of solc's 51. solc keeps both values on
the stack and the loop body is `swap2 dup3 mul swap2 push0 not add dup1
jump`. The internal-function shape pays more because the call frame adds
its own loads and stores.

Severity: gas. Correct results, but every loop whose body is small relative
to two memory round trips per carried value costs 1.25x to 1.7x solc, and
loops are where users spend gas deliberately.

### 33. `this.f()` in an internal function reached from the constructor skips the `extcodesize` guard

File: `symbolic-audit/this_call_from_constructor_helper.sol`
Found by the review of finding 27's fix (`46b9d61f8`).

```solidity
contract R {
    uint256 public seen;
    constructor() { helper(); }
    function helper() internal { this.setIt(); }
    function setIt() external { seen = seen + 1; }
}
```

| Deployment | solc | solar |
|------|------|------|
| `R()`, `this.setIt()` in a helper the constructor calls | reverts (no code at `this` yet) | deploys, `seen() == 0` |
| `Direct()`, `this.setIt()` directly in the constructor | reverts | reverts |
| `ViaModifier()`, `this.setIt()` in a constructor modifier | reverts | reverts |
| `Derived()`, `this.setIt()` in a base constructor | reverts | reverts |
| `Runtime.viaHelper()`, the same helper at runtime | `seen() == 1` | `seen() == 1` |
| all of the above at homestead | same | same |

We skip the `extcodesize` guard for `this.f()` in runtime code, where the
contract's own code is known to exist. solc has no `this` special case at
all (`checkExtcodesize` in `appendExternalFunctionCall` depends only on
the return size, the EVM version, and `revertStrings`), so it guards the
call in creation code too, where the contract has no code yet. Our
lowering disabled the bypass only when the *current* MIR function was the
constructor, so an internal helper compiled into the creation object still
skipped the guard, the `CALL` to the code-less address succeeded with no
effect, and the deployment went through where solc's reverts. The
runtime bypass itself is sound (the code exists) and only saves gas
relative to solc.

Severity: miscompile at every EVM version. A deployment that solc rejects
succeeds silently with the call dropped.

Cause and fix (`e638b191`): creation and runtime code are not separate
MIR; the backend's `prepare_deployment_prefix` emits a second copy of
every function reachable from the constructor into the creation prefix,
so the constructor attribute of the current function said nothing about
the object a helper would run in. The fix exposes the creation half of the
call graph that sema already computes (`Gcx::contract_creation_functions`:
base constructors, their modifiers and inheritance arguments, state
variable initializers, and everything they reach), records it as
`in_creation_code` on the function lowerer, and routes the direct-call and
`try` sites through one `needs_receiver_code_check`. A function shared by
both objects keeps the guard in both copies, which matches solc's gas at
runtime (`Shared::runIt` 44 373 vs 44 548). Re-verified: all five repro
contracts agree at osaka and homestead, `R` now failing to deploy on both
sides. The review (`ffb4d4bf`) checked the creation set against solc's
creation object, probed initializer callees, base-constructor modifiers,
internal and external function pointers, `try` with a bare `catch` (the
guard reverts before the call on both sides, the catch does not run),
`address(this).call` (unguarded on both), free and library functions,
`new` of a contract whose constructor calls `this`, and a shared helper,
all agreeing, and pinned the `try`, initializer, and shared-helper paths
in a run-call test. Residual: internal function-pointer dispatchers carry
runtime-only targets into the creation object without the guard, reachable
only with a forged pointer id.

### 34. Any recursive internal function is rejected before constantinople

File: `symbolic-audit/recursion_preconstantinople.sol`
Sources: `semanticTests/freeFunctions/recursion.sol`,
`freeFunctions/free_namesake_contract_function.sol`,
`inlineAssembly/inline_assembly_recursion.sol`,
`modifiers/modifer_recursive.sol`, `structs/recursive_struct_2.sol`,
`structs/conversion/recursive_storage_memory.sol`. Found by the homestead
and tangerineWhistle stateful sweeps on `49cf1b51d`: all six compile at
osaka and fail to compile at the older versions.

```solidity
function f(uint256 n) internal pure returns (uint256) {
    if (n == 0) return 1;
    return f(n - 1) + 1;
}
```

| Input | solc | solar |
|------|------|------|
| the file above at homestead, tangerineWhistle, spuriousDragon, byzantium | compiles | error `EVM IR verification failed: block 6: `push` grows the stack to 1025 words, exceeding the limit of 1024` |
| the same at constantinople and every later version | compiles | compiles |
| the same at `-Onone`, byzantium | compiles | same error, twice |
| the six upstream tests at byzantium | compile | same error |
| a non-recursive function using `/ 2` and `& 1` at byzantium | compiles | compiles |
| the recursive function next to a second, larger one, byzantium, `-Ogas` | compiles | compiles (`-Onone` still fails) |

The last row shows the check depends on what else the pipeline did to the
module, so which recursive programs are rejected varies with the
optimization level and the surrounding code.

The cause is in the backend's EVM IR verifier, not in lowering.
`verify_after_legalization` (`crates/codegen/src/backend/evm/ir/verify.rs`)
runs the full stack-operation check `verify_module` only when the target has
no `SHL`/`SHR`/`SAR`, that is, only after `legalize-shifts` has rewritten
the module for a pre-constantinople EVM. `verify_stack_ops` walks the CFG
from the entry with a concrete stack depth and re-queues a block whenever
it is reached at a depth not seen before (`alternate_depths`). An internal
recursive call is a jump back to the callee's entry with the return
address and arguments pushed, so the callee's entry is reached at a
strictly larger depth on every round; the walk never converges and stops
only when the depth passes 1024, which it reports as a program error. From
constantinople on the check is skipped, so the same program compiles. The
`-Zvalidate-ir=false` flag does not disable this check.

Severity: valid programs rejected for the four oldest EVM versions, with
an internal verification failure as the diagnostic.

### 35. `virtual` free functions and non-`external` interface functions are accepted

File: `symbolic-audit/interface_free_function_checks.sol`
Found by re-probing the diagnostic gaps the item-22 review listed. The
others on that list are already handled: constructors marked `virtual`
(7001) are a parse error, `receive` in a library and non-pure operator
definitions (7775) are rejected with solc's wording, a fractional array
length is rejected, and "Type too large for memory" is reported.

```solidity
interface I { function f() public; function g() internal; }
function free() virtual {}
```

| Declaration | solc | solar |
|------|------|------|
| `public` or `internal` function in an interface | error 1560 `Functions in interfaces must be declared external.` | compiles |
| `virtual` free function | error 4493 `Free functions cannot be virtual.` | compiles |

Both checks live in `TypeChecker::visit(FunctionDefinition)` next to the
library and `payable` checks fixed in items 21 and 22. Not codegen
divergences: solar accepts programs solc refuses.

Severity: missing diagnostics.

### 36. `-Ogas` regression: a loop assigning a tuple after a branch returns a wrong value

File: `symbolic-audit/loop_tuple_assign_miscompile.sol`
Source: `symbolic-audit/probes/stack_pressure.sol`, `f2` and `f12`.
Found by the byzantium symbolic probe lane on `49cf1b51d`; the same
functions agreed under `-Ogas`, `-Onone`, and `-Osize` in the third
session (`e6db78e6b`) and in the fourth-session probe lanes
(`e9ca037d5`), so this is a regression from one of the codegen fixes that
landed between `e9ca037d5` and `49cf1b51d`.

```solidity
if (v2 & 1 == 1) { v7 = (v6 * v4); } else { v2 = k(v6); }
for (uint256 i = 0; i < (v4 & 3); i++) { (v3, v2) = h(v1, v2); }
return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
```

| Call | solc | solar `-Ogas` | solar `-Onone`, `-Osize` |
|------|------|------|------|
| `f2(0, 1, 0)` | `0xff..e326ec342f968e7d18` | `0xff..e326ec342f968e7d17` | agree |
| `f2Inline(0, 1, 0)`, tuple written inline instead of through `h` | `0xff..968e7d18` | `0xff..968e7d17` | agree |
| `f2(3, 5, 7)`, `f2(1, 0, 2)`, `f2(0, 0, 0)` | agree | agree | agree |
| `stack_pressure.sol` `f2(0, 1, 0)` (loop body also updates `v6`) | `0x1cd913cbd0697182d6` | `0x1cd913cbd0697182d9` | agree |
| `stack_pressure.sol` `f12(1, 0, 0)` | `0x6560d37a9f3396e3252` | `0x4f0b3eacafcd851e80` | agree |
| the same at byzantium and constantinople | same | same | same |

Only the `else` path (`v2 = k(v6)`, taken when `b + 3` is even) is wrong,
and only when the loop body assigns a tuple; the loop bound must be
runtime (`v4 & 3`; a constant `3` agrees), the `if`/`else` must be present,
and the return must combine all eight values (returning `v2 ^ (v3 * 4)` or
`v6` alone agrees), so the defect needs stack pressure across the branch
and the loop. `-Zevm-ir-pipeline=none` still differs, so the EVM IR
passes are not involved; the optimized MIR of `f2` is semantically
correct (the tuple goes through a static memory temporary at 160 and both
components flow into the loop phis), so the defect is in MIR-to-EVM
lowering under an `-Ogas`-only MIR shape, the same area as findings 7 and
15. In the EVM IR the `else` block spills four values to frame slots 192,
224, 256, and 288 before joining the loop header while the `if` block
does not, and the loop body writes the tuple temporary at 160.

Severity: miscompile (wrong return value) at the default optimization
level, with no assembly involved. Highest priority of the open items.

Cause and fix (`daaf2d05`): in the per-block MIR-to-EVM emission loop
(`crates/codegen/src/backend/evm/codegen.rs`), a block whose terminator
keeps its stack alive across an edge leaves the frame-slot stores of the
carried values to the successor, and the successor decides which values to
store from the availability set intersected over its predecessors'
recorded `spill_avail_out`. A predecessor not yet emitted contributed
nothing to that intersection, on the assumption that it would store on its
own path; on a preserved edge that store obligation had been handed to the
successor, so nobody stored. In the repro the `else` arm is laid out after
the join, both arms carry `[v4, v13, v12]` into it, the join skipped the
store of `v4` to slot 320 because the `if` arm happened to store it, and
the `else` path multiplied an unwritten slot (`v4 * 5 = 15` off, the xor
difference of `0x0f`). The fix stores, before the terminator, every value
in `live_out(block) ∩ live_in(successor)` for every successor already
emitted that the block does not reach through a loop back edge; the
now-redundant store in the earlier arm is removed by dead-spill-store
elimination. The defect is latent since `b424b653` and was exposed by MIR
shape changes that moved the arm after the join. Verified with the
harness on the repro at all three optimization levels and on the whole
stack-pressure probe (120 random calls, gas ratio 1.003); runtime corpus
size unchanged, UI codegen corpus +12 bytes at `-Ogas`. The review
(`ae814158`) read the whole spill machinery, built a temporary per-slot
value-dataflow oracle over the emitted EVM IR that flags the parent commit
on exactly the buggy slot and is silent after the fix across `tests/ui`,
the probes, and the semantic tests at three optimization levels, wrote 26
adversarial join and loop shapes of which nine were also miscompiled on
the parent, and pinned five of those layouts in a run-call test. Residual:
`-Onone` grows up to 50 bytes per file because dead-store elimination is
off there, and stack-only (resident argument) values are outside the
store path by construction.

### 37. `a.push()` used as a value on a non-bytes storage array returns 0

File: `symbolic-audit/storage_array_push_rvalue.sol`
Found by the review of finding 28's fix (`3d5d21f8`), which fixed the
same defect for storage `bytes`; the generic array path has it too.

```solidity
uint256[] a;
function dirtyThenPush(uint256 v) external returns (uint256 r) {
    uint256 len = a.length;
    assembly { mstore(0, a.slot) sstore(add(keccak256(0, 32), len), v) }
    r = a.push();
}
```

| Call | solc | solar |
|------|------|------|
| `dirtyThenPush(0xff)` on `uint256[]` | `0xff` | `0` |
| `dirtyThenPush8(0x7f)` on `uint8[]` | `0x7f` | `0` |
| `pushClean()` twice, slot never dirtied | `0` | `0` |

`push()` without an argument appends an element without writing it, and
its value is a reference to the new slot. solc reads that slot when the
expression is used as a value; our `lower_storage_array_push` returns a
constant zero for the no-argument form on value-type elements. Because
neither compiler clears the slot on push, the difference is visible
whenever assembly (or an earlier pop-free length manipulation) left a
value there. Storage effects and the appended length agree.

Severity: miscompile of a returned value, reachable only with an
assembly-written slot. Low.

### 38. At `-Ogas` a jump separates homestead's `sub(gas(), 50)` from a shared CALL block

File: `symbolic-audit/call_gas_reserve_split.sol`
Found by the agent fixing finding 30: its optimized homestead run-call
test failed with out-of-gas on a live callee, and the shape reproduces
without `try`. Measured with `prebyz_gas.py` on a homestead forge EVM,
200 000 gas, on `5aebf672` (finding 29 fixed):

| Call, `Callee` deployed | solc | solar `-Ogas` |
|------|------|------|
| `live(callee)`, one return value | `42`, 23 802 gas | fails, all gas consumed |
| `livePointer(callee)`, through an external function pointer | `42`, 23 758 gas | fails, all gas consumed |
| `liveTwo(callee)`, two return values | `3`, 23 781 | `3`, 23 596 |
| `liveAggregate(callee)`, `uint256[2]` | `7`, 24 678 | `7`, 24 529 |
| `liveNoReturn(callee)` | `1`, 23 616 | `1`, 23 472 |
| the same contract with only `live` in it (finding 29's repro) | agree | agree |

The runtime disassembly shows six `GAS SUB` sequences; four are followed
directly by `CALL`, but one is `GAS SUB PUSH JUMP` into a block starting
`JUMPDEST ... CALL` that several call sites share (EVM IR tail merging
or terminal deduplication of the identical call tails). `JUMP` (8),
`JUMPDEST` (1), and `SUB` (3) cost 12 gas after `GAS` is read, more than
the 10-gas margin in solc's `sub(gas(), 40 + 10)` reserve, so the `CALL`
requests more gas than remains and a pre-EIP-150 EVM throws. solc's
`sub(gas(), N)` is always adjacent to its `CALL` (`PUSH NOT GAS ADD CALL`),
and solc's own comment notes the reserve "retains too much gas for now";
the margin is only safe when nothing but the subtraction executes between
`GAS` and `CALL`. The fix must either keep the reserve computation and
the call in one block for pre-EIP-150 targets, or reserve enough for the
worst-case jump sequence the backend can insert.

Severity: miscompile for the homestead target at the default optimization
level; which call sites fail depends on which tails the backend merges.

### 39. A pre-byzantium external call whose dynamic return value is unused is rejected

File: `symbolic-audit/dynamic_return_unused_prebyzantium.sol`
Found by the review of finding 30's fix.

```solidity
interface I { function dyn() external returns (bytes memory); }
contract C {
    function t(address a) external { I(a).dyn(); }
    function u(address a) external { try I(a).dyn() { } catch { } }
}
```

| Input at homestead | solc | solar |
|------|------|------|
| `I(a).dyn();`, value discarded | compiles | error `codegen cannot decode external function returndata before Byzantium` |
| `try I(a).dyn() { } catch { }` | compiles | error `codegen cannot decode try/catch returndata before Byzantium` |
| `bytes memory b = I(a).dyn();` | error 6509 (inaccessible dynamic type) | rejected |
| the same at byzantium | compiles | compiles |

Before byzantium solc types a dynamically encoded return value as
`InaccessibleDynamicType`: the call is legal as long as the value is not
used, and the type checker rejects any use. Our lowering tries to decode
the return data regardless and reports the codegen gap even when nothing
reads the value.

Severity: valid program rejected. Low; pre-byzantium only.

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

### Fourth session (2026-09-03)

New probe sets and a real-world library corpus on `decbf7fcf` and
`e9ca037d5` (solar binaries snapshotted before the session's fixes; the
fixes touch ABI output and diagnostics, not codegen). Findings 20 to 22
came from this session and are all fixed; none is a codegen divergence. The
probe sources are in `symbolic-audit/probes/session4/`.

| Angle | Probe file(s) | Result |
|-------|------|--------|
| Signed arithmetic edges: `int.min / -1`, `-int.min`, signed `%`, shifts, `**` with negative bases, narrow truncation | `signed_arith.sol` | agree |
| Fixed bytes: shifts, indexing, bitwise ops, conversions to and from integers, `bytes.concat`, `bytesN(bytes calldata)` truncation | `fixed_bytes.sol` | agree |
| Storage packing: mixed narrow variables, packed structs, `uint8[]`/`bool[]`/`int8[]`/`bytes1[]`, fixed arrays, delete, dirty raw slots | `storage_packing.sol` | agree |
| Enums and conversions: out-of-range `Panic(0x21)`, 256-member enum, address and integer chains, dirty enum words | `enum_conv.sol` | agree |
| Control flow: do-while `continue`, nested breaks, early returns, modifier `_` twice or never, return inside modifiers | `control_flow.sol` | agree |
| Memory arrays: nested `new`, reference semantics, array literals, multi-dimensional, storage copies both ways, `pop` on empty | `arrays_memory.sol` | agree |
| Calldata slices: `[i:j]`, slice of slice, `bytes4(d[:4])`, `abi.decode(d[4:])`, slices to internal functions | `calldata_slices.sol` | agree |
| Errors: `require` with custom errors, `revert` with dynamic args, every `Panic` code, exponentiation overflow by base and width | `errors_require.sol` | agree |
| Compound assignment and `++`/`--` on locals, storage, arrays, mappings, structs, with side-effecting indices; checked and unchecked | `checked_ops.sol` | agree |
| Mapping keys of every type, nested mappings, structs and arrays as values, raw slot checks, dirty keys | `mappings.sol` | agree |
| Inheritance: diamond `super` chains, overloads, recursion, many arguments and returns, free functions, constants | `functions_inherit.sol` | agree |
| ABI: `encodeWithSelector`/`encodeCall`/packed with narrow and dirty values, nested static structs, decode round trips | `abi_misc.sol` | agree |
| Yul: `switch`, `for`, functions, `leave`, recursion, `byte`/`signextend`/`sar`/`sdiv`/`smod`, `mcopy`, `clz`, dirty assignments to typed locals, `.slot`/`.offset` | `yul_advanced.sol` | agree |
| Libraries: `using for` on structs, storage pointers, internal and external function pointers in locals, arrays, structs, storage, mappings | `libs_fnptr.sol` | agree (finding 20 on ABI output) |
| Algebraic identities that must keep checked semantics: `x*0`, `x/x`, `(x+1)-1`, dead trapping expressions, phi of constants | `constant_folding.sol` | agree |
| Loop-invariant trapping expressions with zero iterations, loop-carried narrow values, length changes inside loops, aliasing in loops | `loops_opt.sol` | agree |
| CSE and GVN hazards: reads across storage, memory, and assembly writes, keccak after mutation, inlined helpers with side effects, mixed-width equalities | `inline_cse.sol` | agree |
| Deep storage structs: nested structs with arrays and bytes, copies between storage slots, array-of-struct pop clearing, `bytes[]` | `storage_structs_deep.sol` | agree (concrete only; symbolic timeouts) |
| String and hex literals: escapes, unicode, 31/32/33-byte storage boundary, literal to `bytesN`, `string.concat` | `strings_unicode.sol` | agree |
| Public getters for every state variable shape, including structs that drop array members and dirty raw slots | `getters.sol` | agree |
| Deep expressions: 20 live locals across branches and loops, 10-argument calls with side effects, nested ternaries, mixed widths | `deep_expressions.sol` | agree |
| Modifiers under inheritance: overridden modifiers, `super` chains through three levels, modifier arguments with side effects | `modifiers_inherit.sol` | agree |
| `delete` on every storage, memory, and function-pointer type, including packed arrays and dirty slots | `delete_semantics.sol` | agree |
| Memory bytes manipulation, dirty scratch and zero-slot before allocation, empty array pointers, free-memory-pointer deltas | `memory_bytes_ops.sol` | agree except the pointer deltas (non-bug) |
| Memory-safe assembly convention: scribbling at `mload(0x40)` after every allocation shape | `fmp_convention.sol` | agree |
| Real-world libraries: OpenZeppelin `Math`, `SafeCast`, `SignedMath`, `Strings`, `Bytes`, `Packing`, `RLP`, `Base58`, `Base64`, `Arrays`, `ECDSA`, `MerkleProof`, `P256`, `RSA`, `Time`, `SlotDerivation`, and others, plus PRBMath `UD60x18`/`SD59x18` math, casting, and helpers, each internal function wrapped as an external entry point by `tools/gen_wrappers.py` | `target/symaudit/corpus/` | agree except `RLP.decodeList` (non-bug) |

Every lane also ran under `-Onone` and `-Osize`, and every function the
solver could not close (nonlinear multiplication, exponentiation, deep
loops, heavy storage) was replayed concretely at its boundary values under
all three optimization levels: about 480 concrete cases per level, all
agreeing. Further lanes on the same sources:

- `--evm-version paris` (no `PUSH0`, no `MCOPY`, no transient storage) over
  the probes and the corpus: 1104 and 534 checks, 2 mismatches (the
  `fnPtrExt` self-address pair), no new divergence.
- A second corpus from the other project archives: Arbitrum Nitro
  one-step-proof state libraries (`Deserialize`, `Value`, `Machine`,
  `MerkleProof`, `GlobalState`, stacks), Morpho Blue math libraries, Aave's
  `SafeCast`, and forge-std `StdMath`, `StdStyle`, `LibVariable`: 126
  wrapper functions agree symbolically, the rest are solver limits,
  `vm.toString` cheatcodes, or nested struct inputs the harness cannot
  build; the math incompletes were replayed concretely at all three
  optimization levels (26 functions, 5 to 14 boundary cases each) and agree.
- Self-prefix lane: every nonpayable probe function first called concretely
  with representative arguments, then compared symbolically against the
  resulting non-zero storage: 331 tasks, 251 agree, 0 mismatch.
- Non-canonical ABI input: every probe function with a narrow value
  parameter (`uintN`, `intN`, `bool`, `address`, `bytesN`) called with dirty
  high or low bits in each such word, all narrow words dirty at once, and
  the canonical encoding: 347 functions, 0 mismatches (both
  compilers reject the same encodings).

| Lane | Settings | Checked | Agreement | Incomplete | Mismatch |
|------|----------|---------|-----------|------------|----------|
| probes | `-Ogas` | 1297 | 1025 | 267 | 5 |
| probes | `-Onone` | 1261 | 996 | 262 | 3 |
| probes | `-Osize` | 1261 | 995 | 261 | 5 |
| corpus | `-Ogas` | 662 | 537 | 124 | 1 |
| corpus | `-Onone` | 662 | 533 | 128 | 1 |
| corpus | `-Osize` | 662 | 535 | 126 | 1 |

Every mismatch is one of the reviewed non-bugs below: `fnPtrExt` and
`fnPtrExtEnc` return `this.f` (the embedded address differs, the selector
agrees), the `memPtrAfter*` functions return free-memory-pointer deltas, and
`RLP.decodeList` returns `Memory.Slice` values that pack a memory pointer
into their low 128 bits (solc `0xa1`, solar `0x541`; the length half agrees).

### Fifth session (2026-09-03, stateful)

The symbolic tool never runs constructors, never sends value, and compares
one call at a time, so this phase adds a stateful differential,
`symbolic-audit/tools/statediff.py`. It compiles one contract with both
compilers through Standard JSON (via-IR for solc), deploys both creation
codes from a generated Foundry test with the same constructor arguments,
runs randomized or `--fixed` call sequences against both deployments, and
after every call compares success, return data, logs, and a snapshot of
every storage slot either side has written so far, with both deployment
addresses normalized. Targets compiled for a pre-byzantium EVM run on an
osaka EVM with revert data ignored, because the test contract itself needs
`RETURNDATASIZE`; `tools/prebyz_gas.py` covers the real homestead EVM with
a test contract that uses only assembly calls (finding 29).
`tools/sdcampaign.py` sweeps directories of files, taking constructor
arguments from the solc semantic tests' `// constructor():` expectation
lines, skipping files that need linking, legacy codegen, or another EVM
version, and flagging files that observe their own address, code, or
value (`self`) so that `self=False` mismatches are the triage candidates.
The scripts were first written as scratch tooling under `target/symaudit/`;
the versions in `symbolic-audit/tools/` are the ones used from finding 27
on and are runnable from there (`python3 symbolic-audit/tools/statediff.py`).

Findings 23 to 35 came from this phase: the stateful sweep over
`semanticTests/` at osaka found 23 to 25, the per-EVM-version sweeps found
26 to 28, 32, and 34, the fix and review of 27 turned up 29 to 31 and 33,
and re-probing the item-22 review notes gave 35.

Per-EVM-version stateful lanes over `semanticTests/` on `49cf1b51d`
(findings 23 to 27 fixed), 20 random calls in two sequences per
deployable contract, 20M gas per call, constructor arguments from the
tests' expectation lines. "Contracts" counts deployments compared; files
that need linking, another EVM version, legacy codegen, or observe their
own address, code, or value are skipped up front (582 of 1670 files at
osaka). Every mismatch is classified:

| EVM version | Contracts | Agree | Mismatch | Error | Calls compared | Mismatch classes |
|------|------|------|------|------|------|------|
| homestead | 1104 | 1083 | 11 | 10 | 43 351 | 9 fnptr, 1 f32, 1 stack depth |
| tangerineWhistle | 1107 | 1087 | 10 | 10 | 43 509 | 9 fnptr, 1 f32 |
| spuriousDragon | 1107 | 1087 | 10 | 10 | 43 509 | 9 fnptr, 1 f32 |
| byzantium | 1112 | 1088 | 14 | 10 | 43 672 | 9 fnptr, 2 f28, 1 f32, 2 gas cap |
| constantinople | 1117 | 1099 | 14 | 4 | 44 112 | 9 fnptr, 2 f28, 1 f32, 2 gas cap |
| petersburg | 1120 | 1100 | 16 | 4 | 44 226 | 9 fnptr, 2 f28, 1 f32, 4 gas cap |
| istanbul | 1120 | 1102 | 14 | 4 | 44 241 | 9 fnptr, 1 f28, 1 f32, 3 gas cap |
| berlin | 1120 | 1105 | 11 | 4 | 44 266 | 9 fnptr, 1 f32, 1 gas cap |
| london | 1120 | 1105 | 11 | 4 | 44 266 | 9 fnptr, 1 f32, 1 gas cap |
| paris | 1120 | 1105 | 11 | 4 | 44 266 | 9 fnptr, 1 f32, 1 gas cap |
| shanghai | 1121 | 1106 | 11 | 4 | 44 306 | 9 fnptr, 1 f32, 1 gas cap |
| cancun | 1161 | 1145 | 12 | 4 | 45 875 | 9 fnptr, 1 mcopy, 1 f32, 1 gas cap |
| prague | 1161 | 1145 | 12 | 4 | 45 875 | 9 fnptr, 1 mcopy, 1 f32, 1 gas cap |
| osaka | 1161 | 1145 | 12 | 4 | 45 875 | 9 fnptr, 1 mcopy, 1 f32, 1 gas cap |

Classes: "fnptr" is the internal-function-pointer-IDs-in-storage non-bug
(the nine `store_function_in_constructor`-style tests, storage differs by
the pointer ID only); "mcopy" is `inlineAssembly/mcopy.sol`, memory-unsafe
assembly assuming solc's layout; "stack depth" is
`operators/userDefined/recursive_operator.sol`, where solc's recursion hits
the 1024-slot stack first; "f28" and "f32" are findings 28 and 32 running
out of the 20M call gas on our side; "gas cap" is the reverse boundary
effect, a call that solc could not finish in 20M gas while ours did
(`array_storage_*` loops of `push()`), all agreeing with a 500M cap at
ratios 0.97 to 1.0. Errors: 3 support gaps per lane (a storage array
with a keccak-derived length, `uint8[erc7201("example.main")]`, rejected
as too large for codegen; a rational literal expression; and a bare
`abi.encode;` expression statement), 1 constructor expectation line the
runner cannot encode, and the 6 recursion tests of finding 34 before
constantinople. Nothing else differed.

Two further stateful lanes on `3d5d21f8` (findings 23 to 28 fixed):

- Project archives (`testdata/projects/*.json.gz`, extracted with their
  remappings, test and script directories excluded, random constructor
  arguments, the `DOMAIN_SEPARATOR`/`eip712Domain`/code-hash getters
  skipped): 150 deployable contracts from OpenZeppelin, Seaport, Solady,
  Solmate, Morpho, Uniswap v4, Nitro, Aave, Maple, and Lil Web3, 4978
  compared calls, 143 agree, 7 mismatches all in the reviewed classes
  (CREATE2 addresses derived from `address(this)`, EIP-712 hashes,
  internal function pointers in storage).
- Symbolic probe lanes per EVM version on `49cf1b51d` (`campaign.py`
  over `symbolic-audit/probes/`, byzantium and later only: the symbolic
  scaffold itself needs `RETURNDATASIZE`): osaka 1688 bounded agreements,
  306 incomplete, 10 mismatches; byzantium 1290 / 300 / 6; istanbul and
  london 1437 / 271 / 6 each. Every mismatch is finding 36 (`f2`, `f12`,
  now fixed) or a reviewed non-bug (`fnPtrExt`, `fnPtrExtEnc`,
  `fnEqDirty`, `fnRet`, `op_mcopy` clobbering the free-memory pointer,
  the three `memPtrAfter*` deltas).

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
- Fourth-session probes `memPtrAfterConcat`, `memPtrAfterEncode`, and
  `memPtrAfterString` return the free-memory-pointer delta of one allocation;
  solar's allocator reserves different amounts. `fmp_convention.sol` shows
  memory-safe assembly still sees a consistent pointer on both compilers.
- OpenZeppelin `RLP.decodeList` returns `Memory.Slice[]`, and each slice
  packs a memory pointer into the input buffer; only that pointer differs.
- Value-cleanup probes: `f.address := raw` in assembly followed by an assembly
  read of `f.address` shows the raw word in solc and a masked word in solar.
  Assembly reads of dirty locals are implementation-defined. A probe that
  OR-ed a symbolic word into `f.address` before `f == g` differed only because
  the two deployment addresses have different low bits.
- Pre-byzantium builds fail differently on a later EVM. solc emits `REVERT`
  at every EVM version, including homestead where the opcode does not exist
  and behaves as an invalid instruction (the homestead runtime of
  `external_call_prebyzantium.sol` contains 25 `REVERT`s); we emit `INVALID`
  there. On the target chain both consume all gas and return nothing. On the
  stateful harness's osaka EVM, which the pre-byzantium lanes run on, solc's
  code reverts cheaply with data while ours burns the whole call gas, so
  those lanes ignore revert data and the gas of failing calls. Nothing to
  fix: matching solc would only make the mis-targeted case cheaper.

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
