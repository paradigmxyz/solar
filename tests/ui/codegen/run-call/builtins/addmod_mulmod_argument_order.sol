//@ codegen-matrix: standard
// Legacy codegen evaluates the operands right to left; via-IR, which we follow, left to right.
// See https://docs.soliditylang.org/en/v0.8.38/ir-breaking-changes.html.
//@ run-call: addmodBump => 6
//@ run-call: mulmodBump => 9
//@ run-call: addmodTrace => 123
//@ run-call: mulmodTrace => 123
//@ run-call: addmodNestedTrace => 1234
//@ run-call: addmodReadsTrace => 2
//@ run-call: mulmodReadsTrace => 2
//@ run-call-fail: addmodZero => Panic(0x12)
//@ run-call-fail: addmodUnderflowBeforeZero => Panic(0x11)
//@ run-call-fail: mulmodIndexBeforeZero => Panic(0x32)
//@ run-call-fail: mulmodFirstOperandPanicsFirst => Panic(0x32)

contract AddmodMulmodArgumentOrder {
    uint256 n = 2;
    uint256 trace;
    uint256 zero;
    uint256 outOfBounds = 2;
    uint256[2] pair;

    function addmodBump() external returns (uint256) {
        return addmod(bump(), n, 100);
    }

    function mulmodBump() external returns (uint256) {
        return mulmod(bump(), n, 100);
    }

    function addmodTrace() external returns (uint256) {
        addmod(step(1), step(2), step(3));
        return trace;
    }

    function mulmodTrace() external returns (uint256) {
        mulmod(step(1), step(2), step(3));
        return trace;
    }

    function addmodNestedTrace() external returns (uint256) {
        addmod(addmod(step(1), step(2), 100), step(3), step(4) + 100);
        return trace;
    }

    function addmodReadsTrace() external returns (uint256) {
        return addmod(step(1), trace, 100);
    }

    function mulmodReadsTrace() external returns (uint256) {
        return mulmod(step(1), trace + 1, 100);
    }

    function addmodZero() external view returns (uint256) {
        return addmod(1, 1, zero);
    }

    // The underflow in the first operand panics before the zero modulus is checked.
    function addmodUnderflowBeforeZero() external returns (uint256) {
        return addmod(step(1) - 5, 1, zero);
    }

    // The out-of-bounds read in the first operand panics before the zero modulus is checked.
    function mulmodIndexBeforeZero() external view returns (uint256) {
        return mulmod(pair[outOfBounds], 1, zero);
    }

    // Both operands panic, so the first one evaluated decides the code: 0x32, not 0x11.
    function mulmodFirstOperandPanicsFirst() external returns (uint256) {
        return mulmod(pair[outOfBounds], step(1) - 5, 7);
    }

    function bump() internal returns (uint256) {
        n += 1;
        return n;
    }

    function step(uint256 id) internal returns (uint256) {
        trace = trace * 10 + id;
        return id;
    }
}
