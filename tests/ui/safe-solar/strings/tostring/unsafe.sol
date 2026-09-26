//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: decimal 0 => "0"
//@ run-call: decimal 12345 => "12345"
//@ run-call: decimal 115792089237316195423570985008687907853269984665640564039457584007913129639935 => "115792089237316195423570985008687907853269984665640564039457584007913129639935"

// Solady's conversion, as shipped: digits written downwards from a fixed
// offset past the free pointer, the length patched in afterwards.
// CHECK-LABEL: fn @decimal
// CHECK: mstore8
contract Unsafe {
    function decimal(uint256 value) public pure returns (string memory) {
        return _toString(value);
    }

    function _toString(uint256 value) internal pure returns (string memory result) {
        /// @solidity memory-safe-assembly
        assembly {
            // The maximum value of a uint256 contains 78 digits (1 byte per digit), but
            // we allocate 0xa0 bytes to keep the free memory pointer 32-byte word aligned.
            // We will need 1 word for the trailing zeros padding, 1 word for the length,
            // and 3 words for a maximum of 78 digits.
            result := add(mload(0x40), 0x80)
            mstore(0x40, add(result, 0x20)) // Allocate memory.
            mstore(result, 0) // Zeroize the slot after the string.

            let end := result // Cache the end of the memory to calculate the length later.
            let w := not(0) // Tsk.
            // We write the string from rightmost digit to leftmost digit.
            // The following is essentially a do-while loop that also handles the zero case.
            for { let temp := value } 1 {} {
                result := add(result, w) // `sub(result, 1)`.
                // Store the character to the pointer.
                // The ASCII index of the '0' character is 48.
                mstore8(result, add(48, mod(temp, 10)))
                temp := div(temp, 10) // Keep dividing `temp` until zero.
                if iszero(temp) { break }
            }
            let n := sub(end, result)
            result := sub(result, 0x20) // Move the pointer 32 bytes back to make room for the length.
            mstore(result, n) // Store the length.
        }
    }
}
