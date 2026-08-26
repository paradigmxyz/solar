//@ codegen-matrix: standard
//@ run-call: ConstantIntegerValue::calc() => 1020847100762815390390123822295304
//@ run-call: ConstantIntegerValue::avgEdge() => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: ConstantIntegerValue::negativeConstant() => true

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
