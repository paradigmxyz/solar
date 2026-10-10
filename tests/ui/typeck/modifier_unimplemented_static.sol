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

abstract contract D {
    modifier m(uint) virtual;
    function f() D.m(1) public {} //~ ERROR: cannot call unimplemented modifier
    function g() m(1) public {}
}

contract E is D {
    modifier m(uint) override { _; }
    function h() D.m(1) public {} //~ ERROR: cannot call unimplemented modifier
    function i() E.m(1) public {}
    function j() m(1) public {}
}
