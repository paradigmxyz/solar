//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call-fail: boom 0xdeadbeef => 0xdeadbeef
//@ run-call-fail: boom 0x
//@ run-call: maybe 0x01, false => 7
//@ run-call-fail: maybe 0x0102, true => 0x0102

// Reverting with exact bytes: no `Error(string)` wrapper, no allocation.
// This is what bubbling another call's revert data needs.
// CHECK-LABEL: fn @boom
// CHECK: revert {{v[0-9]+}}, {{v[0-9]+}}
// CHECK-NOT: icall @raw
import {Revert} from "solar:core/v1/Revert.sol";

contract Safe {
    function boom(bytes memory data) public pure {
        Revert.raw(data);
    }

    function maybe(bytes memory data, bool go) public pure returns (uint256) {
        if (go) Revert.raw(data);
        return 7;
    }
}
