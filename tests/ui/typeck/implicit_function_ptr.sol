//@compile-flags: -Ztypeck

// Tests for implicit function pointer conversions.
// Function pointers require exact parameter/return types.
// Visibility must match, except private functions can convert to internal function pointers.
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

// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_pure.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_view.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_pure.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_view.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_view.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_pure.sol

contract NonpayableToPayable {
    function h() external {}
    function f() view external returns (bytes4) {
        function() payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract NonpayableToPure {
    function h() external {}
    function f() view external returns (bytes4) {
        function() pure external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract NonpayableToView {
    function h() external {}
    function f() view external returns (bytes4) {
        function() view external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract PayableToNonpayable2 {
    function h() payable external {}
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}

contract PayableToPure {
    function h() payable external {}
    function f() view external returns (bytes4) {
        function() pure external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract PayableToView {
    function h() payable external {}
    function f() view external returns (bytes4) {
        function() view external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract PureToNonpayable {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}

contract PureToPayable {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract PureToView2 {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() view external g = this.h;
        return g.selector;
    }
}

contract ViewToNonpayable2 {
    int dummy;
    function h() view external { dummy; }
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}

contract ViewToPayable {
    function h() view external {}
    function f() view external returns (bytes4) {
        function() payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract ViewToPure2 {
    function h() view external {}
    function f() view external returns (bytes4) {
        function() pure external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
