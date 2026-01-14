//@ compile-flags: -Ztypeck

contract C {
    function f() public returns (uint, uint) {
        try this() { //~ ERROR: expected function, found `contract C`
        } catch Error(string memory) {
        }
    }
}
