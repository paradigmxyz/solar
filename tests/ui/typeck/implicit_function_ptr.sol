//@compile-flags: -Ztypeck

// Tests for implicit function pointer conversions.
// Function pointers require exact parameter/return types and visibility.
// State mutability follows: pure -> view -> nonpayable, payable -> nonpayable.

contract C {
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
}

// Tests from solc for member access on function types with state mutability conversions.

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
