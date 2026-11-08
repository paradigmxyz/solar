//@ codegen-matrix: standard
//@ run-call: fmpIsStable => true, 0, 0
//@ run-call: nullArrayElementsEncodeAsEmpty => true
//@ run-call: conditionalLiteral false => 0
//@ run-call: conditionalLiteral true => 64
//@ run-call: overlappingLiteral false => 0
//@ run-call: overlappingLiteral true => 64
//@ run-call: sharedWordLiteral 0 => 64
//@ run-call: sharedWordLiteral 1 => 64
//@ run-call: sharedWordLiteral 2 => 64
//@ run-call: sharedWordLiteral 3 => 0
//@ run-call: sharedLongLiteral 0 => 96
//@ run-call: sharedLongLiteral 1 => 96
//@ run-call: sharedLongLiteral 2 => 96
//@ run-call: sharedLongLiteral 3 => 96
//@ run-call: sharedLongLiteral 4 => 0

contract UninitializedMemoryReferences {
    function sharedWordLiteral(uint256 choice) external returns (uint256 delta) {
        uint256 before;
        assembly { before := mload(0x40) }
        if (choice == 0) {
            bytes memory data = hex"5678";
            assembly { log0(add(data, 32), 2) }
        } else if (choice == 1) {
            bytes memory data = hex"5678";
            assembly { log0(add(data, 32), 2) }
        } else if (choice == 2) {
            bytes memory data = hex"5678";
            assembly { log0(add(data, 32), 2) }
        }
        assembly { delta := sub(mload(0x40), before) }
    }

    function sharedLongLiteral(uint256 choice) external returns (uint256 delta) {
        uint256 before;
        assembly { before := mload(0x40) }
        if (choice == 0) {
            bytes memory data = hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff001122";
            assembly { log0(add(data, 32), 34) }
        } else if (choice == 1) {
            bytes memory data = hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff001122";
            assembly { log0(add(data, 32), 34) }
        } else if (choice == 2) {
            bytes memory data = hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff001122";
            assembly { log0(add(data, 32), 34) }
        } else if (choice == 3) {
            bytes memory data = hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff001122";
            assembly { log0(add(data, 32), 34) }
        }
        assembly { delta := sub(mload(0x40), before) }
    }

    function conditionalLiteral(bool take) external returns (uint256 delta) {
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        if (take) {
            bytes memory data = hex"1234";
            assembly {
                log0(add(data, 32), 2)
            }
        }
        assembly {
            delta := sub(mload(0x40), before)
        }
    }

    function overlappingLiteral(bool take) external returns (uint256 delta) {
        uint256 before;
        assembly {
            before := shr(8, mload(0x41))
        }
        if (take) {
            bytes memory data = hex"abcd";
            assembly {
                log0(add(data, 32), 2)
            }
        }
        assembly {
            delta := sub(shr(8, mload(0x41)), before)
        }
    }

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
