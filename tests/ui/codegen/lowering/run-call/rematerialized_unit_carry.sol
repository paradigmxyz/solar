//@ codegen-matrix: standard
//@ run-call: pair 0 => 1, false
//@ run-call: pair 1 => 2, false
//@ run-call: pair 115792089237316195423570985008687907853269984665640564039457584007913129639934 => 115792089237316195423570985008687907853269984665640564039457584007913129639935, false
//@ run-call: pair 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0, true

contract RematerializedUnitCarry {
    function pair(uint256 x) external pure returns (uint256 sum, bool carry) {
        unchecked {
            sum = x + 1;
            carry = sum < x;
        }
    }
}
