//@compile-flags: -Ztypeck
contract C {
    function g(string calldata _s) public {}
    function h(bytes calldata _b) public {}

    function f() public {
        g("hello"); //~ ERROR: not yet implemented
        g(unicode"hello"); //~ ERROR: not yet implemented
        h(hex"68656c6c6f"); //~ ERROR: not yet implemented
    }
}
