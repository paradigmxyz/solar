//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: structs((uint256),(uint256,uint256)) (66), (7, 119) => 7, 66
//@ run-call: staticArray(uint256[2][2]) [[8, 7], [6, 5]] => 8, 5
//@ run-call: dynamicArray(uint256[2][]) [[8, 7], [6, 5]] => 2, 8, 5

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
}
