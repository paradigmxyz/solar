//@ codegen-matrix: standard
//@ run-call: allowed 0x, 0 => true
//@ run-call: allowed 0x30313233, 0x3ff000000000000 => true
//@ run-call: allowed 0x303132613334, 0x3ff000000000000 => false
//@ run-call: allowed 0x61, 0x3ff000000000000 => false
//@ run-call: find [], 7 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: find [3, 5, 7, 9], 7 => 2
//@ run-call: find [3, 5, 7, 9], 3 => 0
//@ run-call: find [3, 5, 7, 9], 4 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Caller::check 0x30313233 => true
//@ run-call: Caller::check 0x3031323a => false
// Loops whose header returns as soon as an element fails: gas builds copy the header into the
// latch, return included, and the results stay the same.
contract Test {
    function allowed(bytes memory b, uint256 lookup) public pure returns (bool) {
        for (uint256 i; i < b.length; ++i) {
            if (((lookup >> uint8(b[i])) & 1) == 0) return false;
        }
        return true;
    }

    function find(uint256[] memory a, uint256 x) public pure returns (uint256) {
        for (uint256 i; i < a.length; ++i) {
            if (a[i] == x) return i;
        }
        return type(uint256).max;
    }
}

contract Caller {
    function check(bytes memory b) public pure returns (bool) {
        return _digits(b) && b.length != 0;
    }

    function _digits(bytes memory b) private pure returns (bool) {
        for (uint256 i; i < b.length; ++i) {
            if (((0x3ff000000000000 >> uint8(b[i])) & 1) == 0) return false;
        }
        return true;
    }
}
