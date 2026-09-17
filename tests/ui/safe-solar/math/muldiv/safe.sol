//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: quotient 6, 7, 4 => 10
//@ run-call: quotient 100, 3, 7 => 42
//@ run-call: quotient 1606938044258990275541962092341162602522202993782792835301377, 1267650600228229401496703205377, 1125899906842625 => 1809251394333063946555252381773327513630663086464846662395974532309886959613
//@ run-call-fail: quotient 6, 7, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: quotient 115792089237316195423570985008687907853269984665640564039457584007913129639935, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// A product divided at full precision: exact when the product needs two
// words, a panic when the denominator is zero or the quotient does not fit.
// CHECK-LABEL: fn @quotient
// CHECK: mulmod
import {Math, Rounding} from "solar:core/v1/Math.sol";

contract Safe {
    function quotient(uint256 x, uint256 y, uint256 denominator) public pure returns (uint256) {
        return Math.mulDiv(x, y, denominator, Rounding.Down);
    }
}
