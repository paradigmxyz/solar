// SPDX-License-Identifier: AGPL-3.0-only
pragma solidity >=0.8.0;

/// @notice Arithmetic library with operations for fixed-point numbers.
/// @author Solmate (https://github.com/Rari-Capital/solmate/blob/main/src/utils/FixedPointMathLib.sol)
/// @author Inspired by USM (https://github.com/usmfum/USM/blob/master/contracts/WadMath.sol)
/// @dev Modified to use pure Solidity instead of inline assembly for Solar compatibility
library FixedPointMathLib {
    /*//////////////////////////////////////////////////////////////
                    SIMPLIFIED FIXED POINT OPERATIONS
    //////////////////////////////////////////////////////////////*/

    uint256 internal constant WAD = 1e18; // The scalar of ETH and most ERC20s.

    function mulWadDown(uint256 x, uint256 y) internal pure returns (uint256) {
        return mulDivDown(x, y, WAD); // Equivalent to (x * y) / WAD rounded down.
    }

    function mulWadUp(uint256 x, uint256 y) internal pure returns (uint256) {
        return mulDivUp(x, y, WAD); // Equivalent to (x * y) / WAD rounded up.
    }

    function divWadDown(uint256 x, uint256 y) internal pure returns (uint256) {
        return mulDivDown(x, WAD, y); // Equivalent to (x * WAD) / y rounded down.
    }

    function divWadUp(uint256 x, uint256 y) internal pure returns (uint256) {
        return mulDivUp(x, WAD, y); // Equivalent to (x * WAD) / y rounded up.
    }

    /*//////////////////////////////////////////////////////////////
                    LOW LEVEL FIXED POINT OPERATIONS
    //////////////////////////////////////////////////////////////*/

    function mulDivDown(
        uint256 x,
        uint256 y,
        uint256 denominator
    ) internal pure returns (uint256 z) {
        require(denominator != 0, "ZERO_DENOMINATOR");
        
        // Check for overflow: if x != 0, then (x * y) / x should equal y
        if (x != 0) {
            z = x * y;
            require(z / x == y, "MUL_OVERFLOW");
        } else {
            z = 0;
        }
        
        z = z / denominator;
    }

    function mulDivUp(
        uint256 x,
        uint256 y,
        uint256 denominator
    ) internal pure returns (uint256 z) {
        require(denominator != 0, "ZERO_DENOMINATOR");
        
        // Check for overflow: if x != 0, then (x * y) / x should equal y
        if (x != 0) {
            z = x * y;
            require(z / x == y, "MUL_OVERFLOW");
        } else {
            z = 0;
        }
        
        // Round up: if z > 0, return (z - 1) / denominator + 1
        if (z != 0) {
            z = (z - 1) / denominator + 1;
        }
    }

    function rpow(
        uint256 x,
        uint256 n,
        uint256 scalar
    ) internal pure returns (uint256 z) {
        if (x == 0) {
            z = n == 0 ? scalar : 0;
            return z;
        }
        
        z = n % 2 == 0 ? scalar : x;
        uint256 half = scalar / 2;
        
        n = n / 2;
        while (n > 0) {
            require(x <= type(uint128).max, "OVERFLOW");
            
            uint256 xx = x * x;
            uint256 xxRound = xx + half;
            require(xxRound >= xx, "OVERFLOW");
            
            x = xxRound / scalar;
            
            if (n % 2 != 0) {
                uint256 zx = z * x;
                if (x != 0) {
                    require(zx / x == z, "OVERFLOW");
                }
                
                uint256 zxRound = zx + half;
                require(zxRound >= zx, "OVERFLOW");
                
                z = zxRound / scalar;
            }
            
            n = n / 2;
        }
    }

    /*//////////////////////////////////////////////////////////////
                        GENERAL NUMBER UTILITIES
    //////////////////////////////////////////////////////////////*/

    function sqrt(uint256 x) internal pure returns (uint256 z) {
        if (x == 0) return 0;
        
        // Start off with z at 1
        z = 1;
        uint256 y = x;
        
        // Find the lowest power of 2 that is at least sqrt(x)
        if (y >= 0x100000000000000000000000000000000) {
            y = y >> 128;
            z = z << 64;
        }
        if (y >= 0x10000000000000000) {
            y = y >> 64;
            z = z << 32;
        }
        if (y >= 0x100000000) {
            y = y >> 32;
            z = z << 16;
        }
        if (y >= 0x10000) {
            y = y >> 16;
            z = z << 8;
        }
        if (y >= 0x100) {
            y = y >> 8;
            z = z << 4;
        }
        if (y >= 0x10) {
            y = y >> 4;
            z = z << 2;
        }
        if (y >= 0x8) {
            z = z << 1;
        }
        
        // Newton-Raphson iterations
        z = (z + x / z) >> 1;
        z = (z + x / z) >> 1;
        z = (z + x / z) >> 1;
        z = (z + x / z) >> 1;
        z = (z + x / z) >> 1;
        z = (z + x / z) >> 1;
        z = (z + x / z) >> 1;
        
        // Return the smaller of z and x/z
        uint256 zRoundDown = x / z;
        if (zRoundDown < z) {
            z = zRoundDown;
        }
    }
}
