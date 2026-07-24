//@ check-pass
// ported-from: test/libsolidity/syntaxTests/inheritance/override/ambiguous_base_functions_overridden_in_intermediate_base.sol

contract A {
    function f() external virtual {}
}
contract B {
    function f() external virtual {}
}
contract C is A, B {
    function f() external override(A, B) {}
}
contract X is C {}
