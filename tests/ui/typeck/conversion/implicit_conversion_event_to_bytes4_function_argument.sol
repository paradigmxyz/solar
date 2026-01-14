contract MyContract {
    event MyCustomEvent(uint256);
    function f(bytes4 arg) public {}
    function test() public {
        f(MyCustomEvent);
    }
}
