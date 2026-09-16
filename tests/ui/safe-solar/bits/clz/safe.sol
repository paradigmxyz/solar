//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: leading 0 => 256
//@ run-call: leading 1 => 255
//@ run-call: leading 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0
//@ run-call: leading 255 => 248

// Counting leading zeros. The body is a binary search any compiler runs;
// on a target with the instruction it is one `clz`.
// CHECK-LABEL: fn @leading
// CHECK: clz
import {Bits} from "solar:core/v1/Bits.sol";

contract Safe {
    function leading(uint256 x) public pure returns (uint256) {
        return Bits.leadingZeros(x);
    }
}
