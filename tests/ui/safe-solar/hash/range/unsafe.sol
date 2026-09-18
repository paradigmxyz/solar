//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: range 0x0102030405060708, 0, 8 => 0xfec062278915ba5c3c3af6ebf470b5afc94fedadf39fe78eea427b9aa5df9692
//@ run-call: range 0x0102030405060708, 2, 3 => 0xfe60c754eeb6f4271f086228744a2bb133832435a98f1d79b65583db7d2e406b
//@ run-call: range 0x0102030405060708, 8, 0 => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: range 0x0102, 1, 3 => 0x268a57dd1b34c2e6ecb5ff08bef387c519e2e9a00b39144d8c49f9a20444e051

// The same hash in assembly. Asked for three bytes at offset one of a
// two-byte buffer it does not fail: it hashes the one byte that exists and
// two that follow the buffer, here zero, and returns a digest of something
// that was never in the input.
// CHECK-LABEL: fn @range
// CHECK: keccak256
contract Unsafe {
    function range(bytes memory b, uint256 offset, uint256 count) public pure returns (bytes32 h) {
        assembly ("memory-safe") {
            h := keccak256(add(add(b, 0x20), offset), count)
        }
    }
}
