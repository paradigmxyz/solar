//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: structs((uint256),(uint256,uint256)) (66), (7, 119) => 7, 66
//@ run-call: staticArray(uint256[2][2]) [[8, 7], [6, 5]] => 8, 5
//@ run-call: dynamicArray(uint256[2][]) [[8, 7], [6, 5]] => 2, 8, 5
//@ run-call: dynamicWords(uint256[]) [8, 7] => [8, 7]
//@ run-call: dynamicFixed(uint256[2][]) [[8, 7], [6, 5]] => [[8, 7], [6, 5]]
//@ run-call: dynamicNarrow(uint8[]) [8, 7] => [8, 7]
//@ run-call: dynamicNested(uint256[][]) [[8], [7]] => [[8], [7]]
//@ run-call: dynamicEmptyAtEnd(uint256[]) [] => []
//@ run-call: dynamicWrappingEnd(uint256[]) [] => [0]
//@ run-call: returnedSlice(bytes) 0x01020304 => 0x01020304
//@ run-call: emptyUnassignedSlice() => 0
//@ run-call-fail: dynamicWords(uint256[]) []
//@ run-call-fail: dynamicFixed(uint256[2][]) []
//@ run-call-fail: dynamicNarrow(uint8[]) []
//@ run-call-fail: dynamicNested(uint256[][]) []
//@ run-call-fail: dynamicEmptyPastEnd(uint256[]) []
//@ run-call-fail: dynamicOverflow(uint256[]) [] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

contract AssemblyCalldataReferenceAssignment {
    struct One {
        uint256 value;
    }

    struct Two {
        uint256 value;
        uint256 other;
    }

    function structs(One calldata first, Two calldata second)
        external
        pure
        returns (uint256, uint256)
    {
        assembly {
            first := second
            second := 4
        }
        return (first.value, second.value);
    }

    function staticArray(uint256[2][2] calldata values)
        external
        pure
        returns (uint256, uint256)
    {
        assembly {
            values := 4
        }
        return (values[0][0], values[1][1]);
    }

    function dynamicArray(uint256[2][] calldata values)
        external
        pure
        returns (uint256, uint256, uint256)
    {
        assembly {
            values.offset := 0x44
            values.length := 2
        }
        return (values.length, values[0][0], values[1][1]);
    }

    function dynamicWords(uint256[] calldata values)
        external
        pure
        returns (uint256[] memory)
    {
        assembly {
            values.offset := 0x44
            values.length := 2
        }
        return values;
    }

    function dynamicFixed(uint256[2][] calldata values)
        external
        pure
        returns (uint256[2][] memory)
    {
        assembly {
            values.offset := 0x44
            values.length := 2
        }
        return values;
    }

    function dynamicNarrow(uint8[] calldata values)
        external
        pure
        returns (uint8[] memory)
    {
        assembly {
            values.offset := 0x44
            values.length := 2
        }
        return values;
    }

    function dynamicNested(uint256[][] calldata values)
        external
        pure
        returns (uint256[][] memory)
    {
        assembly {
            values.offset := 0x44
            values.length := 2
        }
        return values;
    }

    function dynamicOverflow(uint256[] calldata values)
        external
        pure
        returns (uint256[] memory)
    {
        assembly {
            values.offset := 0x44
            values.length := not(0)
        }
        return values;
    }

    function dynamicEmptyAtEnd(uint256[] calldata values)
        external
        pure
        returns (uint256[] memory)
    {
        assembly {
            values.offset := calldatasize()
            values.length := 0
        }
        return values;
    }

    function dynamicEmptyPastEnd(uint256[] calldata values)
        external
        pure
        returns (uint256[] memory)
    {
        assembly {
            values.offset := add(calldatasize(), 1)
            values.length := 0
        }
        return values;
    }

    function dynamicWrappingEnd(uint256[] calldata values)
        external
        pure
        returns (uint256[] memory)
    {
        assembly {
            values.offset := not(0)
            values.length := 1
        }
        return values;
    }

    function returnedSlice(bytes calldata values) external pure returns (bytes4) {
        bytes calldata result = slice(values, 0);
        bytes32 value;
        assembly {
            value := calldataload(result.offset)
        }
        return bytes4(value);
    }

    function emptyUnassignedSlice() external pure returns (uint256) {
        bytes calldata value;
        assembly {
            value.length := 0
        }
        return sliceLength(value);
    }

    function sliceLength(bytes calldata value) internal pure returns (uint256) {
        return value.length;
    }

    function slice(bytes calldata values, uint256 offset)
        internal
        pure
        returns (bytes calldata result)
    {
        assembly {
            result.offset := add(values.offset, offset)
            result.length := sub(values.length, offset)
        }
    }
}
