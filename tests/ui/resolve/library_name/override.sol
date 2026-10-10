// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_library.sol

library L {}
contract C {
    function f() public override (L) {}
    //~^ ERROR: invalid use of a library name
    //~| ERROR: Function has override specified but does not override anything
}
