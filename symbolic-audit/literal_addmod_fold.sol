// `(2**255 + 2**255) % 7` is a literal expression and must be evaluated with
// unbounded precision at compile time, giving 2.
// solc `fold` returns 2 and `test` returns 0; solar reverts with Panic(0x11).
contract LiteralAddmodFold {
    function test() external pure returns (uint256) {
        if ((2**255 + 2**255) % 7 != addmod(2**255, 2**255, 7)) return 1;
        return 0;
    }

    function fold() external pure returns (uint256) {
        return (2**255 + 2**255) % 7;
    }
}
