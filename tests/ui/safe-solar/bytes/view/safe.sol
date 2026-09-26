//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: header 0xa9059cbb00000000000000000000000000000000000000000000000000000000000000ff => 0xa9059cbb, 0xe08ec2af2cfc251225e1968fd6ca21e4044f129bffa95bac3503be8bdb30a367
//@ run-call: header 0xa9059cbb => 0xa9059cbb, 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call-fail: header 0xa905 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Views of a packet's header and body. Every other compiler copies each
// `Bytes.slice`; under `@custom:solar-view` this one reads the packet in place
// and rejects any write to the packet while a view is still read. What is left
// is the assembly version's code behind one range check: a packet shorter than
// its header is a `Panic(0x32)`.
// CHECK-LABEL: fn @header
// CHECK-NOT: mcopy
// CHECK: lt {{v[0-9]+}}, 4
// CHECK: keccak256
// CHECK: mload
// CHECK: and {{.*}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
// CHECK-NOT: mcopy
// CHECK: returndata
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    using Bytes for bytes;

    function header(bytes memory packet) public pure returns (bytes4 selector, bytes32 bodyHash) {
        /// @custom:solar-view
        bytes memory head = packet.slice(0, 4);
        selector = head.readBytes4(0);
        /// @custom:solar-view
        bytes memory body = packet.slice(4, packet.length - 4);
        bodyHash = keccak256(body);
    }
}
