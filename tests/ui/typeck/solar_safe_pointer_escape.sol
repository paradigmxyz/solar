library Factory {
    /// @custom:solar-trusted
    function pointer() internal pure returns (function() internal pure returns (uint256)) {
        return unsafeTarget;
    }

    function unsafeTarget() internal pure returns (uint256 result) {
        assembly { result := 42 } //~ ERROR: `Test` is tagged `@custom:solar-safe` but runs inline assembly
    }
}

/// @custom:solar-safe
contract Test {
    function run() external pure returns (uint256) {
        return Factory.pointer()();
    }
}

/// @custom:solar-safe
contract Local {
    /// @custom:solar-trusted
    function pointer() internal pure returns (function() internal pure returns (uint256)) {
        return unsafeTarget;
    }

    function unsafeTarget() internal pure returns (uint256 result) {
        assembly { result := 42 } //~ ERROR: `Local` is tagged `@custom:solar-safe` but runs inline assembly
    }

    function run() external pure returns (uint256) {
        return pointer()();
    }
}
