//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: decode "00ff10" => 0x00ff10
//@ run-call: decode "DEADbeef" => 0xdeadbeef
//@ run-call: decode "zz" => 0x33
//@ run-call: decode "abc" => 0xabc0

// A decoder in assembly that trusts its input: every character goes through
// the same arithmetic, so `zz` is the byte 0x33, and the lone last digit of
// `abc` is paired with whatever follows the string, here a zero.
// CHECK-LABEL: fn @decode
// CHECK: mstore8
contract Unsafe {
    function decode(string memory data) public pure returns (bytes memory out) {
        assembly ("memory-safe") {
            let n := mload(data)
            let count := shr(1, add(n, 1))
            out := mload(0x40)
            mstore(out, count)
            let o := add(out, 0x20)
            mstore(0x40, and(add(add(o, count), 0x1f), not(0x1f)))
            for { let i := 0 } lt(i, n) { i := add(i, 2) } {
                let hi := byte(0, mload(add(add(data, 0x20), i)))
                let lo := byte(0, mload(add(add(data, 0x21), i)))
                hi := sub(and(hi, 0x5f), mul(7, gt(and(hi, 0x5f), 0x19)))
                lo := sub(and(lo, 0x5f), mul(7, gt(and(lo, 0x5f), 0x19)))
                mstore8(add(o, shr(1, i)), or(shl(4, sub(hi, 0x10)), and(sub(lo, 0x10), 0x0f)))
            }
        }
    }
}
