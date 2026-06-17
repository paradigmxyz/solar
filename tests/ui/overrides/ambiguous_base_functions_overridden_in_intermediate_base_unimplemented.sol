// ported-from: test/libsolidity/syntaxTests/inheritance/override/ambiguous_base_functions_overridden_in_intermediate_base_unimplemented.sol

contract A {
    function f() external virtual {}
}
contract B {
    function f() external virtual {}
}
abstract contract C is A, B {
    function f() external override(A, B);
    //~^ ERROR: functions without implementation must be marked virtual
    //~| ERROR: cannot override implemented function with unimplemented function
    //~| ERROR: cannot override implemented function with unimplemented function
}
abstract contract X is C {}
