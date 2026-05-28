// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for string conversion optimizations
/// @dev Solar should optimize uint-to-string and other conversions

contract StringConversion {
    /// @dev Convert uint256 to decimal string (Solady-style)
    function toString(uint256 value) public pure returns (string memory result) {
        assembly {
            // Allocate memory
            result := add(mload(0x40), 0x80)
            mstore(0x40, add(result, 0x20))
            mstore(result, 0)
            
            let end := result
            let w := not(0) // -1, used for decrement
            
            // Write digits right-to-left
            for { let temp := value } 1 {} {
                result := add(result, w) // result--
                mstore8(result, add(48, mod(temp, 10)))
                temp := div(temp, 10)
                if iszero(temp) { break }
            }
            
            let length := sub(end, result)
            result := sub(result, 0x20)
            mstore(result, length)
        }
    }

    /// @dev Naive toString - what safe Solidity generates
    function toStringNaive(uint256 value) public pure returns (string memory) {
        if (value == 0) {
            return "0";
        }
        
        // Count digits
        uint256 temp = value;
        uint256 digits;
        while (temp != 0) {
            digits++;
            temp /= 10;
        }
        
        // Allocate and fill
        bytes memory buffer = new bytes(digits);
        while (value != 0) {
            digits--;
            buffer[digits] = bytes1(uint8(48 + value % 10));
            value /= 10;
        }
        
        return string(buffer);
    }

    /// @dev Convert int256 to string (handles negative)
    function toString(int256 value) public pure returns (string memory) {
        if (value >= 0) {
            return toString(uint256(value));
        }
        
        // Handle negative
        unchecked {
            string memory unsigned = toString(uint256(-value));
            return string(abi.encodePacked("-", unsigned));
        }
    }

    /// @dev Convert to hex string (Solady-style)
    function toHexString(uint256 value) public pure returns (string memory result) {
        assembly {
            result := add(mload(0x40), 0x80)
            mstore(0x40, add(result, 0x20))
            mstore(result, 0)
            
            let end := result
            let w := not(0)
            
            // Write hex digits right-to-left
            for { let temp := value } 1 {} {
                result := add(result, w)
                let char := and(temp, 0xf)
                // 0-9 -> 48-57, a-f -> 97-102
                mstore8(result, add(char, add(48, mul(39, gt(char, 9)))))
                temp := shr(4, temp)
                if iszero(temp) { break }
            }
            
            // Add "0x" prefix
            result := add(result, w)
            mstore8(result, 0x78) // 'x'
            result := add(result, w)
            mstore8(result, 0x30) // '0'
            
            let length := sub(end, result)
            result := sub(result, 0x20)
            mstore(result, length)
        }
    }

    /// @dev Convert address to hex string (checksummed)
    function toHexString(address addr) public pure returns (string memory) {
        return toHexString(uint256(uint160(addr)));
    }

    /// @dev Convert bytes32 to hex string
    function toHexString(bytes32 value) public pure returns (string memory result) {
        assembly {
            result := mload(0x40)
            mstore(0x40, add(result, 0x60))
            mstore(result, 66) // "0x" + 64 hex chars
            
            mstore(add(result, 0x20), 0x3078) // "0x"
            
            let o := add(result, 0x22)
            for { let i := 0 } lt(i, 32) { i := add(i, 1) } {
                let b := byte(i, value)
                let hi := shr(4, b)
                let lo := and(b, 0xf)
                mstore8(o, add(hi, add(48, mul(39, gt(hi, 9)))))
                mstore8(add(o, 1), add(lo, add(48, mul(39, gt(lo, 9)))))
                o := add(o, 2)
            }
        }
    }

    /// @dev Parse string to uint (basic)
    function parseUint(string memory s) public pure returns (uint256 result) {
        bytes memory b = bytes(s);
        for (uint256 i = 0; i < b.length; i++) {
            uint8 c = uint8(b[i]);
            require(c >= 48 && c <= 57, "Not a digit");
            result = result * 10 + (c - 48);
        }
    }

    /// @dev String length in runes (UTF-8 aware)
    function runeCount(string memory s) public pure returns (uint256 result) {
        assembly {
            if mload(s) {
                mstore(0x00, div(not(0), 255))
                mstore(0x20, 0x0202020202020202020202020202020202020202020202020303030304040506)
                let o := add(s, 0x20)
                let end := add(o, mload(s))
                for { result := 1 } 1 { result := add(result, 1) } {
                    o := add(o, byte(0, mload(shr(250, mload(o)))))
                    if iszero(lt(o, end)) { break }
                }
            }
        }
    }

    /// @dev Check if string is 7-bit ASCII
    function is7BitASCII(string memory s) public pure returns (bool result) {
        assembly {
            result := 1
            let mask := shl(7, div(not(0), 255))
            let n := mload(s)
            if n {
                let o := add(s, 0x20)
                let end := add(o, n)
                for {} 1 {} {
                    if and(mask, mload(o)) {
                        result := 0
                        break
                    }
                    o := add(o, 0x20)
                    if iszero(lt(o, end)) { break }
                }
            }
        }
    }
}
