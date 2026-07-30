//@compile-flags: -Zcodegen -O none -Zdump=mir

contract EventIndexedAggregate {
    event IndexedArray(uint256[2] indexed values);

    function emitArray(uint256[2] memory values) external {
        emit IndexedArray(values); //~ ERROR: codegen does not support indexed event aggregate encoding yet
    }
}
