//@ codegen-matrix: standard
//@ run-call: reused 7 => 47
//@ run-call: reused 0 => 26
//@ run-call: changed 7 => 109
//@ run-call-fail: reused 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract StorageReaders {
    uint256 private stored = 13;

    function reused(uint256 x) external view returns (uint256) {
        return compute(x) - stored;
    }

    function changed(uint256 x) external returns (uint256) {
        uint256 first = compute(x);
        stored = 14;
        return first + compute(x) - stored;
    }

    function compute(uint256 x) internal view returns (uint256) {
        return scale(x + stored);
    }

    function scale(uint256 x) internal pure returns (uint256) {
        return x * 3;
    }
}
