//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard amsterdam
//@[amsterdam] compile-flags: -O gas --evm-version amsterdam
//@ run-call: SharedArrayEncoding::sharedArrayEncoding => 1, 2, 9, 2, 2
//@ run-call: init (0x000000000000000000000000000000000000beef, 7, "name", 0x0102), 0x0000000000000000000000000000000000000003 => 10
//@ run-call: tail (0x000000000000000000000000000000000000beef, 7, "name", 0x0102) => 0x02
//@ run-call: allocatedAggregates => 2, 18, 5, 2, 8
//@ run-call: fixedAllocatedAggregates => 2, 12, 5, 2, 9
//@ run-call: zeroAllocatedAggregates => 2, 0, 0, 0
//@ run-call: zeroDynamicAggregates => 2, 0, 2, 0
//@ run-call: zeroDynamicAggregatesAfterScratch => 0, 1
//@ run-call: zeroNestedDynamicAggregateAfterScratch => 0

struct InitInput {
    address asset;
    uint8 decimals;
    string name;
    bytes params;
}

struct Allocated {
    uint256 id;
    bytes data;
    uint256[] values;
}

contract SharedArrayEncoding {
    enum Mode { A, B, C }

    struct Wide {
        uint256 a;
        uint256 b;
        address c;
        address d;
        bytes e;
        bytes f;
        bytes g;
        Allocated[] nested;
        Mode h;
        uint256 i;
    }

    function encodeArrays(Wide[] memory values) external pure returns (Wide[] memory, Wide[] memory, bytes memory) {
        return (values, values, hex"1234");
    }

    function sharedArrayEncoding() external view returns (uint256, uint256, uint256, uint256, uint256) {
        Wide[] memory values = new Wide[](2);
        Allocated[] memory nested = new Allocated[](1);
        nested[0].id = 9;
        nested[0].data = hex"aabb";
        nested[0].values = new uint256[](1);
        nested[0].values[0] = 42;
        values[0] = Wide(1, 2, address(3), address(4), hex"0506", hex"07", hex"08", nested, Mode.B, 9);
        values[1] = Wide(10, 11, address(12), address(13), hex"1415", hex"16", hex"17", new Allocated[](0), Mode.C, 18);
        (Wide[] memory first, Wide[] memory second, bytes memory tail) = this.encodeArrays(values);
        return (uint256(first[0].h), uint256(second[1].h), second[0].nested[0].id, first[0].e.length, tail.length);
    }

}

contract AbiDynamicStruct {
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    function tail(InitInput calldata input) external pure returns (bytes memory) {
        return input.params[1:];
    }

    function allocatedAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256)
    {
        Allocated[] memory values = new Allocated[](2);
        values[0].id = 7;
        values[0].data = hex"0102";
        values[0].values = new uint256[](2);
        values[0].values[0] = 3;
        values[0].values[1] = 4;
        values[1].id = 11;
        values[1].data = hex"030405";
        values[1].values = new uint256[](1);
        values[1].values[0] = 8;
        return (
            values.length,
            values[0].id + values[1].id,
            values[0].data.length + values[1].data.length,
            values[0].values.length,
            values[1].values[0]
        );
    }

    function fixedAllocatedAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256)
    {
        Allocated[2] memory values;
        values[0].id = 5;
        values[0].data = hex"0102";
        values[0].values = new uint256[](1);
        values[0].values[0] = 6;
        values[1].id = 7;
        values[1].data = hex"030405";
        values[1].values = new uint256[](2);
        values[1].values[0] = 8;
        values[1].values[1] = 9;
        return (
            values.length,
            values[0].id + values[1].id,
            values[0].data.length + values[1].data.length,
            values[1].values.length,
            values[1].values[1]
        );
    }

    function zeroAllocatedAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        Allocated[] memory values = new Allocated[](2);
        return (
            values.length,
            values[0].id + values[1].id,
            values[0].data.length + values[1].data.length,
            values[0].values.length + values[1].values.length
        );
    }

    function zeroDynamicAggregates()
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        bytes[] memory bytesValues = new bytes[](2);
        uint256[][] memory arrayValues = new uint256[][](2);
        return (
            bytesValues.length,
            bytesValues[0].length + bytesValues[1].length,
            arrayValues.length,
            arrayValues[0].length + arrayValues[1].length
        );
    }

    function zeroDynamicAggregatesAfterScratch() external pure returns (uint256, uint256) {
        bytes[] memory values = new bytes[](1);
        assembly {
            mstore(0, 99)
        }
        return (values[0].length, values.length);
    }

    function zeroNestedDynamicAggregateAfterScratch() external pure returns (uint256) {
        Allocated[] memory values = new Allocated[](1);
        assembly {
            mstore(0, 99)
        }
        return values[0].data.length;
    }
}
