//@compile-flags: -Ztypeck
// Solar does not report errors here (unlike solc)
pragma abicoder v2;

contract C {
    function f() public pure returns(string[5] calldata) {
        return ["h", "e", "l", "l", "o"];
    }
}
