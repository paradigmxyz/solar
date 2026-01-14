//@compile-flags: -Ztypeck
interface MyInterface {
    error MyCustomError(uint256, bool);
}

contract Test {
    function test() public returns(bytes4) {
        return bytes4(MyInterface.MyCustomError); //~ ERROR: member `MyCustomError` not found
    }
}
