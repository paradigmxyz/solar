//@ compile-flags: -Zcodegen --emit=bin

contract SelfCycle { //~ ERROR: recursive contract creation bytecode dependency
    function create() external {
        new SelfCycle();
    }
}
