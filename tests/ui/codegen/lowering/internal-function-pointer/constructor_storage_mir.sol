//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck: --check-prefix=BUILT

// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

// BUILT-LABEL: fn @_anonymous(
// BUILT: sstore 0, [[ONLY_STORED:[0-9]+]]
contract ConstructorStoredFunctionPointer {
    function() internal returns (uint256) storedOnly;

    constructor() {
        storedOnly = onlyStored;
    }

    function onlyStored() internal pure returns (uint256) {
        return 7;
    }

    // BUILT-LABEL: fn @callStoredOnly(
    // BUILT: [[STORED_ONLY:v[0-9]+]] = sload 0
    // BUILT: internal_call @__internal_dispatch_0, 1, [[STORED_ONLY]]
    // BUILT-LABEL: fn @__internal_dispatch_0(
    // BUILT: eq arg0, [[ONLY_STORED]]
    // BUILT: internal_call @onlyStored, 1
    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}
