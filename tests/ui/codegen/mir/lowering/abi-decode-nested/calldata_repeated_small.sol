//@ revisions: gas size
//@ compile-flags: -Zdump=mir
//@[gas] compile-flags: -Ogas
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -Osize
//@[size] filecheck: --check-prefix=SIZE
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: two 0x000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000006 => 9

// A gas build decodes each copy of a small type that a calldata `abi.decode` repeats inline,
// which saves the call of a shared decoder; a size build shares the decoder.
contract C {
    // GAS-LABEL: fn @two(
    // GAS-NOT: decode_calldata_slice_type
    // SIZE-LABEL: fn @two(
    // SIZE: icall @decode_calldata_slice_type
    // SIZE: icall @decode_calldata_slice_type
    function two(bytes calldata data) external pure returns (uint256) {
        (uint256[] memory a, uint256[] memory b) = abi.decode(data, (uint256[], uint256[]));
        return a[0] + b[1];
    }
}
