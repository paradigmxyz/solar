//@ run-call: length(uint256[]) [7, 11, 13] => 3

contract CalldataTupleAssignmentToMemory {
    function length(uint256[] calldata values) external pure returns (uint256) {
        return lengthInHelper(values);
    }

    function lengthInHelper(uint256[] calldata values) public pure returns (uint256) {
        uint256[] memory copied;
        bytes memory ignored;
        (copied, ignored) = (values, hex"");
        return copied.length;
    }
}
