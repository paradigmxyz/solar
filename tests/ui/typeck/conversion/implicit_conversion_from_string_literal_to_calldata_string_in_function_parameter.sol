//@compile-flags: -Ztypeck
// Solar does not report errors here (unlike solc)
contract C {
    function f1() public pure returns(string calldata) {
        return "hello";
    }

    function f2() public pure returns(string calldata) {
        return unicode"hello";
    }

    function f3() public pure returns(bytes calldata) {
        return hex"68656c6c6f";
    }
}
