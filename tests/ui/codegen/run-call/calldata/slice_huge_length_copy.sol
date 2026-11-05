//@ codegen-matrix: standard
//@ run-call-fail: bytesCopy() => Panic(0x41)
//@ run-call-fail: stringCopy() => Panic(0x41)
//@ run-call-fail: wordArrayCopy() => Panic(0x41)
//@ run-call-fail: wordArrayBelowLimit() => Panic(0x41)
//@ run-call-fail: narrowArrayCopy() => Panic(0x41)
//@ run-call-fail: nestedArrayCopy() => Panic(0x41)
//@ run-call-fail: structArrayCopy() => Panic(0x41)
//@ run-call-fail: bytesSliceCopy() => Panic(0x41)
//@ run-call-fail: arraySliceCopy() => Panic(0x41)
//@ run-call-fail: internalBytesCopy() => Panic(0x41)
//@ run-call-fail: internalArrayCopy() => Panic(0x41)

// An assembly-set calldata length bypasses ABI decoding, so copying the slice to memory must
// panic on the allocation size before it checks the calldata range, as solc does.
contract C {
    struct S {
        uint256 a;
        uint256 b;
    }

    function bytesCopy() external pure returns (bytes memory) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := shl(64, 1)
        }
        return b;
    }

    function stringCopy() external pure returns (string memory) {
        string calldata s;
        assembly {
            s.offset := 4
            s.length := shl(64, 1)
        }
        return s;
    }

    function wordArrayCopy() external pure returns (uint256[] memory) {
        uint256[] calldata a;
        assembly {
            a.offset := 4
            a.length := shl(64, 1)
        }
        return a;
    }

    // The length is below the `2**64` limit but its size is not allocatable.
    function wordArrayBelowLimit() external pure returns (uint256[] memory) {
        uint256[] calldata a;
        assembly {
            a.offset := 4
            a.length := shl(59, 1)
        }
        return a;
    }

    function narrowArrayCopy() external pure returns (uint8[] memory) {
        uint8[] calldata a;
        assembly {
            a.offset := 4
            a.length := shl(64, 1)
        }
        return a;
    }

    function nestedArrayCopy() external pure returns (uint256[][] memory) {
        uint256[][] calldata a;
        assembly {
            a.offset := 4
            a.length := shl(64, 1)
        }
        return a;
    }

    function structArrayCopy() external pure returns (S[] memory) {
        S[] calldata a;
        assembly {
            a.offset := 4
            a.length := shl(64, 1)
        }
        return a;
    }

    function bytesSliceCopy() external pure returns (bytes memory) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := shl(65, 1)
        }
        return b[1:];
    }

    function arraySliceCopy() external pure returns (uint256[] memory) {
        uint256[] calldata a;
        assembly {
            a.offset := 4
            a.length := shl(64, 1)
        }
        return a[1:];
    }

    function copyBytes(bytes calldata b) internal pure returns (bytes memory) {
        return b;
    }

    function internalBytesCopy() external pure returns (bytes memory) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := shl(64, 1)
        }
        return copyBytes(b);
    }

    function copyArray(uint256[] calldata a) internal pure returns (uint256[] memory) {
        return a;
    }

    function internalArrayCopy() external pure returns (uint256[] memory) {
        uint256[] calldata a;
        assembly {
            a.offset := 4
            a.length := shl(64, 1)
        }
        return copyArray(a);
    }
}
