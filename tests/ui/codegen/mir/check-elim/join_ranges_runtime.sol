//@ codegen-matrix: standard
//@ run-call: choose true, 99, 79 => 100
//@ run-call: choose false, 99, 79 => 80
//@ run-call-fail: choose true, 100, 79
//@ run-call-fail: choose false, 99, 80
//@ run-call: boundedLoop 0 => 0
//@ run-call: boundedLoop 100 => 100
//@ run-call: wrapLoop 257 => 1
//@ run-call: wrapLoop 511 => 255
//@ run-call-fail: checked 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
contract JoinedRanges {
    function choose(bool choice, uint256 a, uint256 b) external pure returns (uint256) {
        uint256 x;
        if (choice) { require(a < 100); x = a; }
        else { require(b < 80); x = b; }
        require(x < 100);
        return x + 1;
    }
    function boundedLoop(uint256 limit) external pure returns (uint256 i) {
        require(limit <= 100);
        while (i < limit) ++i;
        require(i <= 100);
    }
    function wrapLoop(uint256 limit) external pure returns (uint8 x) {
        for (uint256 i; i < limit; ++i) { unchecked { ++x; } }
    }
    function checked(uint256 limit) external pure returns (uint8 x) {
        for (uint256 i; i < limit; ++i) ++x;
    }
}
