//@ run-call: min() => 0
//@ run-call: max() => 3
// ported-from: test/libsolidity/semanticTests/enums/minmax.sol

contract test {
    enum MinMax {
        A,
        B,
        C,
        D
    }

    function min() public pure returns (uint256) {
        return uint256(type(MinMax).min);
    }

    function max() public pure returns (uint256) {
        return uint256(type(MinMax).max);
    }
}
