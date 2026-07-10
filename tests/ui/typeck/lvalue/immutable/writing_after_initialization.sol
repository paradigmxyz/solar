//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/immutable/writing_after_initialization.sol

contract C {
    uint256 immutable x = 0;

    function f() internal {
        x = 1;
        //~^ ERROR: cannot assign to immutable here
        //~| HELP: immutables can only be assigned in state variable initializers, constructor arguments, or constructor bodies
    }
}
