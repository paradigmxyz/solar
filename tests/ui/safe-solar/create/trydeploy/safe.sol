//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: attempt 0x63deadbeef6000526004601cf3 => true, 0xdeadbeef
//@ run-call: attempt 0xfe => false, 0x

// A deployment whose failure is an answer, not a revert: the flag and the
// address come back as values, with no revert block and no call.
// CHECK-LABEL: fn @attempt
// CHECK: create
// CHECK-NOT: icall @tryDeploy
import {Create} from "solar:core/v1/Create.sol";

contract Safe {
    function attempt(bytes memory initcode) public returns (bool ok, bytes memory code) {
        address deployed;
        (ok, deployed) = Create.tryDeploy(initcode, 0);
        code = deployed.code;
    }
}
