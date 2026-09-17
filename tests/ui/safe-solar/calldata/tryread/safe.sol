//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: peek 0xdeadbeef01, 0 => true, 0xdeadbeef
//@ run-call: peek 0xdeadbeef01, 1 => true, 0xadbeef01
//@ run-call: peek 0xdeadbeef01, 3 => false, 0x00000000

// A calldata read that answers instead of reverting.
// CHECK-LABEL: fn @peek
// CHECK: calldataload
// CHECK-NOT: icall @tryReadBytes4
import {CalldataBytes} from "solar:core/v1/CalldataBytes.sol";

contract Safe {
    function peek(bytes calldata b, uint256 offset) public pure returns (bool, bytes4) {
        return CalldataBytes.tryReadBytes4(b, offset);
    }
}
