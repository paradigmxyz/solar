//@ codegen-matrix: standard
//@ run-call: fmpIsStable => true, 0, 0
//@ run-call: nullArrayElementsEncodeAsEmpty => true

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

    function nullArrayElementsEncodeAsEmpty() external pure returns (bool) {
        bytes[] memory bytesValues = new bytes[](2);
        bytes memory cleanBytes = abi.encode(bytesValues);
        assembly {
            mstore(0, not(0))
        }
        bytes memory dirtyBytes = abi.encode(bytesValues);

        assembly {
            mstore(0, 0)
        }
        uint256[][] memory arrayValues = new uint256[][](2);
        bytes memory cleanArrays = abi.encode(arrayValues);
        assembly {
            mstore(0, not(0))
        }
        bytes memory dirtyArrays = abi.encode(arrayValues);

        return keccak256(cleanBytes) == keccak256(dirtyBytes)
            && keccak256(cleanArrays) == keccak256(dirtyArrays);
    }
}
