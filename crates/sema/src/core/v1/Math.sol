// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice How `Math.mulDiv` rounds a quotient that is not exact.
enum Rounding {
    Down,
    Up
}

/// @notice Full-width multiplication and division, and arithmetic that wraps
/// by name.
/// @dev Compiler-owned module, imported as `solar:core/v1/Math.sol`.
/// `mul512` and `mulDiv` never lose the high half of a product: `mulDiv`
/// reverts with `Panic(0x12)` when the denominator is zero and with
/// `Panic(0x11)` when the rounded quotient does not fit 256 bits, and is
/// exact otherwise. The `wrapping` operations are modular arithmetic spelled
/// as such, for the algorithms that mean it, so that a whole function need
/// not become `unchecked` to say so. Every body is ordinary Solidity and
/// defines the behaviour.
library Math {
    /// @dev The 512-bit product of `x` and `y`, as its two words.
    function mul512(uint256 x, uint256 y) internal pure returns (uint256 high, uint256 low) {
        unchecked {
            // The product modulo `2**256 - 1` is `high + low` there, so the
            // difference from the low word is the high one, less a borrow.
            uint256 folded = mulmod(x, y, type(uint256).max);
            low = x * y;
            high = folded - low;
            if (folded < low) high -= 1;
        }
    }

    /// @dev `x * y / denominator` at full precision, rounded as `rounding`
    /// says.
    function mulDiv(uint256 x, uint256 y, uint256 denominator, Rounding rounding)
        internal
        pure
        returns (uint256 result)
    {
        result = _mulDivDown(x, y, denominator);
        // The quotient exists, so the denominator is not zero here. Rounding
        // up the largest quotient does not fit and fails as an overflow.
        if (rounding == Rounding.Up && mulmod(x, y, denominator) != 0) result += 1;
    }

    /// @dev `x + y` modulo `2**256`.
    function wrappingAdd(uint256 x, uint256 y) internal pure returns (uint256) {
        unchecked {
            return x + y;
        }
    }

    /// @dev `x - y` modulo `2**256`.
    function wrappingSub(uint256 x, uint256 y) internal pure returns (uint256) {
        unchecked {
            return x - y;
        }
    }

    /// @dev `x * y` modulo `2**256`.
    function wrappingMul(uint256 x, uint256 y) internal pure returns (uint256) {
        unchecked {
            return x * y;
        }
    }

    /// @dev `floor(x * y / denominator)`, after Remco Bloemen's division of a
    /// 512-bit number by a 256-bit one.
    function _mulDivDown(uint256 x, uint256 y, uint256 denominator) private pure returns (uint256) {
        (uint256 high, uint256 low) = mul512(x, y);
        // A product that fits one word is an ordinary division, which also
        // raises the division-by-zero panic.
        if (high == 0) return low / denominator;
        // The quotient fits only if the denominator exceeds the high word.
        if (denominator <= high) {
            if (denominator == 0) return low / denominator;
            _overflow();
        }
        unchecked {
            // Make the division exact by taking the remainder off the product.
            uint256 remainder = mulmod(x, y, denominator);
            if (remainder > low) high -= 1;
            low -= remainder;
            // Divide out the denominator's powers of two, moving the matching
            // bits of the high word down into the low one.
            uint256 twos = denominator & (0 - denominator);
            denominator /= twos;
            low /= twos;
            twos = (0 - twos) / twos + 1;
            low |= high * twos;
            // The denominator is odd now and has an inverse modulo `2**256`:
            // a seed correct to four bits, doubled six times by Newton's step.
            uint256 inverse = (3 * denominator) ^ 2;
            inverse *= 2 - denominator * inverse;
            inverse *= 2 - denominator * inverse;
            inverse *= 2 - denominator * inverse;
            inverse *= 2 - denominator * inverse;
            inverse *= 2 - denominator * inverse;
            inverse *= 2 - denominator * inverse;
            return low * inverse;
        }
    }

    /// @dev Raises the `Panic(0x11)` a checked addition raises.
    function _overflow() private pure {
        uint256 most = type(uint256).max;
        most += 1;
    }
}
