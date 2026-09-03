//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: copyDynamic => 18
//@ run-call: copyFixed => 30
//@ run-call: copyMixed => 18
// ported-from: test/libsolidity/semanticTests/array/copying/nested_array_element_storage_to_storage.sol

pragma abicoder v2;

contract StorageNestedElementCopy {
    uint8[][][] srcDynamic;
    uint8[][][2] srcFixed;
    uint8[][2][] srcMixed = new uint8[][2][](1);

    uint8[][] dstDynamic;
    uint8[][2] dstMixed;

    constructor() {
        srcDynamic = new uint8[][][](2);
        srcDynamic[1] = new uint8[][](2);
        srcDynamic[1][0].push(3);
        srcDynamic[1][0].push(4);
        srcDynamic[1][1].push(5);
        srcDynamic[1][1].push(6);

        srcFixed[0] = new uint8[][](2);
        srcFixed[0][0].push(6);
        srcFixed[0][0].push(7);
        srcFixed[0][1].push(8);
        srcFixed[0][1].push(9);

        srcMixed[0][0].push(3);
        srcMixed[0][0].push(4);
        srcMixed[0][1].push(5);
        srcMixed[0][1].push(6);

    }

    function copyDynamic() external returns (uint256) {
        dstDynamic = srcDynamic[1];
        return dstDynamic[0][0] + dstDynamic[0][1] + dstDynamic[1][0] + dstDynamic[1][1];
    }

    function copyFixed() external returns (uint256) {
        dstDynamic = srcFixed[0];
        return dstDynamic[0][0] + dstDynamic[0][1] + dstDynamic[1][0] + dstDynamic[1][1];
    }

    function copyMixed() external returns (uint256) {
        dstMixed = srcMixed[0];
        return dstMixed[0][0] + dstMixed[0][1] + dstMixed[1][0] + dstMixed[1][1];
    }
}
