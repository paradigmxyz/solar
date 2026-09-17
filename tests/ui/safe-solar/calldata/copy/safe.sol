//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: part 0x0102030405, 1, 3 => 0x020304
//@ run-call: part 0x0102030405, 5, 0 => 0x
//@ run-call-fail: part 0x0102, 1, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A range of a calldata slice copied into memory, checked against the slice.
// CHECK-LABEL: fn @part
// CHECK: calldatacopy
// CHECK-NOT: icall @copyInto
import {CalldataBytes} from "solar:core/v1/CalldataBytes.sol";

contract Safe {
    function part(bytes calldata src, uint256 start, uint256 count) public pure returns (bytes memory out) {
        out = new bytes(count);
        CalldataBytes.copyInto(out, 0, src, start, count);
    }
}
