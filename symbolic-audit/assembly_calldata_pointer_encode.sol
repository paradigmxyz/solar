// A calldata struct pointer assigned in assembly that points past
// calldatasize. Called with only the 4-byte selector.
// solc returns abi.encode of the zero-filled struct (calldataload past the end
// reads zeros); solar reverts with empty returndata.
contract AssemblyCalldataPointerEncode {
    struct Pair {
        uint8 a;
        uint8 b;
    }

    function encodeStruct() external pure returns (bytes memory) {
        return abi.encode(makeStruct());
    }

    function makeStruct() internal pure returns (Pair calldata value) {
        assembly {
            value := 4
        }
    }

    // Item 16: via-IR solc reverts (empty) when copying a static array or
    // `bytes` whose assembly-assigned calldata pointer runs past
    // calldatasize; legacy solc zero-fills. solar zero-fills and should
    // revert to match via-IR. Called with only the selector.
    function arrayCopy() external pure returns (uint256[2] memory) {
        uint256[2] calldata a;
        assembly {
            a := 4
        }
        return a;
    }

    function bytesCopy() external pure returns (bytes memory) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := 64
        }
        return b;
    }

    function bytesHash() external pure returns (bytes32) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := 64
        }
        return keccak256(b);
    }

    // Agree with solc: no copy, or a struct read through calldataload.
    function bytesLength() external pure returns (uint256) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := 64
        }
        return b.length;
    }

    function arrayIndex() external pure returns (uint256) {
        uint256[2] calldata a;
        assembly {
            a := 4
        }
        return a[1];
    }
}
