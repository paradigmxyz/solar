//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: three 0x414243 => 0x0000000000000000000000000000000000000000000000000000000000414243
//@ run-call: three 0x41424344 => 0x0000000000000000000000000000000000000000000000000000000000414243
//@ run-call: two 0x4142 => 0x0000000000000000000000000000000000000000000000000000000000004142
//@ run-call: gapped 0x41424344 => 0x0000000000000000000000000000000000000000000000000000000000004143
//@ run-call: stored 0x414243 => 0x00000000000000000000000000000000000000000000000000000000414a43

// Three adjacent byte reads packed in order are one field of one word read.
// CHECK-LABEL: fn @three
// CHECK: shr 232
// CHECK-NOT: byte 0

// Two adjacent reads are a run as well, at the width they cover.
// CHECK-LABEL: fn @two
// CHECK: shr 240

// Offsets that skip a byte are not a run, so the reads stay separate.
// CHECK-LABEL: fn @gapped
// CHECK-NOT: shr 2

// A store between the reads may change what the later ones see, so the read
// above it cannot join their run.
// CHECK-LABEL: fn @stored
// CHECK-NOT: shr 232

contract Test {
    function three(bytes memory s) public pure returns (uint256) {
        if (s.length < 3) return 0;
        return (uint256(uint8(s[0])) << 16) | (uint256(uint8(s[1])) << 8) | uint256(uint8(s[2]));
    }

    function two(bytes memory s) public pure returns (uint256) {
        if (s.length < 2) return 0;
        return (uint256(uint8(s[0])) << 8) | uint256(uint8(s[1]));
    }

    function gapped(bytes memory s) public pure returns (uint256) {
        if (s.length < 3) return 0;
        return (uint256(uint8(s[0])) << 8) | uint256(uint8(s[2]));
    }

    function stored(bytes memory s) public pure returns (uint256) {
        if (s.length < 3) return 0;
        uint256 first = uint256(uint8(s[0])) << 16;
        s[1] = 0x4a;
        return first | (uint256(uint8(s[1])) << 8) | uint256(uint8(s[2]));
    }
}
