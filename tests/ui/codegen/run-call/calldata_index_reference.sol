//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: dynamicFixed(uint256[2][]) [[1, 2], [3, 4]] => 0x44, 2, 0x84
//@ run-call: staticFixed(uint256[2][2]) [[1, 2], [3, 4]] => 0x44, 3, 4
//@ run-call: nestedDynamic(uint256[][]) [[1], [2, 3]] => 0xe4, 2, 3
//@ run-call: dynamicBytes(bytes[]) [0x01, 0x0203] => 0xe4, 2, 3
// ported-from: test/libsolidity/semanticTests/inlineAssembly/calldata_array_read.sol

contract CalldataIndexReference {
    function dynamicFixed(uint256[2][] calldata values)
        external
        pure
        returns (uint256 offset, uint256 length, uint256 elementOffset)
    {
        assembly {
            offset := values.offset
            length := values.length
        }
        uint256[2] calldata element = values[1];
        assembly {
            elementOffset := element
        }
    }

    function staticFixed(uint256[2][2] calldata values)
        external
        pure
        returns (uint256 offset, uint256 first, uint256 second)
    {
        uint256[2] calldata element = values[1];
        assembly {
            offset := element
        }
        return (offset, element[0], element[1]);
    }

    function nestedDynamic(uint256[][] calldata values)
        external
        pure
        returns (uint256 offset, uint256 length, uint256 last)
    {
        uint256[] calldata element = values[1];
        assembly {
            offset := element.offset
            length := element.length
        }
        last = element[1];
    }

    function dynamicBytes(bytes[] calldata values)
        external
        pure
        returns (uint256 offset, uint256 length, uint256 last)
    {
        bytes calldata element = values[1];
        assembly {
            offset := element.offset
            length := element.length
        }
        last = uint8(element[1]);
    }
}
