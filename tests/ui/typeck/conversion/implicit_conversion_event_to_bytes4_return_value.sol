contract Test {
    event MyCustomEvent(uint256);

    function test() public returns(bytes4) {
        return (MyCustomEvent);
    }
}
