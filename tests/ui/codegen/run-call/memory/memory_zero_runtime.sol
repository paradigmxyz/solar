//@ run-call: bulkDefault 3 => 0
//@ run-call: bulkDeleteInPlace => 0, 9

contract MemoryZeroRuntime {
    function bulkDefault(uint256 i) public pure returns (uint256) {
        uint256[4] memory values;
        return values[i];
    }

    function bulkDeleteInPlace() public pure returns (uint256, uint256) {
        uint256[4] memory values;
        values[0] = 5;
        values[3] = 7;
        delete values;
        values[3] = 9;
        return (values[0], values[3]);
    }
}
