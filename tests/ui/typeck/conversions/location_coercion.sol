//@compile-flags: -Ztypeck

// Tests for data location coercion rules.
// See: https://docs.soliditylang.org/en/latest/types.html#data-location-and-assignment-behaviour

contract C {
    uint256[] storageArr;

    // === Same location conversions (should all work) ===

    function memoryToMemory(uint256[] memory a) internal pure returns (uint256[] memory) {
        uint256[] memory b = a;
        return b;
    }

    function calldataToCalldata(uint256[] calldata a) external pure returns (uint256[] calldata) {
        uint256[] calldata b = a;
        return b;
    }

    function storageToStorage() internal {
        uint256[] storage a = storageArr;
        uint256[] storage b = a;
    }

    // === calldata -> memory (allowed, copy semantics) ===

    function calldataToMemory(uint256[] calldata a) external pure returns (uint256[] memory) {
        uint256[] memory b = a;
        return b;
    }

    // === memory/calldata -> storage (allowed, copy semantics) ===

    function memoryToStorage(uint256[] memory a) internal {
        storageArr = a;
    }

    function calldataToStorage(uint256[] calldata a) external {
        storageArr = a;
    }

    // === storage -> memory (allowed, copy semantics) ===

    function storageToMemory() internal view returns (uint256[] memory) {
        uint256[] memory a = storageArr;
        return a;
    }

    // === Disallowed conversions ===

    // storage -> calldata: never allowed
    function storageToCalldata() external {
        uint256[] calldata a = storageArr; //~ ERROR: mismatched types
    }

    // memory -> calldata: never allowed
    function memoryToCalldata(uint256[] memory a) external {
        uint256[] calldata b = a; //~ ERROR: mismatched types
    }

    // === Nested array tests ===

    function nestedMemoryToMemory(uint256[][] memory a) internal pure returns (uint256[][] memory) {
        uint256[][] memory b = a;
        return b;
    }

    function nestedCalldataToMemory(uint256[][] calldata a) external pure returns (uint256[][] memory) {
        uint256[][] memory b = a;
        return b;
    }

    // === Wrong element type tests ===

    function wrongElementType(uint256[] memory a) internal pure {
        uint128[] memory b = a; //~ ERROR: mismatched types
    }

    function wrongElementTypeCalldata(uint256[] calldata a) external pure {
        uint128[] memory b = a; //~ ERROR: mismatched types
    }
}
