//@ codegen-matrix: standard
//@ run-call: same 3 => 17
//@ run-call: other 3 => 21
//@ run-call: writeBarrier 19 => 26
//@ run-call-fail: checked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x11)
contract StorageReaders {
    uint256 value = 7;
    uint256 unrelated = 11;

    function addValue(uint256 n) internal view returns (uint256) {
        return value + n;
    }

    function same(uint256 n) external view returns (uint256) {
        return addValue(n) + value;
    }

    function other(uint256 n) external view returns (uint256) {
        return addValue(n) + unrelated;
    }

    function checked(uint256 n) external view returns (uint256) {
        return addValue(n) + value;
    }

    function writeThenRead(uint256 n) internal returns (uint256) {
        value = n;
        return value;
    }

    function writeBarrier(uint256 n) external returns (uint256) {
        uint256 before = value;
        writeThenRead(n);
        return before + value;
    }
}
