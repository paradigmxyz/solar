// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for memory copy and allocation optimizations
/// @dev Solar should optimize memory operations and avoid unnecessary allocations

contract MemoryCopy {
    /// @dev Copy bytes to new location (Solady-style 32-byte chunks)
    function copyBytes(bytes memory src) public pure returns (bytes memory dst) {
        assembly {
            let len := mload(src)
            dst := mload(0x40)
            mstore(dst, len)
            
            let srcPtr := add(src, 0x20)
            let dstPtr := add(dst, 0x20)
            let end := add(srcPtr, len)
            
            // Copy 32 bytes at a time
            for {} lt(srcPtr, end) {} {
                mstore(dstPtr, mload(srcPtr))
                srcPtr := add(srcPtr, 0x20)
                dstPtr := add(dstPtr, 0x20)
            }
            
            // Update free memory pointer
            mstore(0x40, dstPtr)
        }
    }

    /// @dev Concat two bytes (Solady-style)
    function concat(bytes memory a, bytes memory b) public pure returns (bytes memory result) {
        assembly {
            let aLen := mload(a)
            let bLen := mload(b)
            let totalLen := add(aLen, bLen)
            
            result := mload(0x40)
            mstore(result, totalLen)
            
            let dst := add(result, 0x20)
            let src := add(a, 0x20)
            let end := add(src, aLen)
            
            // Copy a
            for {} lt(src, end) {} {
                mstore(dst, mload(src))
                src := add(src, 0x20)
                dst := add(dst, 0x20)
            }
            
            // Adjust dst for alignment
            dst := add(add(result, 0x20), aLen)
            src := add(b, 0x20)
            end := add(src, bLen)
            
            // Copy b
            for {} lt(src, end) {} {
                mstore(dst, mload(src))
                src := add(src, 0x20)
                dst := add(dst, 0x20)
            }
            
            // Zero-pad and update free memory pointer
            mstore(add(add(result, 0x20), totalLen), 0)
            mstore(0x40, add(dst, 0x20))
        }
    }

    /// @dev Slice bytes
    function slice(bytes memory data, uint256 start, uint256 length) 
        public 
        pure 
        returns (bytes memory result) 
    {
        require(start + length <= data.length, "Out of bounds");
        
        assembly {
            result := mload(0x40)
            mstore(result, length)
            
            let src := add(add(data, 0x20), start)
            let dst := add(result, 0x20)
            let end := add(src, length)
            
            for {} lt(src, end) {} {
                mstore(dst, mload(src))
                src := add(src, 0x20)
                dst := add(dst, 0x20)
            }
            
            mstore(0x40, and(add(add(result, add(length, 0x20)), 0x1f), not(0x1f)))
        }
    }

    /// @dev Fill bytes with value
    function fill(uint256 length, bytes1 value) public pure returns (bytes memory result) {
        assembly {
            result := mload(0x40)
            mstore(result, length)
            
            // Create word filled with byte value
            let word := byte(0, value)
            word := or(word, shl(8, word))
            word := or(word, shl(16, word))
            word := or(word, shl(32, word))
            word := or(word, shl(64, word))
            word := or(word, shl(128, word))
            
            let dst := add(result, 0x20)
            let end := add(dst, length)
            
            for {} lt(dst, end) {} {
                mstore(dst, word)
                dst := add(dst, 0x20)
            }
            
            mstore(0x40, and(add(add(result, add(length, 0x20)), 0x1f), not(0x1f)))
        }
    }

    /// @dev Zero memory region (for security)
    function zeroMemory(uint256 ptr, uint256 length) public pure {
        assembly {
            let end := add(ptr, length)
            for {} lt(ptr, end) {} {
                mstore(ptr, 0)
                ptr := add(ptr, 0x20)
            }
        }
    }

    /// @dev Compare bytes for equality
    function equals(bytes memory a, bytes memory b) public pure returns (bool result) {
        assembly {
            result := eq(keccak256(add(a, 0x20), mload(a)), 
                        keccak256(add(b, 0x20), mload(b)))
            result := and(result, eq(mload(a), mload(b)))
        }
    }

    /// @dev Naive comparison - optimizer should recognize
    function equalsNaive(bytes memory a, bytes memory b) public pure returns (bool) {
        if (a.length != b.length) return false;
        for (uint256 i = 0; i < a.length; i++) {
            if (a[i] != b[i]) return false;
        }
        return true;
    }

    /// @dev Return memory directly without copy
    function directReturn(bytes memory data) public pure {
        assembly {
            let retStart := sub(data, 0x20)
            mstore(retStart, 0x20)
            return(retStart, add(mload(data), 0x40))
        }
    }

    /// @dev Load word at offset
    function loadWord(bytes memory data, uint256 offset) public pure returns (bytes32 result) {
        assembly {
            result := mload(add(add(data, 0x20), offset))
        }
    }

    /// @dev Reverse bytes
    function reverse(bytes memory data) public pure returns (bytes memory result) {
        uint256 len = data.length;
        result = new bytes(len);
        
        for (uint256 i = 0; i < len; i++) {
            result[i] = data[len - 1 - i];
        }
    }
}
