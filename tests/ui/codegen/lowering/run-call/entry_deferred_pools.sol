//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: one 7 => 0xa66cc928b5edb82af9bd49922954155ab7b0942694bea4ce44661d9a8736c688
//@ run-call: two 7, 9 => 0xae6299332bcd708cd60e3a8defa55de28078a50a4cf2b3de3a546253240ff9e1
//@ run-call: preserved 7, 9 => true
//@ run-call: preserved 0, 0 => true

contract EntryDeferredPools {
    function one(uint256 value) public pure returns (bytes32) {
        return keccak256(abi.encode(value));
    }

    function two(uint256 left, uint256 right) public pure returns (bytes32) {
        return keccak256(abi.encode(left, right));
    }

    function preserved(uint256 left, uint256 right) external pure returns (bool) {
        // This allocation must survive both internal calls and their separate buffers.
        bytes memory buffer = abi.encode(left, right, left, right);
        bytes32 before = keccak256(buffer);
        bytes32 first = one(left);
        bytes32 second = two(left, right);
        return keccak256(buffer) == before && first != second;
    }
}
