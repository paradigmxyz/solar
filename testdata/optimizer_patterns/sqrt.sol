// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for sqrt and other math optimizations
/// @dev Solar should recognize these and emit optimal Newton-Raphson code

contract SqrtPatterns {
    /// @dev Integer square root using Babylonian method
    /// This is the Solady-style optimized version
    function sqrt(uint256 x) public pure returns (uint256 z) {
        assembly {
            // Initial estimate
            z := 181
            let y := x

            // Approximate log2
            if iszero(lt(y, 0x10000000000000000000000000000000000)) {
                y := shr(128, y)
                z := shl(64, z)
            }
            if iszero(lt(y, 0x1000000000000000000)) {
                y := shr(64, y)
                z := shl(32, z)
            }
            if iszero(lt(y, 0x10000000000)) {
                y := shr(32, y)
                z := shl(16, z)
            }
            if iszero(lt(y, 0x1000000)) {
                y := shr(16, y)
                z := shl(8, z)
            }

            // Scale initial estimate
            z := shr(18, mul(z, add(y, 65536)))

            // Newton-Raphson iterations
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))

            // Round down
            z := sub(z, lt(div(x, z), z))
        }
    }

    /// @dev Naive sqrt - what safe Solidity might generate
    function sqrtNaive(uint256 x) public pure returns (uint256 z) {
        if (x == 0) return 0;
        
        // Initial guess
        z = x;
        uint256 y = (z + 1) / 2;
        
        while (y < z) {
            z = y;
            y = (x / z + z) / 2;
        }
    }

    /// @dev Fixed-point square root (18 decimals)
    function sqrtWad(uint256 x) public pure returns (uint256) {
        // sqrt(x * 1e18) = sqrt(x) * sqrt(1e18) = sqrt(x) * 1e9
        return sqrt(x * 1e18);
    }

    /// @dev Cube root approximation
    function cbrt(uint256 x) public pure returns (uint256 z) {
        if (x == 0) return 0;
        
        // Initial estimate
        z = x;
        uint256 y;
        
        // Scale down
        assembly {
            let r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            z := shl(div(r, 3), 1)
        }
        
        // Halley's method iterations
        unchecked {
            for (uint256 i = 0; i < 7; i++) {
                y = z;
                z = (2 * z + x / (z * z)) / 3;
                if (z >= y) break;
            }
        }
    }

    /// @dev Check if perfect square
    function isPerfectSquare(uint256 x) public pure returns (bool) {
        if (x == 0) return true;
        uint256 s = sqrt(x);
        return s * s == x;
    }

    /// @dev Distance calculation (2D)
    function distance(int256 x1, int256 y1, int256 x2, int256 y2) public pure returns (uint256) {
        unchecked {
            int256 dx = x2 - x1;
            int256 dy = y2 - y1;
            uint256 dxSq = uint256(dx * dx);
            uint256 dySq = uint256(dy * dy);
            return sqrt(dxSq + dySq);
        }
    }
}
