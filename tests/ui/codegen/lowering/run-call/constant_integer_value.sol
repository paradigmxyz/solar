//@ codegen-matrix: standard
//@ run-call: ConstantIntegerValue::calc => 1020847100762815390390123822295304
//@ run-call: ConstantIntegerValue::avgEdge => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: ConstantIntegerValue::negativeConstant => true
//@ run-call: ConstantIntegerValue::decimal => 300000000000000000
//@ run-call: ConstantIntegerValue::negativeExponent => 5
//@ run-call: ConstantIntegerValue::fraction => 1
//@ run-call: ConstantIntegerValue::fractionCompare => true
//@ run-call: ConstantIntegerValue::typedDivision => 0
//@ run-call: ConstantIntegerValue::zeroNegativePower => 0

library ConstantMath {
    uint256 internal constant MAX_UINT256 = 2**256 - 1;
    uint256 internal constant WAD = 1e18;
    int24 internal constant MIN_TICK = -887272;

    function mulWadDown(uint256 x, uint256 y) internal pure returns (uint256) {
        return mulDivDown(x, y, WAD);
    }

    function mulDivDown(uint256 x, uint256 y, uint256 denominator)
        internal
        pure
        returns (uint256 z)
    {
        assembly {
            if iszero(mul(denominator, iszero(mul(y, gt(x, div(MAX_UINT256, y)))))) {
                revert(0, 0)
            }
            z := div(mul(x, y), denominator)
        }
    }

    function isMinTick(int24 tick) internal pure returns (bool) {
        return tick == MIN_TICK;
    }
}

contract ConstantIntegerValue {
    using ConstantMath for uint256;

    uint constant ONE = 1;
    uint constant HALF = ONE / 2;

    function zeroNegativePower() external pure returns (uint) { 0.5; return 0 ** -1; }

    function decimal() external pure returns (uint) { return 0.3 * 10**18; }
    function negativeExponent() external pure returns (uint) { return 50 * 10**8 * 10**-9; }
    function fraction() external pure returns (uint) { return (1 / 2) * 2; }
    function fractionCompare() external pure returns (bool) { return 0.3 < 0.5; }
    function typedDivision() external pure returns (uint) { return HALF; }

    function calc() external pure returns (uint256) {
        uint256 liquidity = 10_000 ether;
        uint256 swapAmount = 10 ether;
        return swapAmount.mulWadDown(0.003e18).mulDivDown(2**128, liquidity);
    }

    function avgEdge() external pure returns (uint256) {
        return average(uint256(2**256 - 1), 1);
    }

    function negativeConstant() external pure returns (bool) {
        return ConstantMath.isMinTick(-887272);
    }

    function average(uint256 x, uint256 y) internal pure returns (uint256) {
        unchecked {
            return (x & y) + ((x ^ y) >> 1);
        }
    }
}
