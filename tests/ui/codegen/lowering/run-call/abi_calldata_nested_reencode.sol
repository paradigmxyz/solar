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
//@[none] run-call: encode3d(uint8[][][]) [[[1, 2], [3]], [[4]]] => 0xbf429c49c4987f7dcc49672d7abcfcd6f9f2cd5af75a5241755444829ccd4300
//@[gas] run-call: encode3d(uint8[][][]) [[[1, 2], [3]], [[4]]] => 0xbf429c49c4987f7dcc49672d7abcfcd6f9f2cd5af75a5241755444829ccd4300
//@[size] run-call: encode3d(uint8[][][]) [[[1, 2], [3]], [[4]]] => 0xbf429c49c4987f7dcc49672d7abcfcd6f9f2cd5af75a5241755444829ccd4300
//@[none] run-call: encodeFixed(uint16[][2][]) [[[1, 2], [3]], [[4, 5], []]] => 0x8593cf123f2fc93dd066330b38b4dfe150a16aa2448c98c1c0952260f5b79f18
//@[gas] run-call: encodeFixed(uint16[][2][]) [[[1, 2], [3]], [[4, 5], []]] => 0x8593cf123f2fc93dd066330b38b4dfe150a16aa2448c98c1c0952260f5b79f18
//@[size] run-call: encodeFixed(uint16[][2][]) [[[1, 2], [3]], [[4, 5], []]] => 0x8593cf123f2fc93dd066330b38b4dfe150a16aa2448c98c1c0952260f5b79f18
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_nested_array_reencode_v2.sol

contract AbiCalldataNestedReencode {
    function encode3d(uint8[][][] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encode(values));
    }

    function encodeFixed(uint16[][2][] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encode(values));
    }
}
