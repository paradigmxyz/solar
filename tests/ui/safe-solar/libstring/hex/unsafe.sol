//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: toHex 3735928559, 4 => 0x6465616462656566
//@ run-call: toHex 1461501637330902918203684832716283019655932538316, 20 => 0x66666666666666666666666666666666666666666666666666666666666666666666666665646363
//@ run-call: toHex 0, 1 => 0x3030
//@ run-call: toHex 115792089237316195423570985008687907853269984665640564039457584007913129639935, 32 => 0x66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666
//@ run-call: toHex 2174026233105990997908215097954566016, 17 => 0x30303031613262336334643565366637303831393261336234633564366537663830
//@ run-call-fail: toHex 256, 1 => 0x2194895a

// Solady's encoder, as shipped: the digit table in scratch space and two byte
// stores per input byte, written from the right. On every input above the
// two agree byte for byte, and both refuse a value that does not fit with the
// same `HexLengthInsufficient()`.
// CHECK-LABEL: fn @toHex
// CHECK: mstore8
contract Unsafe {
    function toHex(uint256 value, uint256 byteCount) public pure returns (bytes memory result) {
        assembly ("memory-safe") {
            result := add(mload(0x40), and(add(shl(1, byteCount), 0x42), not(0x1f)))
            mstore(0x40, add(result, 0x20))
            mstore(result, 0)
            let end := result
            mstore(0x0f, 0x30313233343536373839616263646566)
            let start := sub(result, add(byteCount, byteCount))
            let w := not(1)
            let temp := value
            for {} 1 {} {
                result := add(result, w)
                mstore8(add(result, 1), mload(and(temp, 15)))
                mstore8(result, mload(and(shr(4, temp), 15)))
                temp := shr(8, temp)
                if iszero(xor(result, start)) { break }
            }
            if temp {
                mstore(0x00, 0x2194895a)
                revert(0x1c, 0x04)
            }
            let n := sub(end, result)
            result := sub(result, 0x20)
            mstore(result, n)
        }
    }
}
