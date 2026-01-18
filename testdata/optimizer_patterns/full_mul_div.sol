// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for full precision mul/div optimizations
/// @dev Solar should recognize and optimize these common fixed-point patterns

contract FullMulDiv {
    uint256 internal constant WAD = 1e18;
    uint256 internal constant RAY = 1e27;

    /// @dev mulWad: (x * y) / 1e18 with overflow protection
    /// Optimal: Use fullMulDiv or direct assembly
    function mulWad(uint256 x, uint256 y) public pure returns (uint256 z) {
        assembly {
            // Check for overflow
            if gt(x, div(not(0), y)) {
                if y {
                    mstore(0x00, 0xbac65e5b) // MulWadFailed
                    revert(0x1c, 0x04)
                }
            }
            z := div(mul(x, y), WAD)
        }
    }

    /// @dev divWad: (x * 1e18) / y with overflow protection
    function divWad(uint256 x, uint256 y) public pure returns (uint256 z) {
        assembly {
            // Check for overflow
            if iszero(mul(y, iszero(gt(x, div(not(0), WAD))))) {
                mstore(0x00, 0x7c5f487d) // DivWadFailed
                revert(0x1c, 0x04)
            }
            z := div(mul(x, WAD), y)
        }
    }

    /// @dev Full precision x * y / d using 512-bit intermediates
    function fullMulDiv(uint256 x, uint256 y, uint256 d) public pure returns (uint256 z) {
        assembly {
            // Store x * y in [p1, p0]
            // p1 = high 256 bits, p0 = low 256 bits
            let mm := mulmod(x, y, not(0))
            let p0 := mul(x, y)
            let p1 := sub(sub(mm, p0), lt(mm, p0))

            // If d <= p1, overflow
            if iszero(gt(d, p1)) {
                mstore(0x00, 0xae47f702) // FullMulDivFailed
                revert(0x1c, 0x04)
            }

            // If high bits are zero, simple division
            if iszero(p1) {
                z := div(p0, d)
            }
            // Otherwise, full algorithm needed
            if p1 {
                // Factor out powers of 2 from d
                let r := and(d, sub(0, d)) // Lowest bit
                
                // Divide d by r (power of 2)
                d := div(d, r)
                
                // Shift p0 and p1
                p0 := div(p0, r)
                p0 := or(p0, mul(p1, add(div(sub(0, r), r), 1)))
                p1 := div(p1, r)

                // Compute modular inverse of d
                let inv := xor(2, mul(3, d))
                inv := mul(inv, sub(2, mul(d, inv)))
                inv := mul(inv, sub(2, mul(d, inv)))
                inv := mul(inv, sub(2, mul(d, inv)))
                inv := mul(inv, sub(2, mul(d, inv)))
                inv := mul(inv, sub(2, mul(d, inv)))

                z := mul(p0, inv)
            }
        }
    }

    /// @dev Safe mulWad without overflow (use when values known to be small)
    function mulWadUnchecked(uint256 x, uint256 y) public pure returns (uint256) {
        unchecked {
            return x * y / WAD;
        }
    }

    /// @dev mulWad with rounding up
    function mulWadUp(uint256 x, uint256 y) public pure returns (uint256 z) {
        assembly {
            if gt(x, div(not(0), y)) {
                if y {
                    mstore(0x00, 0xbac65e5b)
                    revert(0x1c, 0x04)
                }
            }
            z := add(iszero(iszero(mod(mul(x, y), WAD))), div(mul(x, y), WAD))
        }
    }

    /// @dev divWad with rounding up
    function divWadUp(uint256 x, uint256 y) public pure returns (uint256 z) {
        assembly {
            if iszero(mul(y, iszero(gt(x, div(not(0), WAD))))) {
                mstore(0x00, 0x7c5f487d)
                revert(0x1c, 0x04)
            }
            z := add(iszero(iszero(mod(mul(x, WAD), y))), div(mul(x, WAD), y))
        }
    }

    /// @dev Exponentiation by squaring in WAD
    function rpow(uint256 x, uint256 n, uint256 base) public pure returns (uint256 z) {
        assembly {
            z := mul(base, iszero(n))
            if x {
                z := xor(base, mul(xor(base, x), and(n, 1)))
                let half := shr(1, base)
                
                for { n := shr(1, n) } n { n := shr(1, n) } {
                    let xx := mul(x, x)
                    if iszero(eq(div(xx, x), x)) {
                        mstore(0x00, 0x49f7642b) // RPowOverflow
                        revert(0x1c, 0x04)
                    }
                    let xxRound := add(xx, half)
                    if lt(xxRound, xx) {
                        mstore(0x00, 0x49f7642b)
                        revert(0x1c, 0x04)
                    }
                    x := div(xxRound, base)
                    
                    if and(n, 1) {
                        let zx := mul(z, x)
                        if iszero(eq(div(zx, x), z)) {
                            mstore(0x00, 0x49f7642b)
                            revert(0x1c, 0x04)
                        }
                        let zxRound := add(zx, half)
                        if lt(zxRound, zx) {
                            mstore(0x00, 0x49f7642b)
                            revert(0x1c, 0x04)
                        }
                        z := div(zxRound, base)
                    }
                }
            }
        }
    }

    /// @dev Ray-based mul: (x * y) / 1e27
    function mulRay(uint256 x, uint256 y) public pure returns (uint256) {
        unchecked {
            return x * y / RAY;
        }
    }

    /// @dev Convert wad to ray
    function wadToRay(uint256 x) public pure returns (uint256) {
        return x * 1e9;
    }

    /// @dev Convert ray to wad (truncating)
    function rayToWad(uint256 x) public pure returns (uint256) {
        return x / 1e9;
    }
}
