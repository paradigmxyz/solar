//@ codegen-matrix: standard
//@ run-call: words 0, 0 => true, 0, 0, 0
//@ run-call: words 255, 256 => false, 511, 255, 0
//@ run-call: words 0x1234, 0x1234 => true, 0, 52, 18
//@ run-call: words 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => false, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 255, 255
contract PairedViews {
    function words(uint256 x, uint256 y) external pure returns (bool equal, uint256 different, uint256 low, uint256 other) {
        assembly {
            equal := eq(xor(x, not(0)), xor(y, not(0)))
            different := xor(xor(x, not(0)), xor(y, not(0)))
            low := byte(0, shl(248, x))
            other := byte(0, shl(240, x))
        }
    }
}
