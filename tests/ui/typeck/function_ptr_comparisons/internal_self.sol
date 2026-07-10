//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionTypes/comparison_of_function_types_internal_eq_1.sol

contract C {
    function f() public pure returns (bool) {
        return f == f;
    }

    function g() public pure returns (bool) {
        return f != f;
    }
}
