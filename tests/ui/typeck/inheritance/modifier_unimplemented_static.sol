// ported-from: test/libsolidity/syntaxTests/modifiers/use_unimplemented_static.sol

contract A {
    modifier m() virtual { _; }
}
abstract contract B {
    modifier m() virtual;
}
contract C is A, B {
    modifier m() override(A, B) { _; }
    function f() B.m public {} //~ ERROR: cannot call unimplemented modifier
}
