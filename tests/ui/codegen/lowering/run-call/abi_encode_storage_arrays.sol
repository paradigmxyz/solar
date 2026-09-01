//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: encodeDynamicStatic(uint256[2][]) [[123, 124], [223, 224], [323, 324]] => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000007b000000000000000000000000000000000000000000000000000000000000007c00000000000000000000000000000000000000000000000000000000000000df00000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001430000000000000000000000000000000000000000000000000000000000000144
//@ run-call: readDynamicStorage(uint256[2][]) [[123, 124], [223, 224], [323, 324]] => 123, 124, 223, 224, 323, 324
//@ run-call: encodeStaticStatic(uint256[2][2]) [[123, 124], [223, 224]] => 0x000000000000000000000000000000000000000000000000000000000000007b000000000000000000000000000000000000000000000000000000000000007c00000000000000000000000000000000000000000000000000000000000000df00000000000000000000000000000000000000000000000000000000000000e0
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_storage_array_v2.sol

contract AbiEncodeStorageArrays {
    uint256[2][] private dynamicValues;

    function encodeDynamicStatic(uint256[2][] calldata values)
        external
        returns (bytes memory)
    {
        dynamicValues = values;
        return abi.encode(dynamicValues);
    }

    function readDynamicStorage(uint256[2][] calldata values)
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        dynamicValues = values;
        return (
            dynamicValues[0][0],
            dynamicValues[0][1],
            dynamicValues[1][0],
            dynamicValues[1][1],
            dynamicValues[2][0],
            dynamicValues[2][1]
        );
    }

    uint256[2][2] private staticValues;

    function encodeStaticStatic(uint256[2][2] calldata values)
        external
        returns (bytes memory)
    {
        staticValues = values;
        return abi.encode(staticValues);
    }
}
