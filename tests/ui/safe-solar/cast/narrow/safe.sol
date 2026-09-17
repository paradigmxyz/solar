//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: narrow 0 => 0
//@ run-call: narrow 1099511627775 => 1099511627775
//@ run-call-fail: narrow 1099511627776 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: narrow 1099511627783 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Narrowing that fails when the value does not fit.
// CHECK-LABEL: fn @narrow
// CHECK: 0xffffffffff
import {Cast} from "solar:core/v1/Cast.sol";

contract Safe {
    function narrow(uint256 x) public pure returns (uint40) {
        return Cast.toUint40(x);
    }
}
