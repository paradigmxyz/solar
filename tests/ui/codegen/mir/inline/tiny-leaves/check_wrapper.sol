//@ codegen-matrix: standard
//@ run-call: guarded 9 => 9
//@ run-call-fail: guarded 0 => 0x
//@ run-call-fail: guarded 10 => 0x
//@ run-call: asserted 7 => 7
//@ run-call-fail: asserted 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000001

contract CheckWrapper {
    uint256 private stored;

    function ensure(bool condition) private pure {
        require(condition);
    }

    function invariant(bool condition) private pure {
        assert(condition);
    }

    function guarded(uint256 value) external returns (uint256) {
        ensure(value != 0);
        stored = value;
        ensure(stored < 10);
        return stored;
    }

    function asserted(uint256 value) external pure returns (uint256) {
        invariant(value != 0);
        return value;
    }
}
