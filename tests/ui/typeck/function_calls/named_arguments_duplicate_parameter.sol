//@compile-flags: -Ztypeck

contract test {
    function a(uint a, uint b) public returns (uint r) {
        r = a + b;
    }
    function b() public returns (uint r) {
        r = a({a: 1, a: 2}); //~ ERROR: duplicate named argument
    }
}
