//@compile-flags: -Ztypeck

contract test {
    function f(uint a, bool b, bytes32 c, uint d, bool e) public returns (uint r) {
        if (b && !e)
            r = a + d;
        else
            r = c.length;
    }
    function g() public returns (uint r) {
        r = f({c: "abc", x: 1, e: true, a: 11, b: true}); //~ ERROR: named argument `x` does not match function declaration
    }
}
