//@ codegen-matrix: standard
//@ run-call: masked 9, 4 => 46
//@ run-call: masked 7, 7 => 27
//@ run-call: masked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 1

contract ResidentWords {
    function masked(uint256 x, uint256 y) external pure returns (uint256) {
        uint256 different = x ^ y;
        uint256 common = x & y;
        uint256 either = x | y;
        return (either << 2) ^ (different << 1) ^ common;
    }
}
