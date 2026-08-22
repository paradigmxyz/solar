//@ run-call: ConstantIntegerValue::calc() => 1020847100762815390390123822295304

library ConstantMath {
    uint256 internal constant MAX_UINT256 = 2**256 - 1;
    uint256 internal constant WAD = 1e18;

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
}

contract ConstantIntegerValue {
    using ConstantMath for uint256;

    function calc() external pure returns (uint256) {
        uint256 liquidity = 10_000 ether;
        uint256 swapAmount = 10 ether;
        return swapAmount.mulWadDown(0.003e18).mulDivDown(2**128, liquidity);
    }
}
