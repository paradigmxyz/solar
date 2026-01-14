//@compile-flags: -Ztypeck
interface MyInterface {
    error MyCustomError(uint256, bool);
}

contract Test {
    function test() public returns(bytes4) {
        return (MyInterface.MyCustomError); //~ ERROR: member `MyCustomError` not found
    }
}
