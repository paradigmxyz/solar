//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: shift 0x0102030405 => 0x0101020304
//@ run-call: between 0xaaaaaaaaaaaaaaaa, 0x1122334455667788 => 0xaaaa2233445566aa
//@ run-call-fail: between 0xaaaaaaaa, 0x1122334455667788 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A copy whose two ranges may be the same buffer. The contract is a move --
// the result is as if the source were read in full before the first write --
// which is exactly what `mcopy` is, so the shift right by one comes out
// right and the lowering is one `mcopy` with no disjointness proof.
// CHECK-LABEL: fn @shift
// CHECK: mcopy
// CHECK-NOT: mstore8
// CHECK-NOT: icall @copyInto
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    function shift(bytes memory b) public pure returns (bytes memory) {
        Bytes.copyInto(b, 1, b, 0, b.length - 1);
        return b;
    }

    function between(bytes memory dst, bytes memory src) public pure returns (bytes memory) {
        Bytes.copyInto(dst, 2, src, 1, 5);
        return dst;
    }
}
