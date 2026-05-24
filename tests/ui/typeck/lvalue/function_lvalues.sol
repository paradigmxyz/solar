//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/lvalues/functions.sol

contract FunctionLvalues {
    function f() internal {}

    function g() internal {
        g = f; //~ ERROR: expression has to be an lvalue
    }

    function h() external {}

    function i() external {
        this.i = this.h; //~ ERROR: expression has to be an lvalue
    }
}
