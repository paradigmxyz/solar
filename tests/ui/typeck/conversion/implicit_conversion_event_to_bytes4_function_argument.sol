//@compile-flags: -Ztypeck
contract MyContract {
    event MyCustomEvent(uint256);
    function f(bytes4 arg) public {}
    function test() public {
        f(MyCustomEvent); //~ ERROR: not yet implemented
    }
}
