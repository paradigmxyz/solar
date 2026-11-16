//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f [[[42], [23]]], 0, 0, 0 => 1, 2, 1, 42
//@ run-call: f [[[42], [23]]], 0, 1, 0 => 1, 2, 1, 23
//@ run-call: f [[[42], [23, 17]]], 0, 1, 0 => 1, 2, 2, 23
//@ run-call: f [[[42], [23, 17]]], 0, 1, 1 => 1, 2, 2, 17
//@ run-call-fail: f [[[42], [23]]], 1, 0, 0 => Panic(0x32)
//@ run-call-fail: f [[[42], [23]]], 0, 2, 0 => Panic(0x32)
//@ run-call-fail: f [[[42], [23]]], 0, 0, 1 => Panic(0x32)
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_indexing_3d_v2.sol

pragma abicoder v2;

contract AbiCalldataIndexing3d {
    function f(
        uint256[][2][] calldata values,
        uint256 i,
        uint256 j,
        uint256 k
    ) external pure returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        a = values.length;
        b = values[i].length;
        c = values[i][j].length;
        d = values[i][j][k];
    }
}
