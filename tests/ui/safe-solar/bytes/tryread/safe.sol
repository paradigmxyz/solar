//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: peek 0xdeadbeef01, 0 => true, 0xdeadbeef
//@ run-call: peek 0xdeadbeef01, 1 => true, 0xadbeef01
//@ run-call: peek 0xdeadbeef01, 3 => false, 0x00000000

// A read that answers instead of reverting: out of range is `false` and zero.
// CHECK-LABEL: fn @peek
// CHECK: mload
// CHECK-NOT: icall @tryReadBytes4
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    function peek(bytes memory b, uint256 offset) public pure returns (bool, bytes4) {
        bytes memory next = hex"11223344";
        next;
        return Bytes.tryReadBytes4(b, offset);
    }
}
