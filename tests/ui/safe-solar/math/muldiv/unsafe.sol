//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: quotient 6, 7, 4 => 10
//@ run-call: quotient 100, 3, 7 => 42
//@ run-call: quotient 1606938044258990275541962092341162602522202993782792835301377, 1267650600228229401496703205377, 1125899906842625 => 1427247692705958613407685741222345439493226494
//@ run-call: quotient 6, 7, 0 => 0
//@ run-call: quotient 115792089237316195423570985008687907853269984665640564039457584007913129639935, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 1

// The same division as one `mul` and one `div`. A product that needs two
// words loses the high one and the quotient is of what is left; a zero
// denominator is a quotient of zero; neither is reported.
// CHECK-LABEL: fn @quotient
// CHECK: div
contract Unsafe {
    function quotient(uint256 x, uint256 y, uint256 denominator) public pure returns (uint256 z) {
        assembly ("memory-safe") {
            z := div(mul(x, y), denominator)
        }
    }
}
