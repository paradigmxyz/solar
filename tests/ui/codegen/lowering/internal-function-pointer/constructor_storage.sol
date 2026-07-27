//@ run-call: callStoredOnly => 7

// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

contract ConstructorStoredFunctionPointer {
    function() internal returns (uint256) storedOnly;

    constructor() {
        storedOnly = onlyStored;
    }

    function onlyStored() internal pure returns (uint256) {
        return 7;
    }

    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}
