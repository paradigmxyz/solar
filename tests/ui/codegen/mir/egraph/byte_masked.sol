//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: cleared 0x4142434445464748 => 0
//@ run-call: kept 0x4142434445464748 => 0x0000000000000000000000000000000000000000000000000000000000000044

import {Bytes} from "solar:core/v1/Bytes.sol";

// A four-byte read is the word masked to its first four bytes, so byte seven
// of it is a constant zero, and byte three is the read's own byte three.
// CHECK-LABEL: fn @cleared
// CHECK-NOT: byte
// CHECK-LABEL: fn @kept
// CHECK: byte 3,

contract Test {
    function cleared(bytes memory s) public pure returns (uint8) {
        bytes4 q = Bytes.readBytes4(s, 0);
        return uint8(bytes32(q)[7]);
    }

    function kept(bytes memory s) public pure returns (uint8) {
        bytes4 q = Bytes.readBytes4(s, 0);
        return uint8(bytes32(q)[3]);
    }
}
