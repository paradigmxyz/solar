//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: swap => 2, 1
//@ run-call: repeatedMemoryTarget => 4, 17, 1
//@ run-call: repeatedStateTarget => 3, 1
//@ run-call: repeatedCallTarget => 1
//@ run-call: sideEffectfulTargets => 1, 2, 2
//@ run-call: storageBytesTargets => 0x61, 0x62
// ported-from: test/libsolidity/semanticTests/viaYul/local_tuple_assignment.sol
// ported-from: test/libsolidity/semanticTests/viaYul/tuple_evaluation_order.sol

contract TupleAssignmentOrder {
    uint256 private cursor;
    uint256 private initialValue = 17;
    uint256 private stateTarget;
    bytes private data = "zz";

    function swap() external pure returns (uint256, uint256) {
        uint256 a = 1;
        uint256 b = 2;
        (a, b) = (b, a);
        return (a, b);
    }

    function repeatedMemoryTarget() external view returns (uint256, uint256, uint256) {
        uint256[3] memory values;
        (values[0], values[1], , values[2], values[0]) = (1, initialValue, 3, 4, 42);
        return (values[2], values[1], values[0]);
    }

    function repeatedStateTarget() external returns (uint256, uint256) {
        (stateTarget, stateTarget, stateTarget) = (setCursor(1), setCursor(2), setCursor(3));
        return (cursor, stateTarget);
    }

    function repeatedCallTarget() external pure returns (uint256 target) {
        (target, target, target) = threeValues();
    }

    function sideEffectfulTargets() external returns (uint256, uint256, uint256) {
        uint256[2] memory values;
        (values[nextIndex()], values[nextIndex()]) = (1, 2);
        return (values[0], values[1], cursor);
    }

    function storageBytesTargets() external returns (bytes1, bytes1) {
        (data[0], data[1]) = (bytes1("a"), bytes1("b"));
        return (data[0], data[1]);
    }

    function setCursor(uint256 value) internal returns (uint256) {
        cursor = value;
        return value;
    }

    function nextIndex() internal returns (uint256 value) {
        value = cursor;
        cursor++;
    }

    function threeValues() internal pure returns (uint256, uint256, uint256) {
        return (1, 2, 3);
    }
}
