//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: 0xda91254c => 0x00000000000000000000000022222222222222222222222222222222222222220000000000000000000000000000000000000000000000000000000000000001
//@ run-call: 0xcfae3217 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001668656c6c6f2c20666f7277617264656420776f726c6400000000000000000000
//@ run-call-fail: 0x132e4f3c0000000000000000000000000000000000000000000000000000000000000007 => 0xc77ea6410000000000000000000000000000000000000000000000000000000000000007

// A delegating proxy's fallback on `Calls.forwardDelegate`: the same copies
// and call as the assembly, and the response is returned, or reverted with,
// exactly. The compiler accepts the call only where raw output may end the
// call, in the fallback function, and not while a modifier still has code to
// run after it.
// CHECK-LABEL: fn @fallback
// CHECK: calldatacopy 0, 0, [[SIZE:v[0-9]+]]
// CHECK: delegatecall {{v[0-9]+}}, {{v[0-9]+}}, 0, [[SIZE]], 0, 0
// CHECK-DAG: revert 0, {{v[0-9]+}}
// CHECK-DAG: returndata 0, {{v[0-9]+}}
import {Calls} from "solar:core/v1/Calls.sol";
import {Target} from "./auxiliary/target.sol";

contract Safe {
    address private immutable implementation = address(new Target());

    fallback() external payable {
        Calls.forwardDelegate(implementation, msg.data);
    }
}
