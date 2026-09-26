//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: selector 0xdeadbeef01, 0 => 0xdeadbeef
//@ run-call: selector 0xdeadbeef01, 1 => 0xadbeef01
//@ run-call-fail: selector 0xdeadbeef, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Four bytes out of a calldata slice, checked against the slice.
// CHECK-LABEL: fn @selector
// CHECK: calldataload
// CHECK-NOT: icall @readBytes4
import {CalldataBytes} from "solar:core/v1/CalldataBytes.sol";

contract Safe {
    function selector(bytes calldata b, uint256 offset) public pure returns (bytes4) {
        return CalldataBytes.readBytes4(b, offset);
    }
}
