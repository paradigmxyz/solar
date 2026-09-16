//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: selector 0xa9059cbb000000000000000000000000000000000000000000000000000000000000dead => 0xa9059cbb
//@ run-call: word 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 1 => 909953980780754722974929232440438614579330444661935074573822767059494182945
//@ run-call: selector 0xa9 => 0xa9000000
//@ run-call: word 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 2 => 1364040605240818234439913487376469625768624502174251011983460351404251554048

// The same reads written the way the assembly libraries write them. On valid
// input the two agree byte for byte. Past the end nothing fails: the one-byte
// packet yields a selector whose last three bytes were never in it, and the
// word read at offset two takes a byte from whatever follows the buffer.
// CHECK-LABEL: fn @selector
// CHECK: mload
// CHECK: and {{.*}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
contract Unsafe {
    function selector(bytes memory packet) public pure returns (bytes4 result) {
        assembly ("memory-safe") {
            result := and(mload(add(packet, 0x20)), shl(224, 0xffffffff))
        }
    }

    function word(bytes memory packet, uint256 offset) public pure returns (uint256 result) {
        assembly ("memory-safe") {
            result := mload(add(add(packet, 0x20), offset))
        }
    }
}
