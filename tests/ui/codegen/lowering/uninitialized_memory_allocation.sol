//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: fixedArray() => 96
//@ run-call: structure() => 64
//@ run-call: aliasDoesNotAllocate() => 0
//@ run-call: dynamicDefaults() => 0, 96, 96, 0
//@ run-call: nestedDynamicDefaults() => 96, 96, 96, true
//@ run-call: defaultBytesReturn() => 0x
//@ run-call: defaultThroughInternalCall() => true
//@ run-call: fixedArrayDynamicDefaults() => 64, 96, 96
//@ run-call: nestedStaticDefaults() => 192
//@ run-call: literalBytes() => 0x31
//@ run-call: literalAllocation() => 64

contract UninitializedMemoryAllocation {
    struct Pair {
        uint256 first;
        uint256 second;
    }

    struct DynamicFields {
        uint256 value;
        bytes data;
        uint8[] values;
    }

    struct StaticFields {
        Pair pair;
        uint256[2] values;
    }

    function fixedArray() external pure returns (uint256) {
        uint256 before = freeMemoryPointer();
        uint256[3] memory unused;
        return freeMemoryPointer() - before;
    }

    function structure() external pure returns (uint256) {
        uint256 before = freeMemoryPointer();
        Pair memory unused;
        return freeMemoryPointer() - before;
    }

    function aliasDoesNotAllocate() external pure returns (uint256) {
        uint256[3] memory original;
        uint256 before = freeMemoryPointer();
        uint256[3] memory alias_ = original;
        return freeMemoryPointer() - before;
    }

    function dynamicDefaults()
        external
        pure
        returns (uint256 allocated, uint256 bytesPointer, uint256 arrayPointer, uint256 lengths)
    {
        uint256 before = freeMemoryPointer();
        bytes memory data;
        uint256[] memory values;
        allocated = freeMemoryPointer() - before;
        assembly {
            bytesPointer := data
            arrayPointer := values
        }
        lengths = data.length + values.length;
    }

    function nestedDynamicDefaults()
        external
        pure
        returns (uint256 allocated, uint256 bytesPointer, uint256 arrayPointer, bool nextIsFree)
    {
        uint256 before = freeMemoryPointer();
        DynamicFields memory fields;
        allocated = freeMemoryPointer() - before;
        assembly {
            bytesPointer := mload(add(fields, 0x20))
            arrayPointer := mload(add(fields, 0x40))
        }
        uint256 next = freeMemoryPointer();
        bytes memory data = new bytes(1);
        uint256 dataPointer;
        assembly {
            dataPointer := data
        }
        nextIsFree = dataPointer == next;
    }

    function defaultBytesReturn() external pure returns (bytes memory data) {}

    function defaultThroughInternalCall() external pure returns (bool) {
        bytes memory data;
        return isEmpty(data);
    }

    function fixedArrayDynamicDefaults()
        external
        pure
        returns (uint256 allocated, uint256 firstPointer, uint256 secondPointer)
    {
        uint256 before = freeMemoryPointer();
        uint256[][2] memory values;
        allocated = freeMemoryPointer() - before;
        assembly {
            firstPointer := mload(values)
            secondPointer := mload(add(values, 0x20))
        }
    }

    function nestedStaticDefaults() external pure returns (uint256) {
        uint256 before = freeMemoryPointer();
        StaticFields memory fields;
        assembly {
            pop(fields)
        }
        return freeMemoryPointer() - before;
    }

    function literalBytes() public pure returns (bytes memory) {
        return "1";
    }

    function literalAllocation() external pure returns (uint256) {
        uint256 before = freeMemoryPointer();
        literalBytes();
        return freeMemoryPointer() - before;
    }

    function isEmpty(bytes memory data) private pure returns (bool) {
        return data.length == 0;
    }

    function freeMemoryPointer() private pure returns (uint256 pointer) {
        assembly {
            pointer := mload(0x40)
        }
    }
}
