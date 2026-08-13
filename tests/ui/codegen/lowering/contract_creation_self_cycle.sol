//@ compile-flags: --emit=bin

contract SelfCycle { //~ ERROR: recursive contract creation bytecode dependency
    function create() external {
        new SelfCycle();
    }
}
