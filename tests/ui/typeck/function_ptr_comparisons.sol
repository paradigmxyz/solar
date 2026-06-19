//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/functionTypes/comparison_operators_for_external_functions.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_of_function_types_internal_eq_1.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_of_function_types_internal_eq_2.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_operators_between_internal_and_external_function_pointers.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_operators_external_functions_with_different_parameters.sol

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
