//@compile-flags: -Ztypeck
// Solar does not report an error here (unlike solc)
contract Test {
    event MyCustomEvent(uint256);

    function test() public returns(bytes4) {
        return (MyCustomEvent);
    }
}
