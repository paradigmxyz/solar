//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    // A view is the range itself: after the range check, the read and the hash
    // address the source's bytes, with nothing allocated or copied.
    // CHECK-LABEL: fn @viewed
    // CHECK-NOT: mcopy
    // CHECK: keccak256
    // CHECK-NOT: mcopy
    // CHECK: returndata
    function viewed(bytes memory packet) public pure returns (bytes4 tag, bytes32 hash) {
        /// @custom:solar-view
        bytes memory head = packet.slice(0, 5);
        tag = head.readBytes4(1);
        /// @custom:solar-view
        bytes memory body = packet.slice(5, packet.length - 5);
        hash = keccak256(body);
    }

    // Without the tag, each slice is a copy.
    // CHECK-LABEL: fn @copied
    // CHECK: mcopy
    // CHECK: mcopy
    // CHECK: keccak256
    function copied(bytes memory packet) public pure returns (bytes4 tag, bytes32 hash) {
        bytes memory head = packet.slice(0, 5);
        tag = head.readBytes4(1);
        bytes memory body = packet.slice(5, packet.length - 5);
        hash = keccak256(body);
    }
}
