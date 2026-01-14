//@compile-flags: -Ztypeck
contract Test {
    event MyCustomEvent(uint256);

    function test() public returns(bytes4) {
        return bytes4(MyCustomEvent); //~ ERROR: invalid explicit type conversion
    }
}
