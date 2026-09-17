//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: attempt 0x63deadbeef6000526004601cf3 => true, 0, 0
//@ run-call: attempt 0x7f333333333333333333333333333333333333333333333333333333333333333360005260206000fd => false, 4, 32

// A failed deployment whose revert data lands in a buffer the caller owns,
// bounded by that buffer.
// CHECK-LABEL: fn @attempt
// CHECK: create
// CHECK: returndatacopy
// CHECK-NOT: icall @tryDeployInto
import {Create} from "solar:core/v1/Create.sol";

contract Safe {
    function attempt(bytes memory initcode) public returns (bool ok, uint256 copied, uint256 total) {
        bytes memory diagnostics = new bytes(4);
        (ok,, copied, total) = Create.tryDeployInto(initcode, 0, diagnostics);
    }
}
