//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:

// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

// CHECK-LABEL: fn @_anonymous(
// CHECK: sstore 0, [[ONLY_STORED:[0-9]+]]
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
    // CHECK: internal_call @__internal_dispatch_0, 1, [[STORED_ONLY]]
    // CHECK-LABEL: fn @__internal_dispatch_0(
    // CHECK: eq arg0, [[ONLY_STORED]]
    // CHECK: internal_call @onlyStored, 1
    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}
