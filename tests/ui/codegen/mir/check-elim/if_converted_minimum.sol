//@ codegen-matrix: standard
//@ run-call: minimum 0, 0 => 0
//@ run-call: minimum 1, 2 => 1
//@ run-call: minimum 2, 1 => 1
//@ run-call: minimum 0, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0
//@ run-call: minimum 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 1
//@ run-call: minimum 115792089237316195423570985008687907853269984665640564039457584007913129639935, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: maximumIsMinimum 1, 2 => false
//@ run-call: maximumIsMinimum 2, 1 => false
//@ run-call: maximumIsMinimum 2, 2 => true
//@[mir] normalize-stdout-test: "(?s).+" -> ""

contract Test {
    function minimum(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 n = a < b ? a : b;
        assert(n <= a && n <= b);
        return n;
    }

    function maximumIsMinimum(uint256 a, uint256 b) public pure returns (bool) {
        uint256 n = a > b ? a : b;
        return n <= a && n <= b;
    }
}
