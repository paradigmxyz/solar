// ported-from: test/libsolidity/syntaxTests/inheritance/override/no_common_base_and_unique_implementation.sol

abstract contract A {
    function f() external {}
    function g() external virtual;
}
abstract contract B {
    function g() external {}
    function f() external virtual;
}
contract C is A, B {}
//~^ ERROR: derived contract must override function `f`
//~| ERROR: derived contract must override function `g`
