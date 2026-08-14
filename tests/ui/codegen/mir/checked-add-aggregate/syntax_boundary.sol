//@ compile-flags: -O gas
//@ run-call: direct 1, 2, 3 => 6
//@ run-call: temporary 1, 2, 3 => 6
//@ run-call-fail: direct 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: temporary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract CheckedAddSyntaxBoundary {
    function direct(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a + b + c;
    }

    function temporary(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        uint256 d = a + b;
        return d + c;
    }
}
