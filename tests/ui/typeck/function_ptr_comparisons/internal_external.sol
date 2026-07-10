//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_operators_between_internal_and_external_function_pointers.sol

contract C {
    function externalTestFunction() external {}
    function internalTestFunction() internal {}

    function compare() external returns (bool) {
        function() external externalPointer = this.externalTestFunction;
        function() internal internalPointer = internalTestFunction;
        assert(
            this.externalTestFunction == externalPointer &&
            internalPointer == internalTestFunction
        );
        assert(
            internalPointer != externalPointer && //~ ERROR: cannot apply builtin operator
            internalTestFunction != this.externalTestFunction //~ ERROR: cannot apply builtin operator
        );
        return true;
    }
}
