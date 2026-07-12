// Tests for implicit function pointer conversions.
// Function pointers require exact parameter/return types.
// Function kinds must match.
// State mutability follows: pure -> view -> nonpayable, payable -> nonpayable.

contract C {
    function privateTarget() private pure returns (uint256) {
        return 1;
    }

    function internalTarget() internal pure returns (uint256) {
        return 1;
    }

    // === Valid: same function type ===
    function sameFnType() internal pure {
        function() external pure returns (uint256) f;
        function() external pure returns (uint256) g = f;
    }

    // === Valid: pure -> view (pure is more restrictive) ===
    function pureToView() internal pure {
        function() external pure returns (uint256) f;
        function() external view returns (uint256) g = f;
    }

    // === Valid: pure -> nonpayable ===
    function pureToNonpayable() internal pure {
        function() external pure returns (uint256) f;
        function() external returns (uint256) g = f;
    }

    // === Valid: view -> nonpayable ===
    function viewToNonpayable() internal pure {
        function() external view returns (uint256) f;
        function() external returns (uint256) g = f;
    }

    // === Valid: payable -> nonpayable ===
    function payableToNonpayable() internal pure {
        function() external payable returns (uint256) f;
        function() external returns (uint256) g = f;
    }

    // === Valid: private function -> internal function pointer ===
    function privateToInternal() internal pure {
        function() internal pure returns (uint256) f = privateTarget;
    }

    // === Invalid: view -> pure (view is less restrictive) ===
    function viewToPure() internal pure {
        function() external view returns (uint256) f;
        function() external pure returns (uint256) g = f; //~ ERROR: mismatched types
    }

    // === Invalid: nonpayable -> payable ===
    function nonpayableToPayable() internal pure {
        function() external returns (uint256) f;
        function() external payable returns (uint256) g = f; //~ ERROR: mismatched types
    }

    // === Invalid: pure -> payable ===
    function pureToPayable() internal pure {
        function() external pure returns (uint256) f;
        function() external payable returns (uint256) g = f; //~ ERROR: mismatched types
    }

    // === Invalid: different return type ===
    function differentReturnType() internal pure {
        function() external pure returns (uint256) f;
        function() external pure returns (uint128) g = f; //~ ERROR: mismatched types
    }

    // === Invalid: different parameter type ===
    function differentParamType() internal pure {
        function(uint256) external pure f;
        function(uint128) external pure g = f; //~ ERROR: mismatched types
    }

    // === Invalid: different visibility ===
    function differentVisibility() internal pure {
        function() external pure f;
        function() internal pure g = f; //~ ERROR: mismatched types
    }

    // === Invalid: internal function -> external function pointer ===
    function internalToExternal() internal pure {
        function() external pure returns (uint256) f = internalTarget; //~ ERROR: mismatched types
    }

    // === Invalid: private function -> external function pointer ===
    function privateToExternal() internal pure {
        function() external pure returns (uint256) f = privateTarget; //~ ERROR: mismatched types
    }
}
