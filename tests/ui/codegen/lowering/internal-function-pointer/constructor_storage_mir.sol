//@ revisions: built opt
//@[built] compile-flags: -Zcodegen -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[opt] compile-flags: -Zcodegen -Ogas -Zdump=mir-evm-shaped
//@[opt] filecheck: --check-prefix=OPT

// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

// BUILT-LABEL: fn @_anonymous(
// BUILT: sstore 0, [[ONLY_STORED:[0-9]+]]
// OPT: @phase evm-shaped
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
    // OPT-LABEL: fn @callStoredOnly(
    // OPT: [[STORED_ONLY:v[0-9]+]] = sload 0
    // OPT: eq [[STORED_ONLY]], {{[0-9]+}}
    // OPT: mstore 4, 81
    // OPT: mstore 128, 7
    // OPT-NOT: fn @__internal_dispatch_0(
    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}
