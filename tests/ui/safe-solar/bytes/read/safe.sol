//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: selector 0xa9059cbb000000000000000000000000000000000000000000000000000000000000dead => 0xa9059cbb
//@ run-call: word 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 1 => 909953980780754722974929232440438614579330444661935074573822767059494182945
//@ run-call-fail: selector 0xa9 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: word 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A fixed-width read that checks the whole width fits. Past the end it is a
// `Panic(0x32)`, the same failure an index raises; inside, it is one masked
// word load and no loop.
// CHECK-LABEL: fn @selector
// CHECK: mload
// CHECK: and {{.*}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
// CHECK-NOT: icall @readBytes4
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    using Bytes for bytes;

    function selector(bytes memory packet) public pure returns (bytes4) {
        return packet.readBytes4(0);
    }

    function word(bytes memory packet, uint256 offset) public pure returns (uint256) {
        return packet.readUint256BE(offset);
    }
}
