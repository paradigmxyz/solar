//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: readNarrow [[[42]]] => 44
//@ run-call: readWide [[[42], [43]]] => 87
//@ run-call-fail: 0x49c9edea0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000002a => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_dynamic_static_dynamic_v2.sol

pragma abicoder v2;

contract AbiCalldataDynamicStaticDynamic {
    function readNarrow(uint8[][1][] calldata value) external pure returns (uint256) {
        return value.length + value[0][0].length + value[0][0][0];
    }

    function readWide(uint256[][2][] calldata value) external pure returns (uint256) {
        return value.length + value[0][0].length + value[0][0][0] + value[0][1][0];
    }
}
