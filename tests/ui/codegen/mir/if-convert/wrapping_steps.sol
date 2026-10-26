//@ codegen-matrix: standard
//@ run-call: increment 5 => 6
//@ run-call: increment 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0
//@ run-call: decrement 5 => 4
//@ run-call: decrement 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: disagreeing 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 1
//@ run-call: disagreeing 5 => 6
//@ run-call: plus256 5 => 261
//@ run-call: plus256 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 255

// Checked Solidity spells wrapping steps with a test of the one value that would overflow; the
// arms agree there, so the steps cost what `unchecked` ones do.
contract WrappingSteps {
    function increment(uint256 x) external pure returns (uint256) {
        return x == type(uint256).max ? 0 : x + 1;
    }

    function decrement(uint256 x) external pure returns (uint256) {
        return x == 0 ? type(uint256).max : x - 1;
    }

    function disagreeing(uint256 x) external pure returns (uint256) {
        return x == type(uint256).max ? 1 : x + 1;
    }

    function plus256(uint256 x) external pure returns (uint256) {
        return x > type(uint256).max - 256 ? x - (type(uint256).max - 255) : x + 256;
    }
}
