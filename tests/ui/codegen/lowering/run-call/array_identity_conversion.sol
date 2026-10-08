//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: fresh => 8
//@ run-call: aliased => 5
//@ run-call: fixedSize => 9
//@ run-call: calldataLength [(1, 2), (3, 4), (5, 6)] => 3

interface Types {
    struct Data {
        uint32 a;
        int128 b;
    }
}

contract ArrayIdentityConversion {
    function fresh() external pure returns (uint256) {
        Types.Data[] memory values = Types.Data[](new Types.Data[](1));
        values[0].a = 7;
        return values.length + values[0].a;
    }

    function aliased() external pure returns (uint256) {
        Types.Data[] memory values = new Types.Data[](1);
        Types.Data[] memory converted = Types.Data[](values);
        converted[0].a = 5;
        return values[0].a;
    }

    function fixedSize() external pure returns (uint256) {
        Types.Data[2] memory values;
        values[1].a = 9;
        return Types.Data[2](values)[1].a;
    }

    function calldataLength(Types.Data[] calldata values) external pure returns (uint256) {
        return Types.Data[](values).length;
    }
}
