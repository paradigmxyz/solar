//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: root 0 => 0
//@ run-call: root 17 => 4
//@ run-call: root 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 340282366920938463463374607431768211455

// An integer square root: a first guess from the highest set bit, then
// Newton's step until it stops coming down.
// CHECK-LABEL: fn @root
// CHECK: clz
import {Math} from "solar:core/v1/Math.sol";

contract Safe {
    function root(uint256 x) public pure returns (uint256) {
        return Math.sqrt(x);
    }
}
