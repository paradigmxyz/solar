//@compile-flags: -Ztypeck

// Tests for implicit function pointer conversions.
// See: https://docs.soliditylang.org/en/latest/types.html#function-types

contract C {
    // === Valid: same function type ===
    function pureFunc() internal pure returns (uint256) { return 1; }
    function viewFunc() internal view returns (uint256) { return 1; }
    function nonPayableFunc() internal returns (uint256) { return 1; }

    function assignSame() internal pure {
        function() internal pure returns (uint256) f = pureFunc;
    }

    // === Valid: pure -> view (more restrictive -> less restrictive) ===
    function pureToView() internal pure {
        function() internal view returns (uint256) f = pureFunc;
    }

    // === Valid: pure -> non-payable ===
    function pureToNonPayable() internal pure {
        function() internal returns (uint256) f = pureFunc;
    }

    // === Valid: view -> non-payable ===
    function viewToNonPayable() internal view {
        function() internal returns (uint256) f = viewFunc;
    }

    // === Invalid: non-payable -> pure (less restrictive -> more restrictive) ===
    function nonPayableToPure() internal {
        function() internal pure returns (uint256) f = nonPayableFunc; //~ ERROR: mismatched types
    }

    // === Invalid: non-payable -> view ===
    function nonPayableToView() internal {
        function() internal view returns (uint256) f = nonPayableFunc; //~ ERROR: mismatched types
    }

    // === Invalid: view -> pure ===
    function viewToPure() internal view {
        function() internal pure returns (uint256) f = viewFunc; //~ ERROR: mismatched types
    }

    // === Invalid: different parameter count ===
    function oneParam(uint256 x) internal pure returns (uint256) { return x; }

    function wrongParamCount() internal pure {
        function() internal pure returns (uint256) f = oneParam; //~ ERROR: mismatched types
    }

    // === Invalid: different return count ===
    function twoReturns() internal pure returns (uint256, uint256) { return (1, 2); }

    function wrongReturnCount() internal pure {
        function() internal pure returns (uint256) f = twoReturns; //~ ERROR: mismatched types
    }

    // === Invalid: different parameter types ===
    function intParam(int256 x) internal pure returns (uint256) { return 0; }

    function wrongParamType() internal pure {
        function(uint256) internal pure returns (uint256) f = intParam; //~ ERROR: mismatched types
    }
}
