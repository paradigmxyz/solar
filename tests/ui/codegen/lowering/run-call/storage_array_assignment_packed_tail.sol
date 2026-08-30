//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: fixedLength() => 0
//@ run-call: dynamicLength(uint256) 8 => 4
//@ run-call: dynamicLength(uint256) 9 => 0
//@ run-call: exactSlotBoundary() => 4
// ported-from: test/libsolidity/semanticTests/array/copying/cleanup_during_multi_element_per_slot_copy.sol

contract StorageArrayAssignmentPackedTail {
    uint32[] private values;

    constructor() {
        values.push();
        values.push();
    }

    function fixedLength() external returns (uint256) {
        (values[1], values) = (4, [uint32(0)]);
        values = [uint32(0)];
        values.push();
        return values[1];
    }

    function dynamicLength(uint256 length) external returns (uint256) {
        for (uint256 i = values.length; i <= length; ++i) {
            values.push();
        }

        uint32[] memory replacement = new uint32[](length);
        (values[length], values) = (4, replacement);
        values = replacement;
        values.push();
        return values[length];
    }

    function exactSlotBoundary() external returns (uint256) {
        for (uint256 i = values.length; i < 9; ++i) {
            values.push();
        }

        uint32[8] memory replacement;
        (values[8], values) = (4, replacement);
        values = replacement;
        values.push();
        return values[8];
    }
}
