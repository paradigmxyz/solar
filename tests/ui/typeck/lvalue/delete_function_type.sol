//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionTypes/delete_function_type_invalid.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/delete_external_function_type_invalid.sol

contract FunctionLvalues {
    function f() internal {}

    function h() external {}

    function testDelete() external {
        delete f; //~ ERROR: expression has to be an lvalue
        delete this.h; //~ ERROR: expression has to be an lvalue
    }
}
