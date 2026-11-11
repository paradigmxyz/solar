// Source specifications hold after every optimization: each `@custom:ensures`
// clause, under the function's `@custom:requires` clauses, is proved on the
// optimized MIR.
contract Specs {
    /// @custom:requires x < 100
    /// @custom:ensures r > x && r <= 100
    function next(uint256 x) external pure returns (uint256 r) {
        r = x + 1;
    }

    /// @custom:ensures r >= a && r >= b
    /// @custom:ensures r == a || r == b
    function max(uint256 a, uint256 b) external pure returns (uint256 r) {
        if (a > b) return a;
        return b;
    }

    /// @custom:ensures r <= x
    function half(uint8 x) external pure returns (uint8 r) {
        return x / 2;
    }

    /// @custom:ensures neg == (x < 0)
    function sign(int256 x) external pure returns (bool neg) {
        neg = x < 0;
    }
}
