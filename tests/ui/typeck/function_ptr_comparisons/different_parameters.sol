//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_operators_external_functions_with_different_parameters.sol

contract C {
    function externalTestFunction1(uint256) external {}
    function externalTestFunction2(bool) external {}

    function compare() external returns (bool) {
        function() external pointer1 = this.externalTestFunction1; //~ ERROR: mismatched types
        function() external pointer2 = this.externalTestFunction2; //~ ERROR: mismatched types
        assert(
            this.externalTestFunction1 == pointer1 && //~ ERROR: cannot apply builtin operator
            this.externalTestFunction2 == pointer2 //~ ERROR: cannot apply builtin operator
        );
        assert(
            pointer2 != pointer1 &&
            this.externalTestFunction2 != this.externalTestFunction1 //~ ERROR: cannot apply builtin operator
        );
        return true;
    }
}
