//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: powers => 1, 1, 1, 1, 1, 0
//@ run-call: quorum 2 => 2
//@ run-call-fail: quorum 1
//@ run-call: at [11,22], 1 => 22
//@ run-call-fail: at [11,22], 2 => Panic(0x32)
//@ run-call: memoryLayout => 2, 11, 22
//@ run-call: storageLayout => 2, 11, 22, 33
//@ run-call: selector => true

contract ConstantPowZero {
    uint256 constant ZERO = 0;
    uint256 constant ONE = ZERO ** 0;
    uint256[(0 ** 0) + 1] values;
    uint256 afterValues;

    function powers() external pure returns (uint256, uint256, uint256, int256, uint256, uint256) {
        return (0 ** 0, ONE, 1 ** 0, (-1) ** 0, 42 ** 0, 0 ** 1);
    }

    // CHECK-LABEL: fn @quorum(
    // CHECK: lt arg0, 2
    // CHECK: ret 2
    function quorum(uint256 approvals) external view returns (uint256) {
        require(approvals >= values.length);
        return values.length;
    }

    // CHECK-LABEL: fn @at(
    // CHECK-SAME: abi_params=[array<2, u256>, u256]
    function at(uint256[(0 ** 0) + 1] calldata input, uint256 index) external pure returns (uint256) {
        return input[index];
    }

    // CHECK-LABEL: fn @memoryLayout(
    // CHECK: alloc memoryfixedarray<2, 1>, exact, uninitialized, infallible, 64
    function memoryLayout() external pure returns (uint256, uint256, uint256) {
        uint256[(0 ** 0) + 1] memory input;
        input[0] = 11;
        input[1] = 22;
        return (input.length, input[0], input[1]);
    }

    // CHECK-LABEL: fn @storageLayout(
    // CHECK: sstore 2, 33
    function storageLayout() external returns (uint256, uint256, uint256, uint256) {
        values[0] = 11;
        values[1] = 22;
        afterValues = 33;
        return (values.length, values[0], values[1], afterValues);
    }

    function selector() external pure returns (bool) {
        return this.at.selector == bytes4(keccak256("at(uint256[2],uint256)"));
    }
}
