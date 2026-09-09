//@ codegen-matrix: standard
//@ run-call: known => 42
//@ run-call: unknown 42 => 42
//@ run-call-fail: unknown 41 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000056775617264000000000000000000000000000000000000000000000000000000
//@ run-call: record => 7
//@ run-call: nested => 42
//@ run-call-fail: overflow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
contract ClosedRuntime {
    uint256 private value;
    function known() external pure returns (uint256) {
        guard(42);
        return 42;
    }
    function unknown(uint256 x) external pure returns (uint256) {
        guard(x);
        return x;
    }
    function nested() external pure returns (uint256) {
        return validated(42);
    }
    function validated(uint256 x) internal pure returns (uint256) {
        guard(x);
        return x;
    }
    function guard(uint256 x) internal pure {
        require(x == 42, "guard");
    }
    function record() external returns (uint256) {
        write(7);
        return value;
    }
    function write(uint256 x) internal {
        value = x;
    }
    function overflow() external pure returns (uint256) {
        return increment(type(uint256).max);
    }
    function increment(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }
}
