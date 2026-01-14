//@compile-flags: -Ztypeck
interface MyInterface {
    error MyCustomError(uint256, bool);
}

contract MyContract {
    function f(bytes4 arg) public {}
    function test() public {
        f(MyInterface.MyCustomError); //~ ERROR: not yet implemented
    }
}
