//@compile-flags: -Ztypeck

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

// Ported from test/libsolidity/semanticTests/functionTypes/comparison_operators_for_external_functions.sol.
// Ported from test/libsolidity/syntaxTests/functionTypes/comparison_of_function_types_internal_eq_1.sol.
// Ported from test/libsolidity/syntaxTests/functionTypes/comparison_of_function_types_internal_eq_2.sol.
// Ported from test/libsolidity/syntaxTests/functionTypes/comparison_operators_between_internal_and_external_function_pointers.sol.
// Ported from test/libsolidity/syntaxTests/functionTypes/comparison_operators_external_functions_with_different_parameters.sol.
// Ported from test/libsolidity/syntaxTests/functionTypes/comparison_operator_for_external_functions_with_call_options.sol.
library FunctionComparisonLib {
    function f() public {}
    function g() public {}
}

contract FunctionComparison {
    function f() external {}
    function g() external {}
    function h() pure external {}
    function i() view external {}
    function externalWithUint(uint256) external {}
    function externalWithBool(bool) external {}
    function internalTarget() internal pure {}
    function internalOther() internal pure {}

    function externalComparisons() public returns (bool) {
        return this.f != this.g &&
            this.f != this.h &&
            this.f != this.i &&
            this.g != this.h &&
            this.g != this.i &&
            this.h != this.i &&
            this.f == this.f &&
            this.g == this.g &&
            this.h == this.h &&
            this.i == this.i;
    }

    function localExternalComparisons() public returns (bool) {
        function () external f_local = this.f;
        function () external g_local = this.g;
        function () external pure h_local = this.h;
        function () external view i_local = this.i;

        return f_local == this.f &&
            g_local == this.g &&
            h_local == this.h &&
            i_local == this.i &&
            f_local != this.g &&
            f_local != this.h &&
            f_local != this.i &&
            g_local != this.f &&
            g_local != this.h &&
            g_local != this.i &&
            h_local != this.f &&
            h_local != this.g &&
            h_local != this.i &&
            i_local != this.f &&
            i_local != this.g &&
            i_local != this.h &&
            f_local == f_local &&
            f_local != g_local &&
            f_local != h_local &&
            f_local != i_local &&
            g_local == g_local &&
            g_local != h_local &&
            g_local != i_local &&
            h_local == h_local &&
            i_local == i_local &&
            h_local != i_local;
    }

    function internalComparisons() public pure returns (bool) {
        function () internal ptr = internalTarget;
        return internalTarget == internalTarget &&
            ptr == internalOther &&
            ptr != internalTarget;
    }

    function invalidInternalExternal(function () external externalPtr) external returns (bool) {
        function () internal internalPtr = internalTarget;
        return internalPtr != externalPtr && //~ ERROR: cannot apply builtin operator
            internalTarget != this.f; //~ ERROR: cannot apply builtin operator
    }

    function invalidExternalParameters() external returns (bool) {
        function () external externalPtr1 = this.externalWithUint; //~ ERROR: mismatched types
        function () external externalPtr2 = this.externalWithBool; //~ ERROR: mismatched types

        return this.externalWithUint == externalPtr1 && //~ ERROR: cannot apply builtin operator
            this.externalWithBool == externalPtr2 && //~ ERROR: cannot apply builtin operator
            externalPtr2 != externalPtr1 &&
            this.externalWithBool != this.externalWithUint; //~ ERROR: cannot apply builtin operator
    }

    function invalidLibraryComparisons(function () external externalPtr) external view returns (bool) {
        return FunctionComparisonLib.f == externalPtr || //~ ERROR: cannot apply builtin operator
            FunctionComparisonLib.f == FunctionComparisonLib.g; //~ ERROR: cannot apply builtin operator
    }
}

// Ported from test/libsolidity/syntaxTests/conversion/function_type_nonpayable_payable.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_nonpayable_pure.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_nonpayable_view.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_payable_nonpayable.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_payable_pure.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_payable_view.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_pure_nonpayable.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_pure_payable.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_pure_view.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_view_nonpayable.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_view_payable.sol.
// Ported from test/libsolidity/syntaxTests/conversion/function_type_view_pure.sol.

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
