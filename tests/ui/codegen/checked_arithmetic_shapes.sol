//@compile-flags: -Zcodegen --emit=mir

// Pins the per-op checked-arithmetic check shapes so they stay at or below
// solc's happy-path gas:
// - unsigned add/sub: single `lt` against an operand (sub-word: `gt` max).
// - unsigned mul: `or(iszero(rhs), eq(div(p, rhs), lhs))` (sub-word <= 128
//   bits: `gt` max only).
// - signed add/sub: `xor` of two `slt`/`sgt` comparisons, no constants.
// - signed mul: division-inverse check plus `and(eq(lhs, MIN), slt(rhs, 0))`.
// - div/mod: branch directly on the divisor, no `iszero`/`eq` flag.
contract CheckedArithmeticShapes {
    function sadd(int256 a, int256 b) public pure returns (int256) {
        return a + b;
    }

    function ssub(int256 a, int256 b) public pure returns (int256) {
        return a - b;
    }

    function smul(int256 a, int256 b) public pure returns (int256) {
        return a * b;
    }

    function sdiv(int256 a, int256 b) public pure returns (int256) {
        return a / b;
    }

    function smod(int256 a, int256 b) public pure returns (int256) {
        return a % b;
    }

    function neg(int256 a) public pure returns (int256) {
        return -a;
    }

    function inc(uint256 a) public pure returns (uint256) {
        return ++a;
    }

    function dec(uint256 a) public pure returns (uint256) {
        return --a;
    }

    function uadd128(uint128 a, uint128 b) public pure returns (uint128) {
        return a + b;
    }

    function umul128(uint128 a, uint128 b) public pure returns (uint128) {
        return a * b;
    }

    function smul128(int128 a, int128 b) public pure returns (int128) {
        return a * b;
    }

    function umul192(uint192 a, uint192 b) public pure returns (uint192) {
        return a * b;
    }
}
