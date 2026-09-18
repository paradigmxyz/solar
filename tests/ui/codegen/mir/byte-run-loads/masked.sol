//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: quad 0x41424344 => 0x0000000000000000000000000000000000000000000000000000000000000041, 0x0000000000000000000000000000000000000000000000000000000000000042, 0x0000000000000000000000000000000000000000000000000000000000000043, 0x0000000000000000000000000000000000000000000000000000000000000044
//@ run-call: quad 0x4142434445 => 0x0000000000000000000000000000000000000000000000000000000000000041, 0x0000000000000000000000000000000000000000000000000000000000000042, 0x0000000000000000000000000000000000000000000000000000000000000043, 0x0000000000000000000000000000000000000000000000000000000000000044
//@ run-call: kept 0x41424344 => 0x41424344, 0x0000000000000000000000000000000000000000000000000000000000000042

import {Bytes} from "solar:core/v1/Bytes.sol";

// A four-byte read is the word masked to its first four bytes. Taken apart a
// byte at a time, every extraction reads a byte the mask keeps, so the
// extractions read the word and the mask is gone.
// CHECK-LABEL: fn @quad
// CHECK: {{v[0-9]+}} = mload
// CHECK: [[WORD:v[0-9]+]] = mload
// CHECK-NOT: and [[WORD]]
// CHECK: byte 1, [[WORD]]
// CHECK-NEXT: byte 2, [[WORD]]
// CHECK-NEXT: byte 3, [[WORD]]
// CHECK-NEXT: byte 0, [[WORD]]

// The masked word itself is returned as well, so the mask stays for all of
// its uses.
// CHECK-LABEL: fn @kept
// CHECK: [[MASKED:v[0-9]+]] = and {{v[0-9]+}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
// CHECK: byte 1, [[MASKED]]

contract Test {
    function quad(bytes memory s) public pure returns (uint8, uint8, uint8, uint8) {
        bytes4 q = Bytes.readBytes4(s, 0);
        return (uint8(q[0]), uint8(q[1]), uint8(q[2]), uint8(q[3]));
    }

    function kept(bytes memory s) public pure returns (bytes4, uint8) {
        bytes4 q = Bytes.readBytes4(s, 0);
        return (q, uint8(q[1]));
    }
}
