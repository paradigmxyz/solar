//@ run-call: fmpIsStable() => true, 0, 0

contract UninitializedMemoryReferences {
    function fmpIsStable() external pure returns (bool stable, uint256 bytesLen, uint256 arrayLen) {
        uint256 before;
        uint256 afterValue;
        assembly {
            before := mload(0x40)
        }
        bytes memory data;
        uint256[] memory values;
        assembly {
            afterValue := mload(0x40)
        }
        return (before == afterValue, data.length, values.length);
    }
}
