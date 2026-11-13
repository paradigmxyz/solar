// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// ds-math's overflow test for a product, require(y == 0 || (z = x * y) / y == x), decided
// without a division (the product fits): every ok_* must verify, every bad_* must fail.

library SafeMath {
    function mul(uint256 x, uint256 y) internal pure returns (uint256 z) {
        unchecked {
            require(y == 0 || (z = x * y) / y == x, "ds-math-mul-overflow");
        }
    }
}

contract DsMath {
    using SafeMath for uint256;

    /// @custom:ensures r == a * b
    /// @custom:succeeds-if a * b < 2 ** 256
    function ok_mul(uint256 a, uint256 b) external pure returns (uint256 r) {
        r = a.mul(b);
    }

    /// The test rejects every product that wraps.
    // expect: counterexample
    /// @custom:succeeds-if true
    function bad_mul_total(uint256 a, uint256 b) external pure returns (uint256 r) {
        r = a.mul(b);
    }

    /// A wrong claim about the product is still caught.
    // expect: counterexample
    /// @custom:ensures r == a * b + 1
    function bad_mul_value(uint256 a, uint256 b) external pure returns (uint256 r) {
        r = a.mul(b);
    }

    /// Unchecked `!=`: true exactly when the product wraps.
    /// @custom:ensures wraps == (y != 0 && x * y >= 2 ** 256)
    function ok_wraps(uint256 x, uint256 y) external pure returns (bool wraps) {
        uint256 z;
        unchecked {
            wraps = y != 0 && (z = x * y) / y != x;
        }
    }

    /// The division by zero is still checked.
    // expect: y = 0
    /// @custom:succeeds-if true
    function bad_div_zero(uint256 x, uint256 y) external pure returns (bool) {
        uint256 z;
        unchecked {
            return (z = x * y) / y == x;
        }
    }
}
