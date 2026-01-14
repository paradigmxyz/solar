//@compile-flags: -Ztypeck
// Should compile without errors
contract test {
    function f() public pure returns (bytes memory) {
        return bytes("abc");
    }
}
