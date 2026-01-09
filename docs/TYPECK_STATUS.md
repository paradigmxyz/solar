# Solar Type Checker Status

This document compares Solar's type checker implementation against the reference solc implementation, based on analysis of the codebase and the `testdata/solidity` test suite.

> **Note**: The type checker is currently behind an unstable flag (`-Ztypeck`).

## Summary

| Category | Status |
|----------|--------|
| Type System Representation | ✅ Complete |
| Expression Type Inference | ✅ Mostly Complete |
| Type Conversions | ✅ Complete |
| Lvalue Analysis | ✅ Complete |
| Contract-Level Checks | ✅ Complete |
| Function Calls | ❌ Missing |
| Overload Resolution | ❌ Missing |
| View/Pure Checker | ❌ Missing |
| Custom Operators | ❌ Missing |
| ABI Builtins | ⚠️ Stub Only |

---

## Fully Implemented

### Type System Representation

The core type system matches solc's type universe completely:

- **Value Types**: `int`/`uint` (all sizes), `bool`, `address`/`address payable`, `fixed`/`ufixed`, `bytes1`-`bytes32`
- **Reference Types**: arrays (fixed/dynamic), mappings, structs, `bytes`, `string`
- **Special Types**: contracts, enums, function pointers, user-defined value types (UDVTs)
- **Literals**: integer literals with sign/size tracking, string literals with UTF-8 validation
- **Meta Types**: `type(T)` expressions, module types, builtin modules

**Location**: [crates/sema/src/ty/ty.rs](../crates/sema/src/ty/ty.rs)

### Expression Type Inference

| Expression Kind | Status | Notes |
|-----------------|--------|-------|
| Array literals | ✅ | Common element type inference |
| Assignments | ✅ | Including compound assignments |
| Binary operators | ✅ | Built-in operators only |
| Unary operators | ✅ | Including increment/decrement |
| Delete | ✅ | With proper lvalue enforcement |
| Identifiers | ✅ | With resolution |
| Indexing | ✅ | Arrays, mappings, bytes |
| Slicing | ✅ | Calldata arrays only |
| Member access | ✅ | Structs, contracts, builtins |
| Ternary | ✅ | With common type inference |
| Tuples | ✅ | Including destructuring |
| Literals | ✅ | All literal types |
| New expressions | ✅ | Contract/array creation |
| Payable casts | ✅ | `payable(addr)` |
| Type expressions | ✅ | `type(T)` |

**Location**: [crates/sema/src/typeck/checker.rs](../crates/sema/src/typeck/checker.rs)

### Type Conversions

Both implicit and explicit conversions are fully implemented:

**Implicit Conversions:**
- Integer literals → sized integers (with bounds checking)
- Contract → base contract (inheritance)
- Address payable → address
- Data location coercion (calldata → memory)

**Explicit Conversions:**
- Integer ↔ integer (same size for sign changes)
- Bytes ↔ bytes (any size)
- Integer ↔ bytes (same size)
- Address ↔ bytes20
- Address ↔ uint160
- Contract ↔ address (with payable checks)
- Enum ↔ integer

**Location**: [crates/sema/src/ty/ty.rs](../crates/sema/src/ty/ty.rs) - `try_convert_implicit_to`, `try_convert_explicit_to`

### Lvalue Analysis

Complete lvalue checking with specific error messages:

| Reason | Error Message |
|--------|---------------|
| Constant variable | "cannot assign to a constant variable" |
| Immutable variable | "cannot assign to an immutable variable" |
| Calldata array | "calldata arrays are read-only" |
| Calldata struct | "calldata structs are read-only" |
| Fixed bytes index | "single bytes in fixed bytes arrays cannot be modified" |
| Array length | "member `length` is read-only and cannot be used to resize arrays" |

### Contract-Level Checks

| Check | Status | Description |
|-------|--------|-------------|
| Duplicate definitions | ✅ | Same name + parameter types in scope |
| External type clashes | ✅ | ABI signature collisions |
| Payable fallback warning | ✅ | Fallback without receive |
| Receive function constraints | ✅ | external, payable, no params/returns |
| Storage size bounds | ✅ | Maximum slot calculation |

**Location**: [crates/sema/src/typeck/mod.rs](../crates/sema/src/typeck/mod.rs)

### Builtins

**Global Functions:**
- `blockhash`, `blobhash`, `gasleft`, `selfdestruct`
- `assert`, `require`, `revert`
- `addmod`, `mulmod`
- `keccak256`, `sha256`, `ripemd160`, `ecrecover`

**Builtin Modules:**
- `block.*` (number, timestamp, difficulty, etc.)
- `msg.*` (sender, value, data, sig)
- `tx.*` (gasprice, origin)

**Type Members:**
- `address`: balance, code, codehash, call, delegatecall, staticcall
- `address payable`: + transfer, send
- Arrays: length, push, pop
- Function types: selector, address
- Events: selector
- Structs: field access

**Location**: [crates/sema/src/builtins/](../crates/sema/src/builtins/)

---

## Partially Implemented

### Function Calls

| Feature | Status | Notes |
|---------|--------|-------|
| Struct constructors | ✅ | `S({field: value})` |
| Explicit type casts | ✅ | `uint256(x)` |
| Regular function calls | ❌ | `FnPtr` branch is `todo!()` |
| Event/error calls | ❌ | Both branches are `todo!()` |
| Named arguments | ❌ | No argument name resolution |
| Call options | ❌ | `{value: x, gas: y}` not checked |

**Current code** (checker.rs:150-179):
```rust
match callee_ty.kind {
    TyKind::FnPtr(_f) => {
        todo!()  // <-- All regular function calls
    }
    TyKind::Type(to) => self.check_explicit_cast(expr.span, to, args),
    TyKind::Event(..) | TyKind::Error(..) => {
        todo!()  // <-- Event/error instantiation
    }
    // ...
}
```

### User-Defined Value Types (UDVTs)

| Feature | Status | Notes |
|---------|--------|-------|
| Type representation | ✅ | `TyKind::Udvt` |
| As mapping keys | ✅ | Allowed in `check_mapping_key_type` |
| wrap/unwrap members | ✅ | Attached to `type(UDVT)` |
| Implicit conversions | ⚠️ | Basic rules only |
| Custom operators | ❌ | Not implemented |

### Overload Resolution

Current implementation only handles variables, not functions:

```rust
fn try_resolve_overloads(&self, res: &[hir::Res]) -> Result<hir::Res, OverloadError> {
    match res {
        [] => unreachable!(),
        &[res] => return Ok(res),
        _ => {}
    }
    // Only filters for variables, ignores functions/events/errors
    match res.iter().filter(|res| res.as_variable().is_some()).collect::<WantOne<_>>() {
        // ...
    }
}
```

### ABI Builtins

All `abi.*` functions have placeholder signatures with no argument checking:

| Builtin | Declared Signature | Expected Signature |
|---------|-------------------|-------------------|
| `abi.encode` | `() pure → bytes` | `(T...) pure → bytes` |
| `abi.encodePacked` | `() pure → bytes` | `(T...) pure → bytes` |
| `abi.encodeWithSelector` | `() pure → bytes` | `(bytes4, T...) pure → bytes` |
| `abi.encodeCall` | `() pure → bytes` | `(F, T...) pure → bytes` |
| `abi.encodeWithSignature` | `() pure → bytes` | `(string, T...) pure → bytes` |
| `abi.decode` | `() pure → ()` | `(bytes, (T...)) pure → T...` |

Similarly, `string.concat` and `bytes.concat` are stubs.

---

## Missing Features

### View/Pure Checker

**Status**: Not implemented at all.

While `StateMutability` is tracked on function types and builtins, there is no enforcement that:
- Pure functions don't read state
- View functions don't modify state
- Non-payable functions don't access `msg.value`
- Calls respect callee mutability

**solc test coverage**: 85+ tests in `syntaxTests/viewPureChecker/`

### Custom Operators (User-Defined Operators)

**Status**: Explicitly marked as TODO.

No support for:
- Associating operators with user-defined functions
- Operator overloading based on operand types
- Operator resolution for UDVTs

**solc test coverage**: `syntaxTests/operators/userDefinedOperators/`

### Using For

**Status**: Name resolution may work, but call semantics are missing.

Since function call checking (`FnPtr`) is unimplemented, library extension methods cannot be type-checked even if they resolve correctly.

**solc test coverage**: 100+ tests in `syntaxTests/using/`

### Interface Type Semantics

**Status**: `type(I).interfaceId` resolves but full semantics are missing.

TODO comment in code:
```rust
// TODO: implement `interfaceType`
```

---

## Test Suite Coverage Mapping

Based on `testdata/solidity/test/libsolidity/syntaxTests/` (3,499 total tests):

| Category | Tests | Solar Status |
|----------|-------|--------------|
| `viewPureChecker/` | 85+ | ❌ Not implemented |
| `functionCalls/` | 200+ | ⚠️ Partial (casts/structs only) |
| `functionTypes/` | 100+ | ⚠️ Representation only |
| `using/` | 100+ | ❌ Missing call semantics |
| `operators/` | 150+ | ⚠️ Built-in only |
| `userDefinedValueType/` | 50+ | ⚠️ Partial |
| `inheritance/` | 200+ | ⚠️ Contract-level checks only |
| `modifiers/` | 50+ | ❓ Unknown |
| `conversion/` | 100+ | ✅ Likely complete |
| `dataLocations/` | 100+ | ✅ Likely complete |
| `literals/` | 50+ | ✅ Complete |
| `array/` | 100+ | ✅ Likely complete |
| `structs/` | 100+ | ✅ Likely complete |
| `enums/` | 50+ | ✅ Likely complete |
| `constants/` | 50+ | ✅ Likely complete |
| `immutable/` | 50+ | ✅ Likely complete |
| `parsing/` | 100+ | ✅ Parser-level |
| `scoping/` | 100+ | ✅ Resolution-level |

---

## Implementation Roadmap

### High Priority (Core Functionality)

1. **Function Call Checking** (L)
   - Implement `FnPtr` branch in `ExprKind::Call`
   - Argument count/type checking
   - Return type inference
   
2. **Event/Error Call Checking** (M)
   - Treat as function calls without returns
   - Validate argument types

3. **Overload Resolution** (L-XL)
   - Filter candidates by kind
   - Evaluate applicability based on argument types
   - Handle literal conversions

### Medium Priority (Correctness)

4. **View/Pure Checker** (L)
   - Track current function mutability
   - Validate operations against mutability
   - Check call targets

5. **ABI Builtins** (M-L)
   - Implement variadic argument handling
   - Type validation for encodable types

### Lower Priority (Features)

6. **Custom Operators** (L-XL)
   - Operator-to-function mapping
   - UDVT operator support

7. **Using For** (M)
   - Resolve library methods as members
   - Integrate with call checking

---

## Code Locations

| Component | Location |
|-----------|----------|
| Type definitions | `crates/sema/src/ty/ty.rs` |
| Type interning | `crates/sema/src/ty/mod.rs` |
| Type checker entry | `crates/sema/src/typeck/mod.rs` |
| Expression checker | `crates/sema/src/typeck/checker.rs` |
| Builtins | `crates/sema/src/builtins/mod.rs` |
| Member resolution | `crates/sema/src/builtins/members.rs` |
| Constant evaluation | `crates/sema/src/eval.rs` |
| Reference solc tests | `testdata/solidity/test/libsolidity/syntaxTests/` |

---

## References

- [Solidity TypeChecker.cpp](https://github.com/ethereum/solidity/blob/develop/libsolidity/analysis/TypeChecker.cpp)
- [Solidity Types.cpp](https://github.com/ethereum/solidity/blob/develop/libsolidity/ast/Types.cpp)
- [Solidity Documentation - Types](https://docs.soliditylang.org/en/latest/types.html)
