contract C {
    function g(string calldata _s) public {}
    function h(bytes calldata _b) public {}

    function f() public {
        g("hello");
        g(unicode"hello");
        h(hex"68656c6c6f");
    }
}
