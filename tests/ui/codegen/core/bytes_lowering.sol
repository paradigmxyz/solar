//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    // Four bytes are a masked word read, not four indexed byte reads. The body
    // the module ships is what runs when the intrinsic is off.
    // INTRINSIC-LABEL: fn @selector
    // INTRINSIC: mload
    // INTRINSIC: and {{.*}}, 0xffffffff00000000000000000000000000000000000000000000000000000000
    // INTRINSIC-NOT: byte
    // PORTABLE-LABEL: fn @selector
    // PORTABLE: icall @readBytes4
    function selector(bytes memory packet) public pure returns (bytes4) {
        return packet.readBytes4(0);
    }

    // Writing four bytes preserves the twenty-eight around them, so the store
    // reads the old word back and merges under a constant mask.
    // INTRINSIC-LABEL: fn @patch
    // INTRINSIC: mload
    // INTRINSIC: and {{.*}}, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffff
    // INTRINSIC: mstore
    // INTRINSIC-NOT: mstore8
    function patch(bytes memory packet, bytes4 value) public pure returns (bytes memory) {
        packet.writeBytes4(1, value);
        return packet;
    }

    // Both ranges are parameters and may be the same buffer. The operation is
    // a move, which is what `mcopy` is, so no disjointness proof is needed --
    // the reason this cannot be an ordinary library loop.
    // INTRINSIC-LABEL: fn @move
    // INTRINSIC: mcopy
    // INTRINSIC-NOT: mstore8
    // PORTABLE-LABEL: fn @move
    // PORTABLE: icall @copyInto
    function move(bytes memory b, uint256 dst, uint256 src, uint256 n)
        public
        pure
        returns (bytes memory)
    {
        Bytes.copyInto(b, dst, b, src, n);
        return b;
    }

    // A fill stores the byte broadcast to a word, one word per iteration, and
    // merges the partial last word under a mask.
    // INTRINSIC-LABEL: fn @blank
    // INTRINSIC: mstore {{.*}}, 0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a
    // INTRINSIC-NOT: mstore8
    // PORTABLE-LABEL: fn @blank
    // PORTABLE: icall @fill
    function blank(bytes memory b, uint256 n) public pure returns (bytes memory) {
        Bytes.fill(b, 0, n, 0x2a);
        return b;
    }
}
