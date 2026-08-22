//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: structs((uint256),(uint256,uint256)) (66), (7, 119) => 7, 66
//@[gas] run-call: structs((uint256),(uint256,uint256)) (66), (7, 119) => 7, 66
//@[size] run-call: structs((uint256),(uint256,uint256)) (66), (7, 119) => 7, 66
//@[none] run-call: staticArray(uint256[2][2]) [[8, 7], [6, 5]] => 8, 5
//@[gas] run-call: staticArray(uint256[2][2]) [[8, 7], [6, 5]] => 8, 5
//@[size] run-call: staticArray(uint256[2][2]) [[8, 7], [6, 5]] => 8, 5
//@[none] run-call: dynamicArray(uint256[2][]) [[8, 7], [6, 5]] => 2, 8, 5
//@[gas] run-call: dynamicArray(uint256[2][]) [[8, 7], [6, 5]] => 2, 8, 5
//@[size] run-call: dynamicArray(uint256[2][]) [[8, 7], [6, 5]] => 2, 8, 5
//@[none] run-call: dynamicWords(uint256[]) [8, 7] => [8, 7]
//@[gas] run-call: dynamicWords(uint256[]) [8, 7] => [8, 7]
//@[size] run-call: dynamicWords(uint256[]) [8, 7] => [8, 7]
//@[none] run-call: dynamicFixed(uint256[2][]) [[8, 7], [6, 5]] => [[8, 7], [6, 5]]
//@[gas] run-call: dynamicFixed(uint256[2][]) [[8, 7], [6, 5]] => [[8, 7], [6, 5]]
//@[size] run-call: dynamicFixed(uint256[2][]) [[8, 7], [6, 5]] => [[8, 7], [6, 5]]
//@[none] run-call: dynamicNarrow(uint8[]) [8, 7] => [8, 7]
//@[gas] run-call: dynamicNarrow(uint8[]) [8, 7] => [8, 7]
//@[size] run-call: dynamicNarrow(uint8[]) [8, 7] => [8, 7]
//@[none] run-call: dynamicNested(uint256[][]) [[8], [7]] => [[8], [7]]
//@[gas] run-call: dynamicNested(uint256[][]) [[8], [7]] => [[8], [7]]
//@[size] run-call: dynamicNested(uint256[][]) [[8], [7]] => [[8], [7]]
//@[none] run-call: dynamicEmptyAtEnd(uint256[]) [] => []
//@[gas] run-call: dynamicEmptyAtEnd(uint256[]) [] => []
//@[size] run-call: dynamicEmptyAtEnd(uint256[]) [] => []
//@[none] run-call: dynamicWrappingEnd(uint256[]) [] => [0]
//@[gas] run-call: dynamicWrappingEnd(uint256[]) [] => [0]
//@[size] run-call: dynamicWrappingEnd(uint256[]) [] => [0]
//@[none] run-call-fail: dynamicWords(uint256[]) []
//@[gas] run-call-fail: dynamicWords(uint256[]) []
//@[size] run-call-fail: dynamicWords(uint256[]) []
//@[none] run-call-fail: dynamicFixed(uint256[2][]) []
//@[gas] run-call-fail: dynamicFixed(uint256[2][]) []
//@[size] run-call-fail: dynamicFixed(uint256[2][]) []
//@[none] run-call-fail: dynamicNarrow(uint8[]) []
//@[gas] run-call-fail: dynamicNarrow(uint8[]) []
//@[size] run-call-fail: dynamicNarrow(uint8[]) []
//@[none] run-call-fail: dynamicNested(uint256[][]) []
//@[gas] run-call-fail: dynamicNested(uint256[][]) []
//@[size] run-call-fail: dynamicNested(uint256[][]) []
//@[none] run-call-fail: dynamicEmptyPastEnd(uint256[]) []
//@[gas] run-call-fail: dynamicEmptyPastEnd(uint256[]) []
//@[size] run-call-fail: dynamicEmptyPastEnd(uint256[]) []
//@[none] run-call-fail: dynamicOverflow(uint256[]) [] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@[gas] run-call-fail: dynamicOverflow(uint256[]) [] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@[size] run-call-fail: dynamicOverflow(uint256[]) [] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

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
}
