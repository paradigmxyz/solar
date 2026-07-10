//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/immutable/ctor_indirect_initialization.sol

contract C {
    uint256 immutable x;

    constructor() {
        initX();
    }

    function initX() internal {
        x = 3;
        //~^ ERROR: cannot assign to immutable here
        //~| HELP: immutables can only be assigned in state variable initializers, constructor arguments, or constructor bodies
    }
}
