//@ codegen-matrix: standard
//@ run-call-fail: arrayCopy => 0x
//@ run-call-fail: bytesCopy => 0x
//@ run-call-fail: bytesHash => 0x
//@ run-call: bytesLength => 64
//@ run-call: arrayIndex => 0
//@ run-call: structFirst => 0

contract AssemblyCalldataPointerEncode {
    struct Pair {
        uint8 a;
        uint8 b;
    }

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

    function structFirst() external pure returns (uint8) {
        return makeStruct().a;
    }

    function makeStruct() internal pure returns (Pair calldata value) {
        assembly {
            value := 4
        }
    }
}
