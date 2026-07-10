//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_of_function_types_internal_eq_2.sol

contract C {
    function f() internal {}
    function g() internal {}

    function test() public pure returns (bool) {
        function() internal ptr = C.f;
        return ptr == C.g;
    }
}
