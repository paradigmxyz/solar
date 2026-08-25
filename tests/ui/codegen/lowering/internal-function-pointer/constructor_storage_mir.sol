//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

// CHECK-LABEL: fn @_anonymous(
// CHECK: sstore 0,
contract ConstructorStoredFunctionPointer {
    function() internal returns (uint256) storedOnly;

    constructor() {
        storedOnly = onlyStored;
    }

    function onlyStored() internal pure returns (uint256) {
        return 7;
    }

    // CHECK-LABEL: fn @callStoredOnly(
    // CHECK: [[STORED_ONLY:v[0-9]+]] = sload 0
    // CHECK: [[MASKED:v[0-9]+]] = and [[STORED_ONLY]], 0xffffffffffffffff
    // CHECK: internal_call @internal_dispatcher{{.*}}, 1, [[MASKED]]
    // CHECK-LABEL: fn @internal_dispatcher{{.*}}(
    // CHECK: eq arg0, 2
    // CHECK: internal_call @onlyStored, 1
    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}
